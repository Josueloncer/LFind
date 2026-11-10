//! Terminal integrada. La sesión real vive en `nova_core::terminal`; aquí solo se conecta con la interfaz.

use nova_core::terminal::{start_dir, Session, Shell};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::State;

#[derive(Default)]
pub struct TerminalState {
    next: AtomicU64,
    sessions: Mutex<HashMap<u64, Session>>,
}

/// No lanza ningún comando: solo abre el shell. Asíncrono: comprobar la carpeta, decidir el perfil de PowerShell y crear el
/// proceso no deben ocupar el hilo de la ventana.
#[tauri::command]
pub async fn term_open(shell: Shell, cwd: Option<String>, cols: u16, rows: u16, on_data: Channel<InvokeResponseBody>, on_exit: Channel<u32>, state: State<'_, TerminalState>) -> Result<u64, String> {
    let session = tauri::async_runtime::spawn_blocking(move || {
        let dir = start_dir(cwd.as_deref());
        Session::start(
            shell,
            &dir,
            cols,
            rows,
            Box::new(move |bytes| {
                let _ = on_data.send(InvokeResponseBody::Raw(bytes));
            }),
            Box::new(move |code| {
                let _ = on_exit.send(code);
            }),
        )
    })
    .await
    .map_err(|e| e.to_string())??;
    let id = state.next.fetch_add(1, Ordering::Relaxed) + 1;
    state.sessions.lock().unwrap().insert(id, session);
    Ok(id)
}

#[tauri::command]
pub fn term_write(id: u64, data: String, state: State<'_, TerminalState>) -> Result<(), String> {
    state.sessions.lock().unwrap().get(&id).ok_or("La terminal está cerrada.")?.write(data.into_bytes())
}

#[tauri::command]
pub fn term_resize(id: u64, cols: u16, rows: u16, state: State<'_, TerminalState>) {
    if let Some(s) = state.sessions.lock().unwrap().get(&id) {
        s.resize(cols, rows);
    }
}

#[tauri::command]
pub fn term_ack(id: u64, bytes: u64, state: State<'_, TerminalState>) {
    if let Some(s) = state.sessions.lock().unwrap().get(&id) {
        s.ack(bytes);
    }
}

#[tauri::command]
pub fn term_busy(id: u64, state: State<'_, TerminalState>) -> bool {
    state.sessions.lock().unwrap().get(&id).map_or(true, |s| s.busy())
}

#[tauri::command]
pub async fn term_close(id: u64, state: State<'_, TerminalState>) -> Result<(), String> {
    let session = state.sessions.lock().unwrap().remove(&id);
    if let Some(mut s) = session {
        tauri::async_runtime::spawn_blocking(move || s.close()).await.map_err(|e| e.to_string())?;
    }
    Ok(())
}
