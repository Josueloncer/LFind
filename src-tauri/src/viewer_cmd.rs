//! Comandos del visor y protocolo `lfview`. La lógica vive en `nova_core::viewer`; aquí solo se valida, se saca el trabajo del
//! hilo de la ventana y se adapta la respuesta.

use nova_core::safety;
use nova_core::viewer::serve::{serve, ServeRequest};
use nova_core::viewer::{Source, ViewerService};
use std::sync::Arc;
use tauri::http::{header, Request, Response};
use tauri::{AppHandle, Manager, State};

pub const SCHEME: &str = "lfview";

pub struct ViewerState(pub Arc<ViewerService>);

#[tauri::command]
pub async fn viewer_open(path: String, state: State<'_, ViewerState>) -> Result<Source, String> {
    let p = safety::parse_path(&path)?;
    let svc = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || svc.open(&p)).await.map_err(|e| e.to_string())?
}

/// El visor se cerró: se abandona lo pendiente y se retiran los permisos de lectura.
#[tauri::command]
pub fn viewer_close(state: State<'_, ViewerState>) {
    state.0.close();
}

fn plain(status: u16) -> Response<Vec<u8>> {
    let mut r = Response::new(vec![]);
    *r.status_mut() = tauri::http::StatusCode::from_u16(status).unwrap_or(tauri::http::StatusCode::INTERNAL_SERVER_ERROR);
    r
}

/// Atiende una petición `lfview`. Se ejecuta fuera del hilo de la ventana.
pub fn handle(app: &AppHandle, req: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    let Some(state) = app.try_state::<ViewerState>() else { return plain(503) };
    let text = |name: header::HeaderName| req.headers().get(name).and_then(|v| v.to_str().ok());
    let token = req.uri().path().trim_start_matches('/');
    let out = serve(state.0.tickets(), &ServeRequest { method: req.method().as_str(), token, range: text(header::RANGE), origin: text(header::ORIGIN) });
    let mut r = plain(out.status);
    for (k, v) in out.headers {
        if let Ok(v) = header::HeaderValue::from_str(&v) {
            r.headers_mut().insert(k, v);
        }
    }
    *r.body_mut() = out.body;
    r
}
