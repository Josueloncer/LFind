//! Eliminación definitiva de la Papelera, por lotes y comprobando el resultado.
//!
//! Windows puede dar por buena una operación sin haber borrado lo que estaba en uso: el resultado real se comprueba volviendo a
//! consultar la Papelera. La planificación no conoce a Windows, así que se prueba sin tocar la Papelera de nadie.

use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

/// Archivos sueltos por operación de Windows: bastantes para no pagar su coste fijo, pocos para que el progreso avance.
pub const CHUNK: usize = 50;
/// Sin ningún archivo borrado durante este tiempo, Windows se da por bloqueado. Una carpeta enorme avanza aunque tarde.
pub const STALL: Duration = Duration::from_secs(120);
const CANCEL_GRACE: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(100);
const WALK_LIMIT: usize = 5_000;
const COUNT_LIMIT: u64 = 2_000_000;

const NO_RESPONSE: &str = "Windows no respondió a tiempo.";
const NOT_ALLOWED: &str = "Windows no permitió eliminarlo.";

fn now_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// Señal de vida de una operación en curso, compartida con el hilo que habla con Windows.
#[derive(Debug, Default)]
pub struct Beat {
    files: AtomicU64,
    expected: AtomicU64,
    last: AtomicU64,
}

impl Beat {
    pub fn alive(&self) {
        self.last.store(now_ms(), Ordering::Relaxed);
    }
    pub fn file_done(&self) {
        self.files.fetch_add(1, Ordering::Relaxed);
        self.alive();
    }
    /// Cuántos archivos contiene lo que se va a borrar, si se sabe.
    pub fn expect(&self, files: u64) {
        self.expected.store(files, Ordering::Relaxed);
        self.alive();
    }
    fn quiet_for(&self) -> Duration {
        Duration::from_millis(now_ms().saturating_sub(self.last.load(Ordering::Relaxed)))
    }
}

/// Lo que Windows ejecuta de una vez. Una carpeta va siempre sola y no se interrumpe: cancelar no la deja a medias.
#[derive(Debug)]
pub struct Batch<T> {
    pub items: Vec<T>,
    pub whole: bool,
    pub label: String,
}

/// Agrupa respetando el orden: los archivos seguidos comparten operación y cada carpeta lleva la suya.
pub fn batches<T>(items: Vec<T>, chunk: usize, is_folder: impl Fn(&T) -> bool, name: impl Fn(&T) -> String) -> Vec<Batch<T>> {
    let mut out: Vec<Batch<T>> = Vec::new();
    for item in items {
        let label = name(&item);
        if is_folder(&item) {
            out.push(Batch { items: vec![item], whole: true, label });
        } else if let Some(open) = out.last_mut().filter(|b| !b.whole && b.items.len() < chunk.max(1)) {
            open.label = label;
            open.items.push(item);
        } else {
            out.push(Batch { items: vec![item], whole: false, label });
        }
    }
    out
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    /// Archivos borrados en lo que se está procesando.
    pub files: u64,
    pub files_total: Option<u64>,
    pub current: String,
    /// Es una carpeta: si se cancela, se termina antes de parar.
    pub whole: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunEnd {
    Finished,
    Cancelled,
    TimedOut,
}

#[derive(Debug)]
pub struct PurgeRun {
    /// Elementos entregados a Windows en lotes que terminaron o se dieron por perdidos. Se cuentan desde el principio de la lista.
    pub attempted: usize,
    pub end: RunEnd,
    pub error: Option<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PurgeFailure {
    pub id: String,
    pub name: String,
    pub reason: String,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PurgeReport {
    pub deleted: usize,
    pub failed: Vec<PurgeFailure>,
    /// Sin intentar por haber cancelado o porque Windows dejó de responder.
    pub not_attempted: usize,
    pub cancelled: bool,
}

/// Cada lote corre en su propio hilo y da señales de vida: si Windows se bloquea, la interfaz recibe una respuesta igualmente.
/// `purge` recibe si puede interrumpirse (solo entre archivos sueltos).
pub fn purge_batches<T, P>(batches: Vec<Batch<T>>, purge: P, cancel: &AtomicBool, stall: Duration, mut progress: impl FnMut(Progress)) -> PurgeRun
where
    T: Send + 'static,
    P: Fn(Vec<T>, Arc<Beat>, bool) -> Result<(), String> + Send + Sync + 'static,
{
    let total: usize = batches.iter().map(|b| b.items.len()).sum();
    let purge = Arc::new(purge);
    let mut attempted = 0;
    let mut error = None;
    for batch in batches {
        if cancel.load(Ordering::Relaxed) {
            return PurgeRun { attempted, end: RunEnd::Cancelled, error };
        }
        let Batch { items, whole, label } = batch;
        let size = items.len();
        let beat = Arc::new(Beat::default());
        beat.alive();
        let (tx, rx) = channel();
        let (worker, life) = (purge.clone(), beat.clone());
        let spawned = std::thread::Builder::new().name("lfind-recycle-purge".into()).spawn(move || {
            let _ = tx.send(worker(items, life, !whole));
        });
        if let Err(e) = spawned {
            return PurgeRun { attempted, end: RunEnd::Finished, error: Some(e.to_string()) };
        }
        let mut sent = None;
        loop {
            let files = beat.files.load(Ordering::Relaxed);
            let files_total = Some(beat.expected.load(Ordering::Relaxed)).filter(|n| *n > 0);
            if sent != Some((files, files_total)) {
                sent = Some((files, files_total));
                progress(Progress { done: attempted, total, files, files_total, current: label.clone(), whole });
            }
            match rx.recv_timeout(POLL) {
                Ok(Ok(())) => break,
                Ok(Err(e)) => {
                    error = Some(e);
                    break;
                }
                Err(RecvTimeoutError::Disconnected) => {
                    error = Some("Error interno al acceder a la Papelera.".into());
                    break;
                }
                Err(RecvTimeoutError::Timeout) => {
                    let quiet = beat.quiet_for();
                    if cancel.load(Ordering::Relaxed) && quiet >= CANCEL_GRACE {
                        return PurgeRun { attempted, end: RunEnd::Cancelled, error };
                    }
                    if quiet >= stall {
                        return PurgeRun { attempted: attempted + size, end: RunEnd::TimedOut, error };
                    }
                }
            }
        }
        attempted += size;
    }
    progress(Progress { done: attempted, total, files: 0, files_total: None, current: String::new(), whole: false });
    PurgeRun { attempted, end: if cancel.load(Ordering::Relaxed) && attempted < total { RunEnd::Cancelled } else { RunEnd::Finished }, error }
}

/// Lo que de verdad ha pasado: `requested` va en el orden en que se entregó, y `still` es lo que la Papelera sigue teniendo.
pub fn build_report(requested: &[(String, String)], run: &PurgeRun, still: &HashSet<String>, reason: impl Fn(&str) -> Option<String>) -> PurgeReport {
    let fallback = match (&run.error, run.end) {
        (Some(e), _) => e.clone(),
        (None, RunEnd::TimedOut) => NO_RESPONSE.to_string(),
        _ => NOT_ALLOWED.to_string(),
    };
    let mut report = PurgeReport { deleted: 0, failed: Vec::new(), not_attempted: 0, cancelled: run.end == RunEnd::Cancelled };
    for (i, (id, name)) in requested.iter().enumerate() {
        if !still.contains(id) {
            report.deleted += 1;
        } else if i < run.attempted {
            report.failed.push(PurgeFailure { id: id.clone(), name: name.clone(), reason: reason(id).unwrap_or_else(|| fallback.clone()) });
        } else {
            report.not_attempted += 1;
        }
    }
    report
}

/// Ruta real de un elemento dentro de `$Recycle.Bin`, a partir de su identificador en el shell.
pub fn physical_path(id: &str) -> Option<PathBuf> {
    let at = id.to_ascii_lowercase().find(":\\$recycle.bin\\").filter(|a| *a > 0)?;
    let start = at - 1;
    id.is_char_boundary(start).then(|| PathBuf::from(&id[start..]))
}

/// Elementos que Windows irá notificando al borrar una carpeta: sus archivos, sus subcarpetas y ella misma.
/// No sigue enlaces ni lee contenido. Con demasiados, o si no se puede leer, no se da cifra.
pub fn count_entries(root: &Path, beat: &Beat) -> Option<u64> {
    let mut pending = vec![root.to_path_buf()];
    let mut total = 1u64;
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).ok()?.flatten() {
            total += 1;
            if total > COUNT_LIMIT {
                return None;
            }
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                pending.push(entry.path());
            }
        }
        beat.alive();
    }
    Some(total)
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Block {
    InUse,
    Denied,
}

/// Comprueba si se podría borrar pidiendo solo ese permiso: no lee el contenido, y por eso no descarga lo que esté en la nube.
#[cfg(windows)]
fn block_of(path: &Path) -> Option<Block> {
    use std::os::windows::fs::OpenOptionsExt;
    const DELETE: u32 = 0x0001_0000;
    const FILE_READ_ATTRIBUTES: u32 = 0x0080;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    let open = std::fs::OpenOptions::new().access_mode(DELETE | FILE_READ_ATTRIBUTES).share_mode(0).custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT).open(path);
    match open.err()?.raw_os_error() {
        Some(32 | 33) => Some(Block::InUse),
        Some(5) => Some(Block::Denied),
        _ => None,
    }
}
#[cfg(not(windows))]
fn block_of(_path: &Path) -> Option<Block> {
    None
}

fn first_block_inside(root: &Path) -> Option<(Block, String)> {
    let mut pending = vec![root.to_path_buf()];
    let mut seen = 0;
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).ok()?.flatten() {
            seen += 1;
            if seen > WALK_LIMIT {
                return None;
            }
            let path = entry.path();
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                pending.push(path);
            } else if let Some(block) = block_of(&path) {
                return Some((block, entry.file_name().to_string_lossy().into_owned()));
            }
        }
    }
    None
}

/// Por qué Windows no ha podido enviar esto a la Papelera, cuando se puede saber. Una carpeta va entera o no va.
pub fn trash_blocked_reason(path: &Path) -> Option<String> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    if meta.is_dir() {
        return first_block_inside(path).map(|(block, name)| match block {
            Block::InUse => format!("Contiene un archivo en uso: {name}. Ciérralo e inténtalo de nuevo."),
            Block::Denied => format!("No hay permiso para mover parte de su contenido: {name}."),
        });
    }
    block_of(path).map(|block| match block {
        Block::InUse => "Está en uso por otra aplicación. Ciérrala e inténtalo de nuevo.".to_string(),
        Block::Denied => "No hay permiso para moverlo a la Papelera.".to_string(),
    })
}

/// Por qué Windows no ha podido borrar esto, cuando se puede saber.
pub fn blocked_reason(path: &Path) -> Option<String> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    if meta.is_dir() {
        return first_block_inside(path).map(|(block, name)| match block {
            Block::InUse => format!("Contiene un archivo en uso: {name}. Lo demás ya se ha borrado."),
            Block::Denied => format!("No hay permiso para borrar parte de su contenido: {name}."),
        });
    }
    block_of(path).map(|block| match block {
        Block::InUse => "Está en uso por otra aplicación. Ciérrala e inténtalo de nuevo.".to_string(),
        Block::Denied => "No hay permiso para borrarlo.".to_string(),
    })
}

#[cfg(windows)]
mod shell {
    use super::Beat;
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::Arc;

    type P = *mut c_void;
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Guid(u32, u16, u16, [u8; 8]);
    impl PartialEq for Guid {
        fn eq(&self, o: &Guid) -> bool {
            (self.0, self.1, self.2, self.3) == (o.0, o.1, o.2, o.3)
        }
    }

    const CLSID_FILE_OPERATION: Guid = Guid(0x3AD05575, 0x8857, 0x4850, [0x92, 0x77, 0x11, 0xB8, 0x5B, 0xDB, 0x8E, 0x09]);
    const IID_FILE_OPERATION: Guid = Guid(0x947AAB5F, 0x0A5C, 0x4C13, [0xB4, 0xD6, 0x4B, 0xF7, 0x83, 0x6F, 0xC9, 0xF8]);
    const IID_SINK: Guid = Guid(0x04B0F1A7, 0x9490, 0x44BC, [0x96, 0xE1, 0x42, 0x96, 0xA3, 0x12, 0x52, 0xE2]);
    const IID_SHELL_ITEM: Guid = Guid(0x43826D1E, 0xE718, 0x42EE, [0xBC, 0x55, 0xA1, 0xE2, 0x61, 0xC3, 0x7B, 0xFE]);
    const IID_UNKNOWN: Guid = Guid(0, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);

    const S_OK: i32 = 0;
    const E_NOINTERFACE: i32 = 0x8000_4002_u32 as i32;
    const E_ABORT: i32 = 0x8000_4004_u32 as i32;
    const ERROR_CANCELLED: i32 = 0x8007_04C7_u32 as i32;
    const USER_CANCELLED: i32 = 0x8027_0000_u32 as i32;
    const CLSCTX_ALL: u32 = 0x17;
    const COINIT_STA_NO_DDE: u32 = 0x2 | 0x4;
    const FOF_NO_UI: u32 = 0x0614;

    #[link(name = "ole32")]
    extern "system" {
        fn CoInitializeEx(reserved: P, flags: u32) -> i32;
        fn CoUninitialize();
        fn CoCreateInstance(clsid: *const Guid, outer: P, ctx: u32, iid: *const Guid, out: *mut P) -> i32;
    }
    #[link(name = "shell32")]
    extern "system" {
        fn SHCreateItemFromParsingName(path: *const u16, ctx: P, iid: *const Guid, out: *mut P) -> i32;
    }

    #[repr(C)]
    struct UnknownVtbl {
        query: P,
        add_ref: P,
        release: unsafe extern "system" fn(P) -> u32,
    }

    #[repr(C)]
    struct FileOperationVtbl {
        unknown: UnknownVtbl,
        advise: unsafe extern "system" fn(P, P, *mut u32) -> i32,
        unadvise: unsafe extern "system" fn(P, u32) -> i32,
        set_flags: unsafe extern "system" fn(P, u32) -> i32,
        _set_message: P,
        _set_dialog: P,
        _set_properties: P,
        _set_owner: P,
        _apply_item: P,
        _apply_items: P,
        _rename_item: P,
        _rename_items: P,
        _move_item: P,
        _move_items: P,
        _copy_item: P,
        _copy_items: P,
        delete_item: unsafe extern "system" fn(P, P, P) -> i32,
        _delete_items: P,
        _new_item: P,
        perform: unsafe extern "system" fn(P) -> i32,
    }

    #[repr(C)]
    struct SinkVtbl {
        query: unsafe extern "system" fn(P, *const Guid, *mut P) -> i32,
        add_ref: unsafe extern "system" fn(P) -> u32,
        release: unsafe extern "system" fn(P) -> u32,
        start: unsafe extern "system" fn(P) -> i32,
        finish: unsafe extern "system" fn(P, i32) -> i32,
        pre_rename: unsafe extern "system" fn(P, u32, P, P) -> i32,
        post_rename: unsafe extern "system" fn(P, u32, P, P, i32, P) -> i32,
        pre_move: unsafe extern "system" fn(P, u32, P, P, P) -> i32,
        post_move: unsafe extern "system" fn(P, u32, P, P, P, i32, P) -> i32,
        pre_copy: unsafe extern "system" fn(P, u32, P, P, P) -> i32,
        post_copy: unsafe extern "system" fn(P, u32, P, P, P, i32, P) -> i32,
        pre_delete: unsafe extern "system" fn(P, u32, P) -> i32,
        post_delete: unsafe extern "system" fn(P, u32, P, i32, P) -> i32,
        pre_new: unsafe extern "system" fn(P, u32, P, P) -> i32,
        post_new: unsafe extern "system" fn(P, u32, P, P, P, u32, i32, P) -> i32,
        progress: unsafe extern "system" fn(P, u32, u32) -> i32,
        reset: unsafe extern "system" fn(P) -> i32,
        pause: unsafe extern "system" fn(P) -> i32,
        resume: unsafe extern "system" fn(P) -> i32,
    }

    #[repr(C)]
    struct Sink {
        vtbl: *const SinkVtbl,
        refs: AtomicU32,
        beat: Arc<Beat>,
        abort: Option<&'static AtomicBool>,
    }

    unsafe extern "system" fn query(this: P, iid: *const Guid, out: *mut P) -> i32 {
        if *iid == IID_UNKNOWN || *iid == IID_SINK {
            add_ref(this);
            *out = this;
            S_OK
        } else {
            *out = std::ptr::null_mut();
            E_NOINTERFACE
        }
    }
    unsafe extern "system" fn add_ref(this: P) -> u32 {
        (*(this as *mut Sink)).refs.fetch_add(1, Ordering::SeqCst) + 1
    }
    unsafe extern "system" fn release(this: P) -> u32 {
        let left = (*(this as *mut Sink)).refs.fetch_sub(1, Ordering::SeqCst) - 1;
        if left == 0 {
            drop(Box::from_raw(this as *mut Sink));
        }
        left
    }
    unsafe extern "system" fn none1(_: P) -> i32 {
        S_OK
    }
    unsafe extern "system" fn finish(_: P, _: i32) -> i32 {
        S_OK
    }
    unsafe extern "system" fn none4(_: P, _: u32, _: P, _: P) -> i32 {
        S_OK
    }
    unsafe extern "system" fn none5(_: P, _: u32, _: P, _: P, _: P) -> i32 {
        S_OK
    }
    unsafe extern "system" fn none6(_: P, _: u32, _: P, _: P, _: i32, _: P) -> i32 {
        S_OK
    }
    unsafe extern "system" fn none7(_: P, _: u32, _: P, _: P, _: P, _: i32, _: P) -> i32 {
        S_OK
    }
    unsafe extern "system" fn none8(_: P, _: u32, _: P, _: P, _: P, _: u32, _: i32, _: P) -> i32 {
        S_OK
    }
    unsafe extern "system" fn pre_delete(this: P, _: u32, _: P) -> i32 {
        let s = &*(this as *mut Sink);
        s.beat.alive();
        if s.abort.is_some_and(|a| a.load(Ordering::Relaxed)) {
            E_ABORT
        } else {
            S_OK
        }
    }
    unsafe extern "system" fn post_delete(this: P, _: u32, _: P, _: i32, _: P) -> i32 {
        (*(this as *mut Sink)).beat.file_done();
        S_OK
    }
    unsafe extern "system" fn progress(this: P, _: u32, _: u32) -> i32 {
        (*(this as *mut Sink)).beat.alive();
        S_OK
    }

    static SINK_VTBL: SinkVtbl = SinkVtbl {
        query,
        add_ref,
        release,
        start: none1,
        finish,
        pre_rename: none4,
        post_rename: none6,
        pre_move: none5,
        post_move: none7,
        pre_copy: none5,
        post_copy: none7,
        pre_delete,
        post_delete,
        pre_new: none4,
        post_new: none8,
        progress,
        reset: none1,
        pause: none1,
        resume: none1,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Borra de forma definitiva lo que el shell identifica con esos nombres. `abort`, si se indica, detiene entre un archivo y otro.
    pub fn delete_items(ids: &[String], beat: Arc<Beat>, abort: Option<&'static AtomicBool>) -> Result<(), String> {
        unsafe {
            let init = CoInitializeEx(std::ptr::null_mut(), COINIT_STA_NO_DDE);
            if init < 0 {
                return Err(format!("Windows no pudo preparar la operación ({init:#x})."));
            }
            let result = run(ids, beat, abort);
            CoUninitialize();
            result
        }
    }

    unsafe fn run(ids: &[String], beat: Arc<Beat>, abort: Option<&'static AtomicBool>) -> Result<(), String> {
        let mut op: P = std::ptr::null_mut();
        let hr = CoCreateInstance(&CLSID_FILE_OPERATION, std::ptr::null_mut(), CLSCTX_ALL, &IID_FILE_OPERATION, &mut op);
        if hr < 0 || op.is_null() {
            return Err(format!("Windows no pudo preparar la operación ({hr:#x})."));
        }
        let vt = &**(op as *mut *const FileOperationVtbl);
        let sink = Box::into_raw(Box::new(Sink { vtbl: &SINK_VTBL, refs: AtomicU32::new(1), beat, abort })) as P;
        let mut cookie = 0u32;
        let mut result = match (vt.set_flags)(op, FOF_NO_UI) {
            hr if hr < 0 => Err(format!("Windows no pudo preparar la operación ({hr:#x}).")),
            _ => Ok(()),
        };
        let advised = result.is_ok() && (vt.advise)(op, sink, &mut cookie) >= 0;
        if result.is_ok() {
            let mut queued = 0;
            for id in ids {
                let w = wide(id);
                let mut item: P = std::ptr::null_mut();
                if SHCreateItemFromParsingName(w.as_ptr(), std::ptr::null_mut(), &IID_SHELL_ITEM, &mut item) < 0 || item.is_null() {
                    continue; // ya no existe: la comprobación final lo aclara
                }
                if (vt.delete_item)(op, item, std::ptr::null_mut()) >= 0 {
                    queued += 1;
                }
                ((**(item as *mut *const UnknownVtbl)).release)(item);
            }
            if queued > 0 {
                let hr = (vt.perform)(op);
                if hr < 0 && hr != E_ABORT && hr != ERROR_CANCELLED && hr != USER_CANCELLED {
                    result = Err(format!("Windows no pudo completar la eliminación ({hr:#x})."));
                }
            }
        }
        if advised {
            (vt.unadvise)(op, cookie);
        }
        release(sink);
        (vt.unknown.release)(op);
        result
    }
}

#[cfg(windows)]
pub use shell::delete_items;

#[cfg(not(windows))]
pub fn delete_items(_ids: &[String], _beat: Arc<Beat>, _abort: Option<&'static AtomicBool>) -> Result<(), String> {
    Err("Solo disponible en Windows.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn requested(n: usize) -> Vec<(String, String)> {
        (0..n).map(|i| (format!("id{i}"), format!("n{i}"))).collect()
    }
    fn still(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }
    fn files(n: u32) -> Vec<Batch<u32>> {
        vec![Batch { items: (0..n).collect(), whole: false, label: "x".into() }]
    }
    const LONG: Duration = Duration::from_secs(60);
    fn never() -> AtomicBool {
        AtomicBool::new(false)
    }
    fn plain(items: Vec<u32>, chunk: usize) -> Vec<Batch<u32>> {
        batches(items, chunk, |_| false, |i| i.to_string())
    }

    #[test]
    fn each_folder_goes_alone_and_the_order_is_kept() {
        let b = batches(vec![1u32, 2, 100, 3, 4, 5, 101], 2, |i| *i >= 100, |i| format!("e{i}"));
        let shape: Vec<(Vec<u32>, bool)> = b.iter().map(|x| (x.items.clone(), x.whole)).collect();
        assert_eq!(shape, vec![(vec![1, 2], false), (vec![100], true), (vec![3, 4], false), (vec![5], false), (vec![101], true)]);
        assert_eq!(b[1].label, "e100");
    }

    #[test]
    fn reports_files_inside_a_folder_while_it_is_deleted() {
        let seen = Mutex::new(Vec::new());
        let batch = vec![Batch { items: vec![1u32], whole: true, label: "grande".into() }];
        let run = purge_batches(batch, |_, beat, may_abort| {
            assert!(!may_abort, "una carpeta no se interrumpe");
            beat.expect(3);
            for _ in 0..3 {
                beat.file_done();
                std::thread::sleep(Duration::from_millis(130));
            }
            Ok(())
        }, &never(), LONG, |p| seen.lock().unwrap().push(p));
        assert_eq!((run.attempted, run.end), (1, RunEnd::Finished));
        let seen = seen.into_inner().unwrap();
        assert!(seen.iter().any(|p| p.files >= 1 && p.files_total == Some(3) && p.current == "grande" && p.whole), "{seen:?}");
        assert_eq!(seen.last().map(|p| (p.done, p.total)), Some((1, 1)));
    }

    #[test]
    fn progress_counts_items_processed() {
        let seen = Mutex::new(Vec::new());
        purge_batches(plain((0..7).collect(), 3), |_, _, _| { std::thread::sleep(Duration::from_millis(5)); Ok(()) }, &never(), LONG, |p| seen.lock().unwrap().push(p.done));
        let mut v = seen.into_inner().unwrap();
        v.dedup();
        assert_eq!(v, vec![0, 3, 6, 7]);
    }

    #[test]
    fn an_empty_list_touches_nothing() {
        let run = purge_batches(Vec::<Batch<u32>>::new(), |_, _, _| panic!("no debe llamarse"), &never(), LONG, |_| {});
        assert_eq!((run.attempted, run.end), (0, RunEnd::Finished));
    }

    #[test]
    fn a_failed_batch_does_not_stop_the_rest_and_keeps_its_error() {
        let run = purge_batches(plain((0..6).collect(), 2), |b, _, _| if b[0] == 2 { Err("fallo".to_string()) } else { Ok(()) }, &never(), LONG, |_| {});
        assert_eq!((run.attempted, run.end, run.error.as_deref()), (6, RunEnd::Finished, Some("fallo")));
    }

    #[test]
    fn cancelling_leaves_the_following_items_alone() {
        let cancel = Arc::new(AtomicBool::new(false));
        let c = cancel.clone();
        let calls = Arc::new(Mutex::new(0));
        let k = calls.clone();
        let run = purge_batches(plain((0..6).collect(), 2), move |_, _, _| { *k.lock().unwrap() += 1; c.store(true, Ordering::Relaxed); Ok(()) }, &cancel, LONG, |_| {});
        assert_eq!((run.attempted, run.end, *calls.lock().unwrap()), (2, RunEnd::Cancelled, 1));
    }

    #[test]
    fn cancelling_waits_for_a_folder_that_is_still_advancing() {
        let cancel = Arc::new(AtomicBool::new(false));
        let c = cancel.clone();
        let finished = Arc::new(AtomicBool::new(false));
        let f = finished.clone();
        let batch = vec![Batch { items: vec![1u32], whole: true, label: "c".into() }, Batch { items: vec![2u32], whole: true, label: "d".into() }];
        let run = purge_batches(batch, move |items, beat, _| {
            c.store(true, Ordering::Relaxed);
            assert_eq!(items, vec![1], "lo siguiente no se empieza");
            for _ in 0..6 {
                beat.file_done();
                std::thread::sleep(Duration::from_millis(60));
            }
            f.store(true, Ordering::Relaxed);
            Ok(())
        }, &cancel, LONG, |_| {});
        assert!(finished.load(Ordering::Relaxed), "la carpeta termina: nada queda a medias");
        assert_eq!((run.attempted, run.end), (1, RunEnd::Cancelled));
    }

    #[test]
    fn cancelling_answers_if_windows_stops_responding() {
        let cancel = Arc::new(AtomicBool::new(false));
        let c = cancel.clone();
        std::thread::spawn(move || { std::thread::sleep(Duration::from_millis(200)); c.store(true, Ordering::Relaxed); });
        let t = Instant::now();
        let run = purge_batches(files(1), |_, _, _| { std::thread::sleep(Duration::from_secs(40)); Ok(()) }, &cancel, LONG, |_| {});
        assert!(t.elapsed() < Duration::from_secs(10), "{:?}", t.elapsed());
        assert_eq!((run.attempted, run.end), (0, RunEnd::Cancelled));
    }

    #[test]
    fn a_stuck_batch_gives_up_but_a_slow_one_does_not() {
        let run = purge_batches(files(2), |_, _, _| { std::thread::sleep(Duration::from_secs(40)); Ok(()) }, &never(), Duration::from_millis(250), |_| {});
        assert_eq!((run.attempted, run.end), (2, RunEnd::TimedOut));
        let run = purge_batches(files(2), |_, beat, _| {
            for _ in 0..8 {
                std::thread::sleep(Duration::from_millis(100));
                beat.file_done();
            }
            Ok(())
        }, &never(), Duration::from_millis(400), |_| {});
        assert_eq!(run.end, RunEnd::Finished);
    }

    #[test]
    fn a_panic_in_the_worker_is_an_error_not_a_hang() {
        let run = purge_batches(files(1), |_, _, _| panic!("prueba"), &never(), LONG, |_| {});
        assert_eq!((run.attempted, run.end), (1, RunEnd::Finished));
        assert!(run.error.is_some());
    }

    #[test]
    fn success_reported_by_windows_is_not_believed_without_checking() {
        let run = PurgeRun { attempted: 3, end: RunEnd::Finished, error: None };
        let r = build_report(&requested(3), &run, &still(&["id1"]), |id| Some(format!("motivo {id}")));
        assert_eq!((r.deleted, r.not_attempted, r.cancelled), (2, 0, false));
        assert_eq!(r.failed, vec![PurgeFailure { id: "id1".into(), name: "n1".into(), reason: "motivo id1".into() }]);
    }

    #[test]
    fn what_was_never_attempted_is_not_a_failure() {
        let run = PurgeRun { attempted: 2, end: RunEnd::Cancelled, error: None };
        let r = build_report(&requested(4), &run, &still(&["id2", "id3"]), |_| None);
        assert_eq!((r.deleted, r.failed.len(), r.not_attempted, r.cancelled), (2, 0, 2, true));
    }

    #[test]
    fn a_batch_given_up_after_cancelling_counts_what_windows_did_finish() {
        let run = PurgeRun { attempted: 0, end: RunEnd::Cancelled, error: None };
        let r = build_report(&requested(2), &run, &still(&["id1"]), |_| None);
        assert_eq!((r.deleted, r.failed.len(), r.not_attempted), (1, 0, 1));
    }

    #[test]
    fn the_reason_falls_back_to_what_is_known() {
        let timed = PurgeRun { attempted: 1, end: RunEnd::TimedOut, error: None };
        assert_eq!(build_report(&requested(1), &timed, &still(&["id0"]), |_| None).failed[0].reason, NO_RESPONSE);
        let failed = PurgeRun { attempted: 1, end: RunEnd::Finished, error: Some("detalle".into()) };
        assert_eq!(build_report(&requested(1), &failed, &still(&["id0"]), |_| None).failed[0].reason, "detalle");
        let plain = PurgeRun { attempted: 1, end: RunEnd::Finished, error: None };
        assert_eq!(build_report(&requested(1), &plain, &still(&["id0"]), |_| None).failed[0].reason, NOT_ALLOWED);
    }

    #[test]
    fn finds_the_real_path_of_an_item() {
        let p = physical_path("\\\\?\\C:\\$Recycle.Bin\\S-1-5\\$RABC.txt").unwrap();
        assert_eq!(p, PathBuf::from("C:\\$Recycle.Bin\\S-1-5\\$RABC.txt"));
        assert_eq!(physical_path("::{645FF040-5081-101B-9F08-00AA002F954E}\\x"), None);
        assert_eq!(physical_path(":\\$recycle.bin\\x"), None);
    }

    #[test]
    fn counts_nested_entries() {
        let d = std::env::temp_dir().join(format!("lfind-count-{}", std::process::id()));
        std::fs::create_dir_all(d.join("a").join("b")).unwrap();
        for (i, sub) in ["", "a", "a/b"].iter().enumerate() {
            std::fs::write(d.join(sub).join(format!("{i}.txt")), b"x").unwrap();
        }
        assert_eq!(count_entries(&d, &Beat::default()), Some(6), "3 archivos, 2 subcarpetas y la propia carpeta");
        std::fs::remove_dir_all(d).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn explains_a_file_in_use_and_finds_one_inside_a_folder() {
        use std::os::windows::fs::OpenOptionsExt;
        let d = std::env::temp_dir().join(format!("lfind-recycle-{}", std::process::id()));
        std::fs::create_dir_all(d.join("sub")).unwrap();
        let f = d.join("sub").join("bloqueado.txt");
        std::fs::write(&f, b"x").unwrap();
        // Un antivirus puede tener el archivo recién creado abierto un instante.
        let settles = |p: &Path| (0..60).any(|_| blocked_reason(p).is_none() || { std::thread::sleep(Duration::from_millis(50)); false });
        assert!(settles(&f));
        assert!(settles(&d));
        let hold = std::fs::OpenOptions::new().read(true).share_mode(0).open(&f).unwrap();
        assert!(blocked_reason(&f).unwrap().contains("en uso"));
        assert!(blocked_reason(&d).unwrap().contains("bloqueado.txt"));
        drop(hold);
        assert!(settles(&d));
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn something_that_no_longer_exists_has_no_reason() {
        assert_eq!(blocked_reason(Path::new("C:\\no\\existe\\lfind.txt")), None);
    }
}
