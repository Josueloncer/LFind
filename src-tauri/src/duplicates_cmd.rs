//! Detector de duplicados. La lógica vive en `nova_core::duplicates`; aquí solo se conecta con la interfaz.

use nova_core::duplicates::{self, Comparison, Config, Detector, Event, Group, Pending, RemovalCheck, Summary};
use nova_core::safety;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

pub struct DuplicatesState(pub Arc<Detector>);

pub fn init(app: &AppHandle) -> DuplicatesState {
    let dir = app.path().app_local_data_dir().unwrap_or_else(|_| std::env::temp_dir().join("nova-explorer")).join("duplicates");
    DuplicatesState(Arc::new(Detector::new(dir)))
}

async fn blocking<R: Send + 'static>(f: impl FnOnce() -> Result<R, String> + Send + 'static) -> Result<R, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn dup_start(config: Config, on_event: Channel<Event>, state: State<'_, DuplicatesState>) -> Result<(), String> {
    let d = state.0.clone();
    let weak = Arc::downgrade(&d);
    d.start(config, forward(on_event, weak))
}

/// Solo a petición del usuario. Al arrancar LFind nunca se llama.
#[tauri::command]
pub fn dup_resume(on_event: Channel<Event>, state: State<'_, DuplicatesState>) -> Result<(), String> {
    let d = state.0.clone();
    let weak = Arc::downgrade(&d);
    d.resume(forward(on_event, weak))
}

fn forward(on_event: Channel<Event>, weak: std::sync::Weak<Detector>) -> duplicates::OnEvent {
    Box::new(move |e| {
        // Si la interfaz ya no escucha, no se sigue leyendo el disco para nadie; el avance queda guardado.
        if on_event.send(e).is_err() {
            if let Some(d) = weak.upgrade() {
                d.suspend();
            }
        }
    })
}

#[tauri::command]
pub async fn dup_pending(state: State<'_, DuplicatesState>) -> Result<Option<Pending>, String> {
    let d = state.0.clone();
    blocking(move || d.pending()).await
}

#[tauri::command]
pub fn dup_discard(state: State<'_, DuplicatesState>) -> Result<(), String> {
    state.0.discard_pending()
}

#[tauri::command]
pub fn dup_suspend(state: State<'_, DuplicatesState>) {
    state.0.suspend();
}

#[tauri::command]
pub fn dup_pause(paused: bool, state: State<'_, DuplicatesState>) {
    state.0.pause(paused);
}

#[tauri::command]
pub fn dup_cancel(state: State<'_, DuplicatesState>) {
    state.0.cancel();
}

#[tauri::command]
pub fn dup_default_exclusions() -> Vec<String> {
    duplicates::default_exclusions()
}

#[tauri::command]
pub async fn dup_summary(state: State<'_, DuplicatesState>) -> Result<Option<Summary>, String> {
    let d = state.0.clone();
    blocking(move || d.summary()).await
}

#[tauri::command]
pub async fn dup_groups(offset: u32, limit: u32, state: State<'_, DuplicatesState>) -> Result<Vec<Group>, String> {
    let d = state.0.clone();
    blocking(move || d.groups(offset, limit)).await
}

#[tauri::command]
pub async fn dup_check_removal(paths: Vec<String>, state: State<'_, DuplicatesState>) -> Result<RemovalCheck, String> {
    let d = state.0.clone();
    blocking(move || d.check_removal(&paths)).await
}

#[tauri::command]
pub async fn dup_prune(paths: Vec<String>, state: State<'_, DuplicatesState>) -> Result<(), String> {
    let d = state.0.clone();
    blocking(move || d.prune(&paths)).await
}

#[tauri::command]
pub async fn dup_forget(state: State<'_, DuplicatesState>) -> Result<(), String> {
    let d = state.0.clone();
    blocking(move || d.forget()).await
}

#[tauri::command]
pub async fn dup_compare(a: String, b: String) -> Result<Option<Comparison>, String> {
    let (a, b) = (safety::parse_path(&a)?, safety::parse_path(&b)?);
    blocking(move || duplicates::compare(&a, &b, &AtomicBool::new(false))).await
}
