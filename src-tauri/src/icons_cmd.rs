//! Comando de iconos del sistema. La lógica vive en `nova-core::icons`; aquí solo se validan las rutas,
//! se saca el trabajo del hilo de la ventana y se adapta el resultado para la interfaz.

use nova_core::icons::IconService;
use nova_core::{b64, safety};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;

pub struct IconState(pub Arc<IconService>);

impl IconState {
    pub fn svc(&self) -> Arc<IconService> {
        self.0.clone()
    }
}

/// Un lote nunca pasa de aquí: la interfaz envía solo lo visible.
const MAX_BATCH: usize = 512;

#[derive(Deserialize)]
pub struct IconItem {
    pub path: String,
    pub dir: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IconsReply {
    /// Una clave por elemento pedido, en el mismo orden. `null` = sin icono del sistema (la interfaz usa su glifo).
    pub keys: Vec<Option<String>>,
    /// `data:` URL de los iconos que la interfaz no tenía todavía (`have`), por clave.
    pub icons: HashMap<String, String>,
}

/// Iconos del sistema de un lote de elementos a `px` píxeles. `have`: claves compartidas que la interfaz ya
/// conserva (no se reenvían sus bytes).
#[tauri::command]
pub async fn get_icons(items: Vec<IconItem>, px: u32, have: Vec<String>, state: State<'_, IconState>) -> Result<IconsReply, String> {
    if items.len() > MAX_BATCH {
        return Err(format!("Demasiados elementos en un lote ({}).", items.len()));
    }
    let svc = state.svc();
    let r = tauri::async_runtime::spawn_blocking(move || {
        // Las rutas no válidas se quedan con clave `None` sin llegar al servicio.
        let mut valid: Vec<(PathBuf, bool)> = Vec::with_capacity(items.len());
        let mut slot: Vec<Option<usize>> = Vec::with_capacity(items.len());
        for it in &items {
            match safety::parse_path(&it.path) {
                Ok(p) => {
                    slot.push(Some(valid.len()));
                    valid.push((p, it.dir));
                }
                Err(_) => slot.push(None),
            }
        }
        let have: HashSet<String> = have.into_iter().collect();
        let batch = svc.resolve(&valid, px, &have);
        let keys = slot.into_iter().map(|s| s.and_then(|i| batch.keys[i].clone())).collect();
        let icons = batch.icons.into_iter().map(|(k, b)| (k, format!("data:image/png;base64,{}", b64::encode(&b)))).collect();
        IconsReply { keys, icons }
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(r)
}

/// Vacía la caché de iconos (memoria y disco). Devuelve los bytes liberados en disco.
#[tauri::command]
pub async fn clear_icon_cache(state: State<'_, IconState>) -> Result<u64, String> {
    let svc = state.svc();
    tauri::async_runtime::spawn_blocking(move || svc.clear_cache()).await.map_err(|e| e.to_string())
}
