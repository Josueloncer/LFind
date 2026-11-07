//! Portapapeles de archivos del sistema (el mismo que usa el Explorador de Windows).
//! El formato y el acceso viven en `nova-core::{clipboard, clipboard_win}`; aquí solo se validan las rutas y se saca el
//! trabajo del hilo de la ventana.

use nova_core::clipboard::{self, Mode};
use nova_core::safety;
use serde::Serialize;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardFiles {
    pub paths: Vec<String>,
    pub cut: bool,
    pub sequence: u32,
}

/// Publica archivos en el portapapeles del sistema (copiar/cortar). Devuelve el número de secuencia resultante.
#[tauri::command]
pub async fn clipboard_set_files(paths: Vec<String>, cut: bool) -> Result<u32, String> {
    let paths = paths.iter().map(|p| safety::parse_path(p).map(|b| b.to_string_lossy().into_owned())).collect::<Result<Vec<_>, _>>()?;
    tauri::async_runtime::spawn_blocking(move || {
        clipboard::write_files(&paths, if cut { Mode::Cut } else { Mode::Copy })?;
        Ok(clipboard::sequence())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Archivos del portapapeles del sistema, si los hay.
#[tauri::command]
pub async fn clipboard_read_files() -> Result<Option<ClipboardFiles>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let seq = clipboard::sequence();
        Ok(clipboard::read_files()?.map(|(paths, mode)| ClipboardFiles { paths, cut: mode == Mode::Cut, sequence: seq }))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Número de secuencia actual (barato): permite saber si otra aplicación cambió el portapapeles.
#[tauri::command]
pub fn clipboard_sequence() -> u32 {
    clipboard::sequence()
}

/// ¿Hay archivos en el portapapeles del sistema? No lee su contenido: sirve para activar/desactivar «Pegar» sin coste.
#[tauri::command]
pub fn clipboard_has_files() -> bool {
    clipboard::has_files()
}
