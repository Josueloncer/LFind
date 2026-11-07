//! Operaciones del sistema que respaldan el menú contextual propio: abrir con, terminal, comprimir/extraer, «Enviar a»
//! y plantillas de «Nuevo». Todo son llamadas directas a Rust (sin IContextMenu ni extensiones COM). Las rutas se validan
//! aquí; «Enviar a» y «Nuevo» se invocan por identificador, nunca por una ruta ejecutable llegada de la interfaz.

use nova_core::{archive, newmenu, safety, sendto, shell};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

fn existing(path: &str) -> Result<PathBuf, String> {
    let p = safety::parse_path(path)?;
    std::fs::symlink_metadata(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn open_with(path: String) -> Result<(), String> {
    let p = existing(&path)?;
    blocking(move || shell::open_with(&p).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn open_terminal(dir: String) -> Result<(), String> {
    let p = existing(&dir)?;
    if !p.is_dir() {
        return Err("Eso no es una carpeta.".into());
    }
    blocking(move || shell::open_terminal(&p).map_err(|e| e.to_string())).await
}

/// Comprime `paths` (todos dentro de `dir`) en un `.zip` nuevo. Devuelve la ruta del zip.
#[tauri::command]
pub async fn compress_paths(paths: Vec<String>, dir: String) -> Result<String, String> {
    let dir = existing(&dir)?;
    let sources = paths.iter().map(|p| existing(p)).collect::<Result<Vec<_>, _>>()?;
    blocking(move || archive::compress(&sources, &dir).map(|p| p.display().to_string()).map_err(|e| e.to_string())).await
}

/// Extrae un archivo comprimido en una carpeta nueva junto a él. Devuelve la carpeta.
#[tauri::command]
pub async fn extract_archive(path: String) -> Result<String, String> {
    let p = existing(&path)?;
    blocking(move || archive::extract(&p).map(|p| p.display().to_string()).map_err(|e| e.to_string())).await
}

// ───────── Enviar a ─────────
static SEND_TO: OnceLock<Mutex<HashMap<String, sendto::Target>>> = OnceLock::new();

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendToItem {
    id: String,
    label: String,
    kind: &'static str,
}

#[tauri::command]
pub async fn list_send_to() -> Result<Vec<SendToItem>, String> {
    blocking(|| {
        let (items, map) = sendto::list().map_err(|e| e.to_string())?;
        *SEND_TO.get_or_init(Default::default).lock().map_err(|e| e.to_string())? = map;
        Ok(items.into_iter().map(|i| SendToItem { id: i.id, label: i.label, kind: i.kind }).collect())
    })
    .await
}

/// Qué debe hacer la interfaz tras `send_to`.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SendResult {
    /// Rust ya lanzó la aplicación.
    Launched,
    /// Destino carpeta: la interfaz inicia una copia con su cola de operaciones (progreso, deshacer, conflictos).
    #[serde(rename_all = "camelCase")]
    CopyTo { dest: String },
    /// «Carpeta comprimida»: la interfaz llama a `compress_paths`.
    Zip,
}

#[tauri::command]
pub async fn send_to(paths: Vec<String>, id: String) -> Result<SendResult, String> {
    let sources = paths.iter().map(|p| existing(p).map(|b| b.to_string_lossy().into_owned())).collect::<Result<Vec<_>, _>>()?;
    let target = SEND_TO
        .get()
        .and_then(|m| m.lock().ok()?.get(&id).cloned())
        .ok_or_else(|| "Ese destino de «Enviar a» ya no está disponible.".to_string())?;
    match target {
        sendto::Target::Zip => Ok(SendResult::Zip),
        sendto::Target::Folder(d) => Ok(SendResult::CopyTo { dest: d.display().to_string() }),
        sendto::Target::App { exe, args } => blocking(move || sendto::launch(&exe, &args, &sources).map(|_| SendResult::Launched).map_err(|e| e.to_string())).await,
    }
}

// ───────── Nuevo ─────────
#[derive(Serialize)]
pub struct NewTemplate {
    id: String,
    label: String,
}

#[tauri::command]
pub async fn list_new_templates() -> Result<Vec<NewTemplate>, String> {
    blocking(|| Ok(newmenu::list().into_iter().map(|i| NewTemplate { id: i.id, label: i.label }).collect())).await
}

#[tauri::command]
pub async fn create_from_template(dir: String, id: String) -> Result<String, String> {
    let d = existing(&dir)?;
    blocking(move || newmenu::create(&d, &id).map(|p| p.display().to_string()).map_err(|e| e.to_string())).await
}
