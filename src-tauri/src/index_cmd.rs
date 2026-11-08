//! Comandos del motor de búsqueda global (Update 05). La lógica vive en `nova_core::index`; aquí solo se abre el índice
//! en la carpeta de datos de la aplicación, se guarda la configuración y se adaptan las llamadas para la interfaz.

use nova_core::index::engine::{detect_fixed_drives, StartOpts};
use nova_core::index::{IndexSettings, IndexStatus, Indexer, Query, QueryPage};
use nova_core::index::sqlite::SqliteStore;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Manager, State};

const SETTINGS_KEY: &str = "settings";
/// Cada cuánto se comprueba si hay unidades nuevas o que reconciliar (solo reindexa lo que toca).
const TICK: Duration = Duration::from_secs(600);

pub struct IndexState {
    pub ix: Option<Indexer>,
    pub init_error: Option<String>,
    settings: Mutex<IndexSettings>,
    /// Bandera de la última búsqueda: una nueva la cancela.
    last_search: Mutex<Option<Arc<AtomicBool>>>,
}

fn unavailable(s: &IndexState) -> String {
    s.init_error.clone().unwrap_or_else(|| "El índice de búsqueda no está disponible.".into())
}

fn roots_of(s: &IndexSettings) -> Vec<String> {
    s.drives.clone().unwrap_or_else(detect_fixed_drives)
}

fn load_settings(ix: &Indexer) -> IndexSettings {
    ix.store().get_meta(SETTINGS_KEY).ok().flatten().and_then(|j| serde_json::from_str::<IndexSettings>(&j).ok()).unwrap_or_default()
}

fn opts_for(s: &IndexSettings, force: bool) -> StartOpts {
    StartOpts { force, reconcile_ms: s.reconcile_minutes as i64 * 60_000, ..Default::default() }
}

/// Abre el índice y programa el indexado en segundo plano. Un fallo no impide arrancar la aplicación: la búsqueda queda desactivada.
pub fn init(app: &AppHandle) -> IndexState {
    let dir = app.path().app_local_data_dir().unwrap_or_else(|_| std::env::temp_dir().join("nova-explorer")).join("index");
    match SqliteStore::open(&dir.join("lfind-index.db")) {
        Ok(store) => {
            let ix = Indexer::new(Arc::new(store));
            let settings = load_settings(&ix);
            let ticker = ix.clone();
            std::thread::spawn(move || {
                // Espera breve para que la ventana termine de pintarse antes de empezar a leer discos.
                std::thread::sleep(Duration::from_secs(3));
                loop {
                    let s = load_settings(&ticker);
                    if s.enabled {
                        let _ = ticker.start(roots_of(&s), s.rules(), opts_for(&s, false));
                    }
                    std::thread::sleep(TICK);
                }
            });
            IndexState { ix: Some(ix), init_error: None, settings: Mutex::new(settings), last_search: Mutex::new(None) }
        }
        Err(e) => IndexState { ix: None, init_error: Some(format!("No se pudo abrir el índice de búsqueda: {e}")), settings: Mutex::new(IndexSettings::default()), last_search: Mutex::new(None) },
    }
}

#[tauri::command]
pub async fn index_search(query: Query, state: State<'_, IndexState>) -> Result<QueryPage, String> {
    let ix = state.ix.clone().ok_or_else(|| unavailable(&state))?;
    // Una búsqueda nueva cancela la anterior (la interfaz ignora las páginas canceladas).
    let flag = Arc::new(AtomicBool::new(false));
    if let Some(prev) = state.last_search.lock().unwrap().replace(flag.clone()) {
        prev.store(true, Ordering::Relaxed);
    }
    tauri::async_runtime::spawn_blocking(move || ix.search(&query, &flag)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn index_status(state: State<'_, IndexState>) -> Result<IndexStatus, String> {
    let ix = state.ix.clone().ok_or_else(|| unavailable(&state))?;
    tauri::async_runtime::spawn_blocking(move || ix.status()).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn index_get_settings(state: State<'_, IndexState>) -> IndexSettings {
    state.settings.lock().unwrap().clone()
}

/// Devuelve las unidades fijas detectadas (para elegir cuáles indexar).
#[tauri::command]
pub fn index_available_drives() -> Vec<String> {
    detect_fixed_drives()
}

/// Guarda la configuración y la aplica: si cambian exclusiones o unidades se reindexa (lo ya excluido sale del índice).
#[tauri::command]
pub async fn index_set_settings(settings: IndexSettings, state: State<'_, IndexState>) -> Result<(), String> {
    let ix = state.ix.clone().ok_or_else(|| unavailable(&state))?;
    let old = std::mem::replace(&mut *state.settings.lock().unwrap(), settings.clone());
    let changed = old != settings;
    let json = serde_json::to_string(&settings).map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        ix.store().set_meta(SETTINGS_KEY, &json)?;
        if !changed {
            return Ok(());
        }
        ix.stop();
        if settings.enabled {
            // Cambiar exclusiones o la lista de unidades obliga a una pasada completa para limpiar y completar el índice.
            let force = old.excluded_paths != settings.excluded_paths || old.excluded_names != settings.excluded_names || old.excluded_root_dirs != settings.excluded_root_dirs || old.excluded_exts != settings.excluded_exts;
            ix.start(roots_of(&settings), settings.rules(), opts_for(&settings, force))?;
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn index_start(state: State<'_, IndexState>) -> Result<bool, String> {
    let ix = state.ix.clone().ok_or_else(|| unavailable(&state))?;
    let s = state.settings.lock().unwrap().clone();
    tauri::async_runtime::spawn_blocking(move || ix.start(roots_of(&s), s.rules(), opts_for(&s, false))).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn index_pause(state: State<'_, IndexState>) -> Result<(), String> {
    state.ix.as_ref().ok_or_else(|| unavailable(&state))?.pause();
    Ok(())
}

#[tauri::command]
pub fn index_resume(state: State<'_, IndexState>) -> Result<(), String> {
    state.ix.as_ref().ok_or_else(|| unavailable(&state))?.resume();
    Ok(())
}

#[tauri::command]
pub async fn index_rebuild(state: State<'_, IndexState>) -> Result<bool, String> {
    let ix = state.ix.clone().ok_or_else(|| unavailable(&state))?;
    let s = state.settings.lock().unwrap().clone();
    tauri::async_runtime::spawn_blocking(move || ix.rebuild(roots_of(&s), s.rules(), opts_for(&s, true))).await.map_err(|e| e.to_string())?
}
