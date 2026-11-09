//! Integración con Windows y apertura de carpetas desde fuera. La lógica vive en `nova_core::winshell`.

use nova_core::winshell::{path_from_args, Feature, Integration, Status};
use std::path::Path;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};

/// Carpeta con la que se abrió LFind, a la espera de que la interfaz la recoja.
#[derive(Default)]
pub struct StartupPath(pub Mutex<Option<String>>);

/// Una compilación de desarrollo abre una consola y depende del servidor de Vite: Windows nunca debe apuntar a ella.
fn writable() -> Result<Integration, String> {
    if cfg!(debug_assertions) {
        return Err("Estás usando una compilación de desarrollo. Activa la integración desde LFind instalado.".into());
    }
    integration()
}

fn integration() -> Result<Integration, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(Integration::for_user(exe.display().to_string()))
}

/// Solo cuenta si es una carpeta que existe: una línea de órdenes no debe poder llevar a ningún otro sitio.
fn folder_of(args: &[String]) -> Option<String> {
    path_from_args(args).filter(|p| Path::new(p).is_dir())
}

pub fn remember_startup(app: &AppHandle) {
    let args: Vec<String> = std::env::args().collect();
    *app.state::<StartupPath>().0.lock().unwrap() = folder_of(&args);
}

/// Otra invocación de LFind: se trae al frente la ventana abierta y se abre allí la carpeta.
pub fn on_second_instance(app: &AppHandle, args: Vec<String>) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
    if let Some(path) = folder_of(&args) {
        let _ = app.emit("lfind://open-path", path);
    }
}

#[tauri::command]
pub fn take_startup_path(state: State<'_, StartupPath>) -> Option<String> {
    state.0.lock().unwrap().take()
}

#[tauri::command]
pub async fn win_status() -> Result<Status, String> {
    let i = integration()?;
    tauri::async_runtime::spawn_blocking(move || i.status()).await.map_err(|e| e.to_string())
}

/// Cada cambio llega aquí solo después de que el usuario lo haya confirmado en la interfaz.
#[tauri::command]
pub async fn win_repair() -> Result<Status, String> {
    let i = writable()?;
    tauri::async_runtime::spawn_blocking(move || i.repair()).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn win_set(feature: Feature, on: bool) -> Result<Status, String> {
    let i = if on { writable()? } else { integration()? };
    tauri::async_runtime::spawn_blocking(move || i.set(feature, on)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn win_restore() -> Result<Status, String> {
    let i = integration()?;
    tauri::async_runtime::spawn_blocking(move || i.restore()).await.map_err(|e| e.to_string())?
}
