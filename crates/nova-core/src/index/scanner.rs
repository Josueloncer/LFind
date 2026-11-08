//! Recorrido de una raíz (unidad o carpeta) hacia el almacén. Un único hilo, por lotes, pausable y cancelable.
//!
//! * Los errores de acceso se registran y se sigue (no detienen el indexado).
//! * No se desciende por enlaces simbólicos ni uniones (evita ciclos); el enlace sí queda indexado.
//! * Las carpetas y extensiones excluidas ni se indexan ni se recorren.

use super::classify::{depth_of, ext_of, path_key};
use super::exclude::ExcludeRules;
use super::model::{IndexedEntry, StoreResult};
use super::store::IndexStore;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Default)]
pub struct ScanCtl {
    pub cancel: AtomicBool,
    pub pause: AtomicBool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScanTick {
    pub scanned: u64,
    pub skipped: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScanOutcome {
    pub scanned: u64,
    pub skipped: u64,
    pub cancelled: bool,
}

pub const BATCH: usize = 2000;

fn ms(t: std::io::Result<SystemTime>) -> Option<i64> {
    let t = t.ok()?;
    Some(match t.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_millis() as i64,
        Err(e) => -(e.duration().as_millis() as i64),
    })
}

fn wait_if_paused(ctl: &ScanCtl) {
    while ctl.pause.load(Ordering::Relaxed) && !ctl.cancel.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(40));
    }
}

/// Indexa `root`. No toca el estado de la raíz en el almacén: eso lo decide quien llama según el resultado.
pub fn scan_root(
    store: &dyn IndexStore,
    root_id: i64,
    root: &Path,
    gen: i64,
    rules: &ExcludeRules,
    ctl: &ScanCtl,
    batch_size: usize,
    yield_ms: u64,
    on_tick: &mut dyn FnMut(ScanTick),
) -> StoreResult<ScanOutcome> {
    let mut batch: Vec<IndexedEntry> = Vec::with_capacity(batch_size.min(BATCH));
    let (mut scanned, mut skipped) = (0u64, 0u64);
    let mut stack: Vec<(PathBuf, u32)> = vec![(root.to_path_buf(), 1)];
    let batch_size = batch_size.max(1);

    macro_rules! flush {
        () => {
            if !batch.is_empty() {
                store.upsert(root_id, gen, &batch)?;
                batch.clear();
                on_tick(ScanTick { scanned, skipped });
                if yield_ms > 0 {
                    std::thread::sleep(Duration::from_millis(yield_ms));
                }
            }
        };
    }

    while let Some((dir, rel)) = stack.pop() {
        wait_if_paused(ctl);
        if ctl.cancel.load(Ordering::Relaxed) {
            flush!();
            return Ok(ScanOutcome { scanned, skipped, cancelled: true });
        }
        let rd = match fs::read_dir(&dir) {
            Ok(r) => r,
            Err(e) => {
                skipped += 1;
                let _ = store.log_issue(root_id, &dir.to_string_lossy(), &e.to_string());
                continue;
            }
        };
        for (n, ent) in rd.enumerate() {
            if n % 512 == 511 {
                wait_if_paused(ctl);
                if ctl.cancel.load(Ordering::Relaxed) {
                    flush!();
                    return Ok(ScanOutcome { scanned, skipped, cancelled: true });
                }
            }
            let ent = match ent {
                Ok(e) => e,
                Err(e) => {
                    skipped += 1;
                    let _ = store.log_issue(root_id, &dir.to_string_lossy(), &e.to_string());
                    continue;
                }
            };
            let name = ent.file_name().to_string_lossy().into_owned();
            let path = ent.path();
            let path_s = path.to_string_lossy().into_owned();
            let ft = match ent.file_type() {
                Ok(t) => t,
                Err(e) => {
                    skipped += 1;
                    let _ = store.log_issue(root_id, &path_s, &e.to_string());
                    continue;
                }
            };
            let is_link = ft.is_symlink();
            // Un enlace se indexa según su destino (carpeta o archivo) pero nunca se recorre.
            let is_dir = if is_link { fs::metadata(&path).map(|m| m.is_dir()).unwrap_or(false) } else { ft.is_dir() };
            let name_lc = name.to_lowercase();
            let key = path_key(&path_s);
            if is_dir && rules.dir_excluded(&key, &name_lc, rel) {
                continue;
            }
            let ext = if is_dir { String::new() } else { ext_of(&name) };
            if !is_dir && rules.ext_excluded(&ext) {
                continue;
            }
            let md = ent.metadata();
            let (size, modified, created, accessed) = match &md {
                Ok(m) => (if is_dir { 0 } else { m.len() }, ms(m.modified()), ms(m.created()), ms(m.accessed())),
                Err(e) => {
                    let _ = store.log_issue(root_id, &path_s, &e.to_string());
                    (0, None, None, None)
                }
            };
            batch.push(IndexedEntry { root_id, depth: depth_of(&key), key, path: path_s, name, name_lc, ext, is_dir, size, modified, created, accessed });
            scanned += 1;
            if is_dir && !is_link {
                stack.push((path, rel + 1));
            }
            if batch.len() >= batch_size {
                flush!();
            }
        }
    }
    flush!();
    Ok(ScanOutcome { scanned, skipped, cancelled: false })
}
