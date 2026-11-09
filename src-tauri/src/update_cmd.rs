//! Actualizaciones con el plugin oficial de Tauri, desde Rust: así hay progreso real y la descarga se puede cancelar.
//!
//! El plugin comprueba la firma del paquete al terminar la descarga y otra vez justo antes de instalar. En Windows, `install` lanza el
//! instalador NSIS y cierra la aplicación; por eso se separa en dos pasos: se descarga y verifica, y se instala cuando el usuario reinicia.
//! Nada de esto toca las preferencias ni el índice: viven fuera de la carpeta de instalación.

use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use tauri_plugin_updater::{Error, Update, UpdaterExt};

const CHECK_TIMEOUT: Duration = Duration::from_secs(20);
/// Tiempo máximo de la descarga completa: holgado para conexiones lentas.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const PROGRESS_EVERY: Duration = Duration::from_millis(80);

type Abort = Box<dyn FnOnce() + Send>;

#[derive(Default)]
pub struct UpdateState {
    found: Mutex<Option<Update>>,
    ready: Mutex<Option<(Update, Vec<u8>)>>,
    abort: Mutex<Option<Abort>>,
    cancelled: AtomicBool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub notes: Option<String>,
    pub size: Option<u64>,
}

#[derive(Serialize)]
pub struct UpdateFailure {
    /// "offline" | "signature" | "cancelled" | "server" | "install"
    pub kind: &'static str,
    pub message: String,
}

#[derive(Serialize, Clone)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum UpdateEvent {
    Progress { done: u64, total: Option<u64> },
    Verifying,
}

fn describe(e: Error) -> UpdateFailure {
    let (kind, message): (&'static str, &str) = match &e {
        Error::Reqwest(r) if r.is_connect() || r.is_timeout() => ("offline", "No se pudo conectar. Comprueba tu conexión a Internet."),
        Error::Reqwest(_) | Error::Network(_) => ("server", "Se interrumpió la conexión con el servidor de actualizaciones."),
        Error::Minisign(_) | Error::Base64(_) | Error::SignatureUtf8(_) | Error::SignedVersionMismatch { .. } | Error::MissingSignedVersion => {
            ("signature", "El paquete descargado no tiene una firma válida, así que se ha descartado. No se instaló nada.")
        }
        Error::Io(_) | Error::Tauri(_) => ("install", "No se pudo iniciar la instalación. LFind sigue funcionando con la versión actual."),
        _ => ("server", "El servidor de actualizaciones no dio una respuesta válida."),
    };
    UpdateFailure { kind, message: message.into() }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[tauri::command]
pub fn app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

/// Consulta el manifiesto publicado. `None`: no hay una versión superior a la instalada.
#[tauri::command]
pub async fn update_check(app: AppHandle, state: State<'_, UpdateState>) -> Result<Option<UpdateInfo>, UpdateFailure> {
    let updater = app.updater_builder().timeout(CHECK_TIMEOUT).build().map_err(describe)?;
    let Some(mut update) = updater.check().await.map_err(describe)? else {
        *lock(&state.found) = None;
        return Ok(None);
    };
    update.timeout = Some(DOWNLOAD_TIMEOUT);
    let info = UpdateInfo { version: update.version.clone(), notes: update.body.clone(), size: update.raw_json.get("sizeBytes").and_then(|v| v.as_u64()) };
    *lock(&state.found) = Some(update);
    Ok(Some(info))
}

/// Descarga y verifica. No instala: el paquete queda en memoria hasta que el usuario decide reiniciar.
#[tauri::command]
pub async fn update_download(on_event: Channel<UpdateEvent>, state: State<'_, UpdateState>) -> Result<(), UpdateFailure> {
    let update = lock(&state.found).clone().ok_or_else(|| UpdateFailure { kind: "server", message: "Vuelve a comprobar si hay actualizaciones.".into() })?;
    state.cancelled.store(false, Ordering::SeqCst);
    let worker = update.clone();
    let task = tauri::async_runtime::spawn(async move {
        let (mut done, mut last) = (0u64, Instant::now() - PROGRESS_EVERY);
        let progress = on_event.clone();
        worker
            .download(
                move |n, total| {
                    done += n as u64;
                    if last.elapsed() >= PROGRESS_EVERY || total == Some(done) {
                        last = Instant::now();
                        let _ = progress.send(UpdateEvent::Progress { done, total });
                    }
                },
                move || {
                    let _ = on_event.send(UpdateEvent::Verifying);
                },
            )
            .await
    });
    let abort = task.inner().abort_handle();
    *lock(&state.abort) = Some(Box::new(move || abort.abort()));
    let outcome = task.await;
    *lock(&state.abort) = None;
    match outcome {
        Ok(Ok(bytes)) => {
            *lock(&state.ready) = Some((update, bytes));
            Ok(())
        }
        Ok(Err(e)) => Err(describe(e)),
        Err(_) if state.cancelled.load(Ordering::SeqCst) => Err(UpdateFailure { kind: "cancelled", message: "Descarga cancelada.".into() }),
        Err(_) => Err(UpdateFailure { kind: "server", message: "La descarga se interrumpió.".into() }),
    }
}

#[tauri::command]
pub fn update_cancel(state: State<'_, UpdateState>) {
    state.cancelled.store(true, Ordering::SeqCst);
    if let Some(abort) = lock(&state.abort).take() {
        abort();
    }
}

/// Lanza el instalador ya verificado. En Windows la aplicación se cierra aquí y el instalador la vuelve a abrir.
#[tauri::command]
pub async fn update_restart(state: State<'_, UpdateState>) -> Result<(), UpdateFailure> {
    let (update, bytes) = lock(&state.ready).take().ok_or_else(|| UpdateFailure { kind: "install", message: "No hay ninguna actualización preparada.".into() })?;
    let (update, bytes, result) = tauri::async_runtime::spawn_blocking(move || {
        let r = update.install(&bytes);
        (update, bytes, r)
    })
    .await
    .map_err(|_| UpdateFailure { kind: "install", message: "No se pudo iniciar la instalación.".into() })?;
    result.map_err(|e| {
        let f = describe(e);
        // Si no llegó a instalarse, se puede volver a intentar sin descargar otra vez (salvo que la firma ya no sea válida).
        if f.kind != "signature" {
            *lock(&state.ready) = Some((update, bytes));
        }
        f
    })
}
