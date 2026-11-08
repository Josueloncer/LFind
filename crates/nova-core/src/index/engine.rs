//! `Indexer`: orquesta el indexado en segundo plano, su estado y las búsquedas.
//!
//! Incremental (fase 1): reconciliación por generaciones. Cada pasada completa de una unidad marca lo que ve con una generación
//! nueva y barra lo demás; así se detectan altas, bajas, renombrados y movimientos aunque no se haya observado ningún evento.
//! La fase 2 añade el observador (`ReadDirectoryChangesW`) sobre las mismas operaciones del almacén (`upsert`, `remove_tree`).

use super::exclude::ExcludeRules;
use super::model::*;
use super::scanner::{scan_root, ScanCtl, BATCH};
use super::store::IndexStore;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug)]
pub struct StartOpts {
    /// Reindexar aunque la unidad figure como completa y reciente.
    pub force: bool,
    /// Una unidad completa se vuelve a reconciliar si su último indexado es más antiguo que esto (0 = nunca por antigüedad).
    pub reconcile_ms: i64,
    pub batch: usize,
    /// Pausa breve tras cada lote para ceder CPU y disco al resto de la aplicación.
    pub yield_ms: u64,
}

impl Default for StartOpts {
    fn default() -> Self {
        StartOpts { force: false, reconcile_ms: 6 * 3_600_000, batch: BATCH, yield_ms: 2 }
    }
}

struct Live {
    phase: Phase,
    current_root: Option<String>,
    scanned: u64,
    expected: Option<u64>,
    last_update: Option<i64>,
}

struct Shared {
    ctl: ScanCtl,
    live: Mutex<Live>,
    worker: Mutex<Option<JoinHandle<()>>>,
    running: AtomicBool,
}

#[derive(Clone)]
pub struct Indexer {
    store: Arc<dyn IndexStore>,
    sh: Arc<Shared>,
}

pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Raíces de unidades fijas conectadas (solo Windows; en otros sistemas la lista se da explícitamente).
pub fn detect_fixed_drives() -> Vec<String> {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetLogicalDrives() -> u32;
            fn GetDriveTypeW(root: *const u16) -> u32;
        }
        let mask = unsafe { GetLogicalDrives() };
        (0..26u8)
            .filter(|i| mask & (1 << i) != 0)
            .filter_map(|i| {
                let letter = (b'A' + i) as char;
                let root: Vec<u16> = format!("{letter}:\\").encode_utf16().chain(Some(0)).collect();
                (unsafe { GetDriveTypeW(root.as_ptr()) } == 3).then(|| format!("{letter}:\\"))
            })
            .collect()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// Prioridad de fondo para el hilo actual (también reduce la prioridad de E/S). No hace nada fuera de Windows.
fn background_priority() {
    #[cfg(windows)]
    {
        use std::ffi::c_void;
        #[link(name = "kernel32")]
        extern "system" {
            fn GetCurrentThread() -> *mut c_void;
            fn SetThreadPriority(h: *mut c_void, p: i32) -> i32;
        }
        const THREAD_MODE_BACKGROUND_BEGIN: i32 = 0x0001_0000;
        unsafe { SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN) };
    }
}

impl Indexer {
    pub fn new(store: Arc<dyn IndexStore>) -> Self {
        Indexer {
            store,
            sh: Arc::new(Shared {
                ctl: ScanCtl::default(),
                live: Mutex::new(Live { phase: Phase::Idle, current_root: None, scanned: 0, expected: None, last_update: None }),
                worker: Mutex::new(None),
                running: AtomicBool::new(false),
            }),
        }
    }

    pub fn store(&self) -> &Arc<dyn IndexStore> {
        &self.store
    }

    pub fn is_running(&self) -> bool {
        self.sh.running.load(Ordering::SeqCst)
    }

    /// Lanza el indexado en un hilo propio y vuelve enseguida. `Ok(false)` si ya hay uno en marcha.
    /// Las raíces que ya no están en `roots` (o no están conectadas) dejan de buscarse, pero su índice se conserva.
    pub fn start(&self, roots: Vec<String>, rules: ExcludeRules, opts: StartOpts) -> StoreResult<bool> {
        if self.sh.running.swap(true, Ordering::SeqCst) {
            return Ok(false);
        }
        self.sh.ctl.cancel.store(false, Ordering::SeqCst);
        self.sh.ctl.pause.store(false, Ordering::SeqCst);
        {
            let mut l = self.sh.live.lock().unwrap();
            l.phase = Phase::Scanning;
            l.scanned = 0;
            l.expected = None;
            l.current_root = None;
        }
        let me = self.clone();
        let h = std::thread::Builder::new().name("lfind-index".into()).spawn(move || {
            background_priority();
            me.run(roots, rules, opts);
            let mut l = me.sh.live.lock().unwrap();
            l.phase = Phase::Idle;
            l.current_root = None;
            l.last_update = Some(now_ms());
            drop(l);
            me.sh.running.store(false, Ordering::SeqCst);
        });
        match h {
            Ok(h) => {
                *self.sh.worker.lock().unwrap() = Some(h);
                Ok(true)
            }
            Err(e) => {
                self.sh.running.store(false, Ordering::SeqCst);
                self.sh.live.lock().unwrap().phase = Phase::Idle;
                Err(format!("No se pudo iniciar el indexado: {e}"))
            }
        }
    }

    fn run(&self, roots: Vec<String>, rules: ExcludeRules, opts: StartOpts) {
        let ctl = &self.sh.ctl;
        // Lo que ya no se pide indexar se oculta de las búsquedas (el índice se conserva por si vuelve).
        if let Ok(all) = self.store.roots() {
            let wanted: Vec<String> = roots.iter().map(|r| super::classify::path_key(r)).collect();
            for r in all {
                if !wanted.contains(&super::classify::path_key(&r.path)) && r.online {
                    let _ = self.store.set_online(r.id, false);
                }
            }
        }
        for path in roots {
            if ctl.cancel.load(Ordering::SeqCst) {
                break;
            }
            let id = match self.store.ensure_root(&path) {
                Ok(i) => i,
                Err(_) => continue,
            };
            let online = Path::new(&path).is_dir();
            let _ = self.store.set_online(id, online);
            if !online {
                continue;
            }
            let rec = match self.store.roots().ok().and_then(|v| v.into_iter().find(|r| r.id == id)) {
                Some(r) => r,
                None => continue,
            };
            let stale = opts.reconcile_ms > 0 && rec.last_complete.map_or(true, |t| now_ms() - t >= opts.reconcile_ms);
            if !(opts.force || rec.state != RootState::Complete || stale) {
                continue;
            }
            {
                let mut l = self.sh.live.lock().unwrap();
                l.current_root = Some(path.clone());
                l.scanned = 0;
                l.expected = if rec.files + rec.dirs > 0 { Some(rec.files + rec.dirs) } else { None };
            }
            let gen = match self.store.begin_scan(id) {
                Ok(g) => g,
                Err(_) => continue,
            };
            let sh = self.sh.clone();
            let mut tick = |t: super::scanner::ScanTick| {
                let mut l = sh.live.lock().unwrap();
                l.scanned = t.scanned;
                l.last_update = Some(now_ms());
            };
            match scan_root(self.store.as_ref(), id, Path::new(&path), gen, &rules, ctl, opts.batch, opts.yield_ms, &mut tick) {
                Ok(o) if !o.cancelled => {
                    let _ = self.store.finish_scan(id, gen, o.skipped, now_ms());
                }
                Ok(_) => {
                    let _ = self.store.interrupt_scan(id);
                }
                Err(e) => {
                    let _ = self.store.log_issue(id, &path, &e);
                    let _ = self.store.interrupt_scan(id);
                }
            }
        }
    }

    pub fn pause(&self) {
        if self.is_running() {
            self.sh.ctl.pause.store(true, Ordering::SeqCst);
            self.sh.live.lock().unwrap().phase = Phase::Paused;
        }
    }

    pub fn resume(&self) {
        self.sh.ctl.pause.store(false, Ordering::SeqCst);
        let mut l = self.sh.live.lock().unwrap();
        if l.phase == Phase::Paused {
            l.phase = Phase::Scanning;
        }
    }

    /// Detiene el indexado en curso y espera a que el hilo termine. La raíz queda `Interrupted` (se retoma al volver a iniciar).
    pub fn stop(&self) {
        self.sh.ctl.cancel.store(true, Ordering::SeqCst);
        self.sh.ctl.pause.store(false, Ordering::SeqCst);
        let h = self.sh.worker.lock().unwrap().take();
        if let Some(h) = h {
            let _ = h.join();
        }
        self.sh.ctl.cancel.store(false, Ordering::SeqCst);
    }

    /// Borra el índice y lo construye de nuevo.
    pub fn rebuild(&self, roots: Vec<String>, rules: ExcludeRules, mut opts: StartOpts) -> StoreResult<bool> {
        self.stop();
        self.store.clear_all()?;
        opts.force = true;
        self.start(roots, rules, opts)
    }

    pub fn status(&self) -> StoreResult<IndexStatus> {
        let (phase, current_root, scanned, expected, last_update) = {
            let l = self.sh.live.lock().unwrap();
            (l.phase, l.current_root.clone(), l.scanned, l.expected, l.last_update)
        };
        let roots = self.store.roots()?;
        let (total_files, total_dirs) = self.store.counts()?;
        let skipped = roots.iter().filter(|r| r.online).map(|r| r.skipped).sum();
        let progress = match (phase, expected) {
            (Phase::Idle, _) => None,
            (_, Some(e)) if e > 0 => Some(((scanned as f64 / e as f64).min(0.99)) as f32),
            _ => None,
        };
        let last = roots.iter().filter_map(|r| r.last_complete).max().max(last_update);
        Ok(IndexStatus { phase, current_root, scanned, progress, roots, total_files, total_dirs, skipped, issues: self.store.issues(50)?, last_update: last })
    }

    /// Busca en el índice. `q.drives` se traduce a las raíces conectadas que coinciden.
    pub fn search(&self, q: &Query, cancel: &Arc<AtomicBool>) -> StoreResult<QueryPage> {
        let roots = self.store.roots()?;
        let wanted: Vec<String> = q.drives.iter().map(|d| super::classify::path_key(d)).collect();
        let ids: Vec<i64> = roots
            .iter()
            .filter(|r| r.online && (wanted.is_empty() || wanted.contains(&super::classify::path_key(&r.path))))
            .map(|r| r.id)
            .collect();
        let mut q = q.clone();
        q.root_ids = Some(ids);
        self.store.query(&q, cancel)
    }
}
