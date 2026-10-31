//! Comandos de miniaturas y vista previa. Toda la lógica vive en `nova-core::thumbs`; aquí solo se
//! valida la ruta, se saca el trabajo del hilo de la ventana y se adapta el resultado para la interfaz.

use nova_core::thumbs::ThumbService;
use nova_core::{b64, preview, safety};
use serde::Serialize;
use std::sync::Arc;
use tauri::State;

pub struct ThumbState(pub Arc<ThumbService>);

impl ThumbState {
    /// Método explícito: evita confundir `state.0` con el campo interno de `tauri::State`.
    pub fn svc(&self) -> Arc<ThumbService> {
        self.0.clone()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThumbReply {
    /// `data:` URL lista para un `<img>` (la CSP de la app ya permite `img-src data:`).
    pub data_url: String,
}

#[derive(Serialize)]
pub struct ThumbFail {
    /// "unsupported" | "toolarge" | "cancelled" | "failed"
    pub kind: &'static str,
    pub message: String,
}

fn fail(m: impl Into<String>) -> ThumbFail {
    ThumbFail { kind: "failed", message: m.into() }
}

/// Miniatura de `path` a `px` píxeles. `epoch`: las solicitudes con una época anterior a la vigente
/// se descartan sin trabajo (se cambió de carpeta). `None` = no caduca (panel de vista previa).
#[tauri::command]
pub async fn get_thumbnail(path: String, px: u32, epoch: Option<u64>, state: State<'_, ThumbState>) -> Result<ThumbReply, ThumbFail> {
    let p = safety::parse_path(&path).map_err(fail)?;
    let svc = state.svc();
    let r = tauri::async_runtime::spawn_blocking(move || svc.get(&p, px, epoch)).await.map_err(|e| fail(e.to_string()))?;
    match r {
        Ok(t) => Ok(ThumbReply { data_url: format!("data:{};base64,{}", t.mime, b64::encode(&t.bytes)) }),
        Err(e) => Err(ThumbFail { kind: e.kind(), message: e.message() }),
    }
}

#[tauri::command]
pub fn set_thumb_epoch(epoch: u64, state: State<'_, ThumbState>) {
    state.svc().set_epoch(epoch);
}

/// Vacía la caché de miniaturas (también los marcadores de «sin proveedor»). Devuelve los bytes liberados.
#[tauri::command]
pub async fn clear_thumb_cache(state: State<'_, ThumbState>) -> Result<u64, String> {
    let svc = state.svc();
    tauri::async_runtime::spawn_blocking(move || svc.clear_cache()).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_text_preview(path: String) -> Result<preview::TextPreview, String> {
    let p = safety::parse_path(&path)?;
    tauri::async_runtime::spawn_blocking(move || preview::text_preview(&p, preview::MAX_PREVIEW_BYTES)).await.map_err(|e| e.to_string())?
}
