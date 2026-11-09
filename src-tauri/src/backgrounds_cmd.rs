//! Imágenes de fondo de la personalización. La lógica vive en `nova_core::backgrounds`.

use nova_core::backgrounds::Backgrounds;
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};

pub struct BackgroundState(pub Arc<Backgrounds>);

pub fn init(app: &AppHandle) -> BackgroundState {
    let dir = app.path().app_local_data_dir().unwrap_or_else(|_| std::env::temp_dir().join("nova-explorer")).join("backgrounds");
    BackgroundState(Arc::new(Backgrounds::new(dir)))
}

#[tauri::command]
pub async fn background_import(path: String, state: State<'_, BackgroundState>) -> Result<String, String> {
    let b = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || b.import(std::path::Path::new(&path))).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn background_load(id: String, state: State<'_, BackgroundState>) -> Result<Option<String>, String> {
    let b = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || b.load(&id)).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn background_retain(keep: Vec<String>, state: State<'_, BackgroundState>) -> Result<usize, String> {
    let b = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || b.retain(&keep)).await.map_err(|e| e.to_string())
}
