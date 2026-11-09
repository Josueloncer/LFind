//! Aplicaciones para «Abrir con», arrastre nativo y Papelera de Windows.

use nova_core::{safety, shellapps};
use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;
use tauri::WebviewWindow;

async fn blocking<R: Send + 'static>(f: impl FnOnce() -> Result<R, String> + Send + 'static) -> Result<R, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn open_with_list(path: String) -> Result<Vec<String>, String> {
    let ext = Path::new(&path).extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    blocking(move || shellapps::handlers(&ext)).await
}

#[tauri::command]
pub async fn open_with_app(path: String, app: String) -> Result<(), String> {
    let p = safety::parse_path(&path)?;
    blocking(move || shellapps::open_with_handler(&p, &app)).await
}

/// Arrastre OLE. Ventana y arrastre comparten hilo: Windows atiende los mensajes mientras dura.
#[tauri::command]
pub async fn start_native_drag(paths: Vec<String>, window: WebviewWindow) -> Result<u32, String> {
    #[cfg(windows)]
    {
        for p in &paths {
            safety::parse_path(p)?;
        }
        let hwnd = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
        let (tx, rx) = std::sync::mpsc::channel();
        window
            .run_on_main_thread(move || {
                let _ = tx.send(unsafe { shellapps::drag_files(hwnd, &paths) });
            })
            .map_err(|e| e.to_string())?;
        blocking(move || rx.recv().map_err(|e| e.to_string())?).await
    }
    #[cfg(not(windows))]
    {
        let _ = (paths, window);
        Err("Solo disponible en Windows.".into())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecycleItem {
    id: String,
    name: String,
    original: String,
    deleted: i64,
    size: Option<u64>,
    entries: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecycleInfo {
    items: Vec<RecycleItem>,
    total_bytes: Option<u64>,
}

/// La biblioteca inicializa COM a su manera en cada hilo: con uno propio no choca con otras tareas.
fn own_thread<R: Send + 'static>(f: impl FnOnce() -> Result<R, String> + Send + 'static) -> Result<R, String> {
    std::thread::Builder::new().name("lfind-recycle".into()).spawn(f).map_err(|e| e.to_string())?.join().map_err(|_| "Error interno al acceder a la Papelera.".to_string())?
}

#[cfg(windows)]
fn total_bytes() -> Option<u64> {
    #[repr(C)]
    struct Info {
        size: u32,
        bytes: i64,
        items: i64,
    }
    #[link(name = "shell32")]
    extern "system" {
        fn SHQueryRecycleBinW(root: *const u16, info: *mut Info) -> i32;
    }
    let mut i = Info { size: std::mem::size_of::<Info>() as u32, bytes: 0, items: 0 };
    (unsafe { SHQueryRecycleBinW(std::ptr::null(), &mut i) } >= 0).then_some(i.bytes as u64)
}
#[cfg(not(windows))]
fn total_bytes() -> Option<u64> {
    None
}

/// Tamaño leído del propio archivo en `$Recycle.Bin`: preguntárselo al shell por cada elemento tarda varios segundos con una papelera normal.
fn physical_size(id: &str) -> (Option<u64>, Option<u64>) {
    let lower = id.to_ascii_lowercase();
    let Some(at) = lower.find(":\\$recycle.bin\\").filter(|a| *a > 0) else { return (None, None) };
    let path = Path::new(&id[at - 1..]);
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => (None, std::fs::read_dir(path).ok().map(|d| d.take(10_000).count() as u64)),
        Ok(m) => (Some(m.len()), None),
        Err(_) => (None, None),
    }
}

#[tauri::command]
pub async fn recycle_list() -> Result<RecycleInfo, String> {
    blocking(|| {
        own_thread(|| {
            let items = trash::os_limited::list().map_err(|e| e.to_string())?;
            let items = items
                .into_iter()
                .map(|it| {
                    let (size, entries) = physical_size(&it.id.to_string_lossy());
                    RecycleItem {
                        id: it.id.to_string_lossy().into_owned(),
                        original: it.original_path().display().to_string(),
                        deleted: it.time_deleted,
                        size,
                        entries,
                        name: it.name.to_string_lossy().into_owned(),
                    }
                })
                .collect();
            Ok(RecycleInfo { items, total_bytes: total_bytes() })
        })
    })
    .await
}

/// Solo actúa sobre lo que la propia Papelera confirma tener: la interfaz no puede pasar una ruta cualquiera.
fn selected(ids: &[String]) -> Result<Vec<trash::TrashItem>, String> {
    let wanted: HashSet<&str> = ids.iter().map(String::as_str).collect();
    let items: Vec<_> = trash::os_limited::list().map_err(|e| e.to_string())?.into_iter().filter(|it| wanted.contains(it.id.to_string_lossy().as_ref())).collect();
    if items.is_empty() {
        return Err("Esos elementos ya no están en la Papelera.".into());
    }
    Ok(items)
}

#[tauri::command]
pub async fn recycle_restore(ids: Vec<String>) -> Result<(), String> {
    blocking(move || {
        own_thread(move || {
            trash::os_limited::restore_all(selected(&ids)?).map_err(|e| match e {
                trash::Error::RestoreCollision { path, .. } => format!("Ya existe un elemento en la ubicación original: {}", path.display()),
                other => other.to_string(),
            })
        })
    })
    .await
}

/// La confirmación se hace siempre en la interfaz, antes de llegar aquí.
#[tauri::command]
pub async fn recycle_purge(ids: Vec<String>) -> Result<(), String> {
    blocking(move || own_thread(move || trash::os_limited::purge_all(selected(&ids)?).map_err(|e| e.to_string()))).await
}
