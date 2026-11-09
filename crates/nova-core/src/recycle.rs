//! Eliminación definitiva de la Papelera, por tandas y comprobando el resultado.
//!
//! Windows puede dar por buena una operación sin haber borrado lo que estaba en uso: el resultado real se comprueba volviendo a
//! consultar la Papelera. Esta parte no conoce la biblioteca que borra, así que se prueba sin tocar la Papelera de nadie.

use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Elementos por operación de Windows: bastantes para no pagar el coste fijo de cada una, pocos para poder cancelar pronto.
pub const CHUNK: usize = 25;
/// Una tanda sin carpetas no debería tardar esto ni con el disco ocupado. Las carpetas no tienen límite: se pueden cancelar.
pub const FILES_LIMIT: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(100);
const CANCEL_GRACE: Duration = Duration::from_secs(2);
const WALK_LIMIT: usize = 5_000;

const NO_RESPONSE: &str = "Windows no respondió a tiempo.";
const NOT_ALLOWED: &str = "Windows no permitió eliminarlo.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunEnd {
    Finished,
    Cancelled,
    TimedOut,
}

#[derive(Debug)]
pub struct PurgeRun {
    /// Elementos entregados a Windows en tandas que terminaron o agotaron su tiempo. Se cuentan desde el principio de la lista.
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

/// Cada tanda corre en su propio hilo: si Windows se queda sin responder, la interfaz recibe una respuesta igualmente.
pub fn purge_chunks<T, P>(mut items: Vec<T>, chunk: usize, limit_of: impl Fn(&[T]) -> Option<Duration>, purge: P, cancel: &AtomicBool, mut progress: impl FnMut(usize, usize)) -> PurgeRun
where
    T: Send + 'static,
    P: Fn(Vec<T>) -> Result<(), String> + Send + Sync + 'static,
{
    let total = items.len();
    let purge = Arc::new(purge);
    let mut attempted = 0;
    let mut error = None;
    progress(0, total);
    while !items.is_empty() {
        if cancel.load(Ordering::Relaxed) {
            return PurgeRun { attempted, end: RunEnd::Cancelled, error };
        }
        let rest = items.split_off(chunk.max(1).min(items.len()));
        let batch = std::mem::replace(&mut items, rest);
        let size = batch.len();
        let limit = limit_of(&batch);
        let (tx, rx) = channel();
        let worker = purge.clone();
        let spawned = std::thread::Builder::new().name("lfind-recycle-purge".into()).spawn(move || {
            let _ = tx.send(worker(batch));
        });
        if let Err(e) = spawned {
            return PurgeRun { attempted, end: RunEnd::Finished, error: Some(e.to_string()) };
        }
        let started = Instant::now();
        loop {
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
                    if cancel.load(Ordering::Relaxed) {
                        // La tanda en curso casi siempre acaba enseguida: esperarla un momento da un recuento exacto.
                        return match rx.recv_timeout(CANCEL_GRACE) {
                            Ok(result) => {
                                if let Err(e) = result {
                                    error = Some(e);
                                }
                                attempted += size;
                                progress(attempted, total);
                                PurgeRun { attempted, end: RunEnd::Cancelled, error }
                            }
                            Err(_) => PurgeRun { attempted, end: RunEnd::Cancelled, error },
                        };
                    }
                    if limit.is_some_and(|l| started.elapsed() >= l) {
                        return PurgeRun { attempted: attempted + size, end: RunEnd::TimedOut, error };
                    }
                }
            }
        }
        attempted += size;
        progress(attempted, total);
    }
    PurgeRun { attempted, end: RunEnd::Finished, error }
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
    fn none(_: &[u32]) -> Option<Duration> {
        None
    }

    #[test]
    fn splits_in_chunks_and_reports_progress_in_order() {
        let sizes = Arc::new(Mutex::new(Vec::new()));
        let s = sizes.clone();
        let steps = Mutex::new(Vec::new());
        let run = purge_chunks((0..7u32).collect(), 3, none, move |b| { s.lock().unwrap().push(b.len()); Ok(()) }, &AtomicBool::new(false), |d, t| steps.lock().unwrap().push((d, t)));
        assert_eq!(*sizes.lock().unwrap(), vec![3, 3, 1]);
        assert_eq!(*steps.lock().unwrap(), vec![(0, 7), (3, 7), (6, 7), (7, 7)]);
        assert_eq!((run.attempted, run.end), (7, RunEnd::Finished));
    }

    #[test]
    fn an_empty_list_touches_nothing() {
        let run = purge_chunks(Vec::<u32>::new(), 3, none, |_| panic!("no debe llamarse"), &AtomicBool::new(false), |_, _| {});
        assert_eq!((run.attempted, run.end), (0, RunEnd::Finished));
    }

    #[test]
    fn a_failed_chunk_does_not_stop_the_rest_and_keeps_its_error() {
        let run = purge_chunks((0..6u32).collect(), 2, none, |b| if b[0] == 2 { Err("fallo".to_string()) } else { Ok(()) }, &AtomicBool::new(false), |_, _| {});
        assert_eq!((run.attempted, run.end, run.error.as_deref()), (6, RunEnd::Finished, Some("fallo")));
    }

    #[test]
    fn cancelling_before_a_chunk_leaves_the_rest_alone() {
        let cancel = Arc::new(AtomicBool::new(false));
        let c = cancel.clone();
        let calls = Arc::new(Mutex::new(0));
        let k = calls.clone();
        let run = purge_chunks((0..6u32).collect(), 2, none, move |_| { *k.lock().unwrap() += 1; c.store(true, Ordering::Relaxed); Ok(()) }, &cancel, |_, _| {});
        assert_eq!((run.attempted, run.end, *calls.lock().unwrap()), (2, RunEnd::Cancelled, 1));
    }

    #[test]
    fn cancelling_answers_even_if_windows_never_returns() {
        let cancel = Arc::new(AtomicBool::new(false));
        let c = cancel.clone();
        std::thread::spawn(move || { std::thread::sleep(Duration::from_millis(250)); c.store(true, Ordering::Relaxed); });
        let t = Instant::now();
        let run = purge_chunks(vec![1u32], 2, none, |_| { std::thread::sleep(Duration::from_secs(30)); Ok(()) }, &cancel, |_, _| {});
        assert!(t.elapsed() < Duration::from_secs(5), "{:?}", t.elapsed());
        assert_eq!((run.attempted, run.end), (0, RunEnd::Cancelled));
    }

    #[test]
    fn cancelling_waits_for_the_chunk_in_progress_when_it_ends_soon() {
        let cancel = Arc::new(AtomicBool::new(false));
        let c = cancel.clone();
        let run = purge_chunks(vec![1u32, 2, 3], 2, none, move |_| { c.store(true, Ordering::Relaxed); std::thread::sleep(Duration::from_millis(300)); Ok(()) }, &cancel, |_, _| {});
        assert_eq!((run.attempted, run.end), (2, RunEnd::Cancelled));
    }

    #[test]
    fn a_stuck_chunk_of_files_gives_up() {
        let run = purge_chunks(vec![1u32, 2], 2, |_| Some(Duration::from_millis(200)), |_| { std::thread::sleep(Duration::from_secs(30)); Ok(()) }, &AtomicBool::new(false), |_, _| {});
        assert_eq!((run.attempted, run.end), (2, RunEnd::TimedOut));
    }

    #[test]
    fn a_panic_in_the_worker_is_an_error_not_a_hang() {
        let run = purge_chunks(vec![1u32], 2, none, |_| panic!("prueba"), &AtomicBool::new(false), |_, _| {});
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
    fn a_chunk_cancelled_midway_counts_what_windows_did_finish() {
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
