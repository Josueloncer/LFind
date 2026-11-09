//! Inspector de archivos. La lógica vive en `nova_core::inspector`.

use crate::state::AppState;
use nova_core::{inspector, safety};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::ipc::Channel;
use tauri::State;

#[tauri::command]
pub async fn inspect_path(path: String) -> Result<inspector::Inspection, String> {
    let p = safety::parse_path(&path)?;
    tauri::async_runtime::spawn_blocking(move || inspector::inspect(&p)).await.map_err(|e| e.to_string())?
}

#[derive(Serialize, Clone)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum HashEvent {
    #[serde(rename_all = "camelCase")]
    Progress { done: u64, total: u64 },
    Done { hash: Option<String>, error: Option<String> },
}

/// Calcula el hash de un archivo. Solo se llama a petición del usuario y se cancela con `cancel_job`.
#[tauri::command]
pub fn start_hash(path: String, on_event: Channel<HashEvent>, state: State<'_, AppState>) -> Result<u64, String> {
    let p = safety::parse_path(&path)?;
    let jobs = state.jobs.clone();
    let id = jobs.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    let cancel = Arc::new(AtomicBool::new(false));
    jobs.flags.lock().unwrap().insert(id, cancel.clone());
    tauri::async_runtime::spawn_blocking(move || {
        nova_core::sys::background_thread();
        let r = inspector::hash_file(&p, &cancel, &mut |done, total| {
            let _ = on_event.send(HashEvent::Progress { done, total });
        });
        let _ = on_event.send(match r {
            Ok(hash) => HashEvent::Done { hash, error: None },
            Err(e) => HashEvent::Done { hash: None, error: Some(e) },
        });
        jobs.flags.lock().unwrap().remove(&id);
    });
    Ok(id)
}
