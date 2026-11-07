use crate::state::AppState;
use nova_core::ops::{self, Decision};
use nova_core::undo::{self, UndoEntry};
use nova_core::{dir, names, pathbar, places, prefs, props, safety, shell, volumes};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

/// Eventos que se envían a la interfaz durante la lectura de un directorio.
#[derive(Serialize, Clone)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DirEvent {
    Batch { entries: Vec<dir::Entry> },
    #[serde(rename_all = "camelCase")]
    Done { total: usize, skipped: usize, cancelled: bool },
    Error { message: String },
}

/// Inicia la lectura de `path` en un hilo del pool de bloqueo y devuelve un id cancelable.
/// Los resultados llegan por `on_event`; esta llamada no espera a que termine la lectura.
#[tauri::command]
pub fn open_dir(path: String, on_event: Channel<DirEvent>, state: State<'_, AppState>) -> u64 {
    let jobs = state.jobs.clone();
    let id = jobs.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    let cancel = Arc::new(AtomicBool::new(false));
    jobs.flags.lock().unwrap().insert(id, cancel.clone());

    tauri::async_runtime::spawn_blocking(move || {
        let ch = on_event.clone();
        let cancel_for_emit = cancel.clone();
        let t0 = Instant::now();
        let mut first_ms: Option<u128> = None;
        let result = dir::read_dir_batched(
            &PathBuf::from(&path),
            dir::FIRST_BATCH,
            dir::BATCH,
            &cancel,
            |entries| {
                first_ms.get_or_insert_with(|| t0.elapsed().as_millis());
                // Si la ventana ya no escucha, dejamos de leer.
                if ch.send(DirEvent::Batch { entries }).is_err() {
                    cancel_for_emit.store(true, Ordering::Relaxed);
                }
            },
        );
        // Solo en compilaciones de desarrollo: aparece en la terminal de `tauri dev`.
        if cfg!(debug_assertions) {
            if let Ok(s) = &result {
                eprintln!(
                    "[nova] open_dir {path:?}: primer lote {} ms · {} entradas en {} ms{}",
                    first_ms.map(|m| m.to_string()).unwrap_or_else(|| "-".into()),
                    s.total,
                    t0.elapsed().as_millis(),
                    if s.cancelled { " (cancelada)" } else { "" }
                );
            }
        }
        let _ = match result {
            Ok(s) => on_event.send(DirEvent::Done { total: s.total, skipped: s.skipped, cancelled: s.cancelled }),
            Err(e) => on_event.send(DirEvent::Error { message: describe_io_error(&e) }),
        };
        jobs.flags.lock().unwrap().remove(&id);
    });
    id
}

#[tauri::command]
pub fn cancel_dir(id: u64, state: State<'_, AppState>) {
    cancel_job(id, state)
}

/// Cancela cualquier trabajo (lectura, copia, estadísticas…).
#[tauri::command]
pub fn cancel_job(id: u64, state: State<'_, AppState>) {
    if let Some(flag) = state.jobs.flags.lock().unwrap().get(&id) {
        flag.store(true, Ordering::Relaxed);
    }
}

fn describe_io_error(e: &std::io::Error) -> String {
    use std::io::ErrorKind::*;
    match e.kind() {
        NotFound => "La carpeta no existe o se ha movido.".into(),
        PermissionDenied => "Acceso denegado a esta carpeta.".into(),
        _ => format!("No se pudo leer la carpeta: {e}"),
    }
}

#[derive(Serialize)]
pub struct PlacesInfo {
    pub places: Vec<places::Place>,
    pub drives: Vec<places::Place>,
}

/// `async` + `spawn_blocking`: los comandos síncronos de Tauri se ejecutan en el hilo
/// principal de la ventana. Esta consulta toca el disco (carpetas conocidas del shell,
/// OneDrive, unidades) y no debe poder congelar la interfaz.
#[tauri::command]
pub async fn get_places() -> Result<PlacesInfo, String> {
    tauri::async_runtime::spawn_blocking(|| PlacesInfo { places: places::user_places(), drives: places::drives() })
        .await
        .map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformInfo {
    pub vibrancy: bool,
}

#[tauri::command]
pub fn get_platform_info(state: State<'_, AppState>) -> PlatformInfo {
    PlatformInfo { vibrancy: state.vibrancy.load(Ordering::Relaxed) }
}

fn prefs_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_config_dir().map(|d| d.join("prefs.json")).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_prefs(app: AppHandle) -> Result<prefs::Prefs, String> {
    let path = prefs_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || prefs::load(&path))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_prefs(app: AppHandle, prefs: prefs::Prefs) -> Result<(), String> {
    let path = prefs_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || prefs::save(&path, &prefs))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}


// ───────────────────────── operaciones de archivo ─────────────────────────

#[derive(Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OpKind {
    Copy,
    Move,
    /// Enviar a la Papelera de reciclaje (recuperable).
    Trash,
    /// Eliminar de forma permanente.
    Delete,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpRequest {
    pub kind: OpKind,
    pub sources: Vec<String>,
    pub dest: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DoneInfo {
    pub completed: u64,
    pub skipped: u64,
    pub errors: Vec<ops::OpError>,
    pub cancelled: bool,
    pub undoable: bool,
}

#[derive(Serialize, Clone)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum OpEvent {
    Progress(ops::Progress),
    Conflict(ops::ConflictInfo),
    Done(DoneInfo),
}

fn send_to_trash(p: &Path) -> Result<(), String> {
    trash::delete(p).map_err(|e| format!("No se pudo enviar a la Papelera: {e}. Usa Mayús+Supr para eliminar de forma permanente."))
}

fn parse_all(list: &[String]) -> Result<Vec<PathBuf>, String> {
    if list.is_empty() {
        return Err("No hay elementos seleccionados.".into());
    }
    list.iter().map(|s| safety::parse_path(s)).collect()
}

fn run_op(req: &OpRequest, on_event: &Channel<OpEvent>, cancel: &AtomicBool, answers: &mpsc::Receiver<(Decision, bool)>) -> Result<ops::OpOutcome, String> {
    let sources = parse_all(&req.sources)?;
    let mut last = Instant::now() - Duration::from_secs(1);
    // Se limita a ~12 eventos/s: miles de archivos pequeños no deben saturar la interfaz.
    let mut on_progress = |p: &ops::Progress| {
        if last.elapsed() >= Duration::from_millis(80) {
            last = Instant::now();
            if on_event.send(OpEvent::Progress(p.clone())).is_err() {
                cancel.store(true, Ordering::Relaxed);
            }
        }
    };
    let mut sticky: Option<Decision> = None;
    let mut on_conflict = |info: &ops::ConflictInfo| -> Decision {
        if let Some(d) = sticky {
            return d;
        }
        if on_event.send(OpEvent::Conflict(info.clone())).is_err() {
            return Decision::Cancel;
        }
        loop {
            match answers.recv_timeout(Duration::from_millis(100)) {
                Ok((d, all)) => {
                    if all {
                        sticky = Some(d);
                    }
                    return d;
                }
                Err(mpsc::RecvTimeoutError::Timeout) if !cancel.load(Ordering::Relaxed) => continue,
                Err(_) => return Decision::Cancel,
            }
        }
    };
    let mut ctx = ops::Ctx { cancel, on_progress: &mut on_progress, on_conflict: &mut on_conflict };

    match req.kind {
        OpKind::Copy | OpKind::Move => {
            let dest = safety::parse_path(req.dest.as_deref().ok_or("Falta la carpeta de destino.")?)?;
            Ok(if req.kind == OpKind::Copy { ops::copy_items(&sources, &dest, &mut ctx) } else { ops::move_items(&sources, &dest, &mut ctx) })
        }
        OpKind::Delete => Ok(ops::delete_items(&sources, &mut ctx)),
        OpKind::Trash => Ok(ops::trash_items(
            &sources,
            &mut ctx,
            // Un lote = una sola operación del sistema (mucho más rápido que una por elemento); el reintento individual
            // atribuye el error al elemento correcto.
            &mut |chunk| trash::delete_all(chunk).map_err(|e| e.to_string()),
            &mut |p| send_to_trash(p),
        )),
    }
}

/// Inicia una operación en segundo plano y devuelve su id. El avance, los conflictos y el
/// resultado llegan por `on_event`. Toda la validación ocurre en el backend.
#[tauri::command]
pub fn start_op(req: OpRequest, on_event: Channel<OpEvent>, state: State<'_, AppState>) -> u64 {
    let jobs = state.jobs.clone();
    let undo_stack = state.undo.clone();
    let id = jobs.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    let cancel = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    jobs.flags.lock().unwrap().insert(id, cancel.clone());
    jobs.answers.lock().unwrap().insert(id, tx);

    tauri::async_runtime::spawn_blocking(move || {
        let t0 = Instant::now();
        let done = match run_op(&req, &on_event, &cancel, &rx) {
            Ok(o) => {
                let undoable = o.undo.is_some();
                if let Some(u) = o.undo {
                    let mut st = undo_stack.lock().unwrap();
                    st.push(u);
                    let excess = st.len().saturating_sub(crate::state::MAX_UNDO);
                    st.drain(..excess);
                }
                DoneInfo { completed: o.completed, skipped: o.skipped, errors: o.errors, cancelled: o.cancelled, undoable }
            }
            Err(message) => DoneInfo { completed: 0, skipped: 0, errors: vec![ops::OpError { path: String::new(), message, kind: ops::ErrKind::Invalid }], cancelled: false, undoable: false },
        };
        if cfg!(debug_assertions) {
            eprintln!("[nova] op: {} completados, {} errores en {} ms", done.completed, done.errors.len(), t0.elapsed().as_millis());
        }
        let _ = on_event.send(OpEvent::Done(done));
        jobs.flags.lock().unwrap().remove(&id);
        jobs.answers.lock().unwrap().remove(&id);
    });
    id
}

/// Respuesta de la interfaz a un conflicto («Reemplazar», «Omitir»…).
#[tauri::command]
pub fn resolve_conflict(id: u64, decision: Decision, apply_all: bool, state: State<'_, AppState>) {
    if let Some(tx) = state.jobs.answers.lock().unwrap().get(&id) {
        let _ = tx.send((decision, apply_all));
    }
}

fn push_undo(state: &AppState, u: UndoEntry) {
    let mut st = state.undo.lock().unwrap();
    st.push(u);
    let excess = st.len().saturating_sub(crate::state::MAX_UNDO);
    st.drain(..excess);
}

#[tauri::command]
pub async fn rename_path(path: String, new_name: String, state: State<'_, AppState>) -> Result<String, ops::OpError> {
    let invalid = |m: String| ops::OpError { path: path.clone(), message: m, kind: ops::ErrKind::Invalid };
    let p = safety::parse_path(&path).map_err(invalid)?;
    let r = tauri::async_runtime::spawn_blocking(move || ops::rename_item(&p, &new_name)).await.map_err(|e| ops::OpError { path: String::new(), message: e.to_string(), kind: ops::ErrKind::Other })??;
    push_undo(&state, r.1);
    Ok(r.0.display().to_string())
}

#[tauri::command]
pub async fn create_folder(parent: String, name: String, state: State<'_, AppState>) -> Result<String, ops::OpError> {
    create_item(parent, name, true, state).await
}

#[tauri::command]
pub async fn create_file(parent: String, name: String, state: State<'_, AppState>) -> Result<String, ops::OpError> {
    create_item(parent, name, false, state).await
}

async fn create_item(parent: String, name: String, folder: bool, state: State<'_, AppState>) -> Result<String, ops::OpError> {
    let p = safety::parse_path(&parent).map_err(|m| ops::OpError { path: parent.clone(), message: m, kind: ops::ErrKind::Invalid })?;
    let r = tauri::async_runtime::spawn_blocking(move || if folder { ops::create_folder(&p, &name) } else { ops::create_file(&p, &name) })
        .await
        .map_err(|e| ops::OpError { path: String::new(), message: e.to_string(), kind: ops::ErrKind::Other })??;
    push_undo(&state, r.1);
    Ok(r.0.display().to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoResult {
    pub description: String,
    pub restored: u64,
    pub errors: Vec<ops::OpError>,
}

/// Descripción de la última operación deshacible, si la hay.
#[tauri::command]
pub fn peek_undo(state: State<'_, AppState>) -> Option<String> {
    state.undo.lock().unwrap().last().map(|u| u.describe())
}

#[tauri::command]
pub async fn undo_last(state: State<'_, AppState>) -> Result<UndoResult, String> {
    let entry = state.undo.lock().unwrap().pop().ok_or("No hay nada que deshacer.")?;
    let description = entry.describe();
    let rep = tauri::async_runtime::spawn_blocking(move || undo::apply(&entry, &send_to_trash)).await.map_err(|e| e.to_string())?;
    Ok(UndoResult { description, restored: rep.restored, errors: rep.errors })
}

// ───────────────────────── abrir, propiedades, accesos directos ─────────────────────────

fn existing(path: &str) -> Result<PathBuf, String> {
    let p = safety::parse_path(path)?;
    std::fs::symlink_metadata(&p).map_err(|e| ops::classify(&e).1)?;
    Ok(p)
}

#[tauri::command]
pub async fn open_path(path: String) -> Result<(), String> {
    let p = existing(&path)?;
    tauri::async_runtime::spawn_blocking(move || shell::open_path(&p).map_err(|e| e.to_string())).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn reveal_path(path: String) -> Result<(), String> {
    let p = existing(&path)?;
    tauri::async_runtime::spawn_blocking(move || shell::reveal(&p).map_err(|e| e.to_string())).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn create_shortcut(target: String) -> Result<String, ops::OpError> {
    let fail = |m: String| ops::OpError { path: target.clone(), message: m, kind: ops::ErrKind::Other };
    let t = existing(&target).map_err(fail)?;
    tauri::async_runtime::spawn_blocking(move || {
        let dir = t.parent().ok_or_else(|| ops::OpError { path: t.display().to_string(), message: "No se puede crear un acceso directo de una raíz.".into(), kind: ops::ErrKind::Invalid })?;
        let base = t.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let ext = if cfg!(windows) { ".lnk" } else { "" };
        let link = dir.join(names::unique_name(dir, &format!("{base} - Acceso directo{ext}"), false));
        shell::create_shortcut(&t, &link).map_err(|e| ops::op_error(&link, &e))?;
        Ok(link.display().to_string())
    })
    .await
    .map_err(|e| ops::OpError { path: String::new(), message: e.to_string(), kind: ops::ErrKind::Other })?
}

#[tauri::command]
pub async fn get_properties(path: String) -> Result<props::FileProps, String> {
    let p = safety::parse_path(&path)?;
    tauri::async_runtime::spawn_blocking(move || props::file_props(&p).map_err(|e| ops::classify(&e).1)).await.map_err(|e| e.to_string())?
}

#[derive(Serialize, Clone)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum StatsEvent {
    Progress(props::Stats),
    #[serde(rename_all = "camelCase")]
    Done { stats: props::Stats, cancelled: bool },
}

/// Calcula tamaño y nº de elementos de uno o varios caminos, sin bloquear y cancelable.
#[tauri::command]
pub fn start_stats(paths: Vec<String>, on_event: Channel<StatsEvent>, state: State<'_, AppState>) -> Result<u64, String> {
    let list = parse_all(&paths)?;
    let jobs = state.jobs.clone();
    let id = jobs.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    let cancel = Arc::new(AtomicBool::new(false));
    jobs.flags.lock().unwrap().insert(id, cancel.clone());
    tauri::async_runtime::spawn_blocking(move || {
        let (stats, cancelled) = props::dir_stats(&list, &cancel, &mut |s| {
            if on_event.send(StatsEvent::Progress(*s)).is_err() {
                cancel.store(true, Ordering::Relaxed);
            }
        });
        let _ = on_event.send(StatsEvent::Done { stats, cancelled });
        jobs.flags.lock().unwrap().remove(&id);
    });
    Ok(id)
}

/// Capacidad de una unidad. Es una llamada aparte (y no parte de `get_places`) porque una
/// unidad de red desconectada puede tardar: nunca debe retrasar el arranque.
#[tauri::command]
pub async fn get_drive_usage(root: String) -> Result<volumes::DriveUsage, String> {
    tauri::async_runtime::spawn_blocking(move || volumes::drive_usage(&root)).await.map_err(|e| e.to_string())?
}

/// Resultado de comprobar lo que se escribió en la barra de dirección.
#[derive(Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ResolvedPath {
    Dir { path: String },
    File { dir: String, file: String },
}

/// Valida la ruta de la barra de dirección (normaliza, comprueba que existe y se puede abrir). NO navega: la pestaña
/// sigue usando `open_dir`. Corre fuera del hilo principal y con tiempo límite (rutas de red caídas).
#[tauri::command]
pub async fn resolve_path(input: String) -> Result<ResolvedPath, String> {
    tauri::async_runtime::spawn_blocking(move || {
        pathbar::resolve(&input, &|k| std::env::var(k).ok(), pathbar::CHECK_TIMEOUT)
            .map(|r| match r {
                pathbar::Resolved::Dir(path) => ResolvedPath::Dir { path },
                pathbar::Resolved::File { dir, file } => ResolvedPath::File { dir, file },
            })
            .map_err(|p| p.message)
    })
    .await
    .map_err(|e| e.to_string())?
}
