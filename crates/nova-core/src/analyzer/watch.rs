//! Vigilancia de cambios de una ubicación analizada (`ReadDirectoryChangesW`). Solo anota qué cambió, con un límite fijo:
//! si se supera o el sistema pierde avisos, se marca desbordamiento y la siguiente actualización comprueba las fechas de las carpetas.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Carpetas distintas que se anotan antes de considerar que hay demasiados cambios.
pub const MAX_DIRTY: usize = 4096;
/// Archivos concretos que se recuerdan. Pasado el límite se conservan las carpetas: solo se pierde la lectura exacta de cada archivo.
pub const MAX_FILES: usize = 8192;

/// Lo que la vigilancia sabe que cambió, con rutas relativas y `/`.
#[derive(Default, Debug, Clone)]
pub struct Changes {
    pub dirs: Vec<String>,
    pub files: Vec<String>,
}

/// Qué hacer con lo anotado.
#[derive(Debug)]
pub enum Plan {
    /// Se perdieron avisos: hay que comparar las fechas de las carpetas.
    Lost,
    Exact(Changes),
}

#[derive(Default)]
pub struct Dirty {
    pub dirs: HashSet<String>,
    pub files: HashSet<String>,
    files_capped: bool,
    pub overflow: bool,
    /// Último aviso y primero sin atender.
    pub last: Option<Instant>,
    pub first: Option<Instant>,
    /// La vigilancia se detuvo sola (unidad retirada, carpeta borrada): hay que volver a iniciarla.
    pub broken: bool,
}

impl Dirty {
    /// Anota la carpeta que contiene `rel` (ruta relativa a la raíz vigilada).
    pub fn note(&mut self, rel: &str, now: Instant) {
        self.last = Some(now);
        self.first.get_or_insert(now);
        if self.overflow {
            return;
        }
        let parent = rel.rsplit_once(['\\', '/']).map_or("", |(p, _)| p);
        self.dirs.insert(parent.replace('\\', "/"));
        if self.dirs.len() > MAX_DIRTY {
            self.mark_overflow(now);
            return;
        }
        if !self.files_capped {
            self.files.insert(rel.replace('\\', "/"));
            if self.files.len() > MAX_FILES {
                self.files_capped = true;
                self.files = HashSet::new();
            }
        }
    }

    pub fn mark_overflow(&mut self, now: Instant) {
        self.overflow = true;
        self.dirs = HashSet::new();
        self.files = HashSet::new();
        self.last = Some(now);
        self.first.get_or_insert(now);
    }

    pub fn is_empty(&self) -> bool {
        self.dirs.is_empty() && !self.overflow
    }

    /// Entrega lo anotado cuando hay calma o cuando ya ha esperado demasiado: en una unidad con actividad constante, la calma no llega nunca.
    pub fn take(&mut self, now: Instant, quiet: Duration, max_wait: Duration) -> Option<Plan> {
        if self.is_empty() {
            return None;
        }
        let calm = self.last.map_or(true, |t| now.saturating_duration_since(t) >= quiet);
        let waited = self.first.is_some_and(|t| now.saturating_duration_since(t) >= max_wait);
        if !calm && !waited {
            return None;
        }
        let plan = if self.overflow {
            Plan::Lost
        } else {
            Plan::Exact(Changes { dirs: self.dirs.drain().collect(), files: self.files.drain().collect() })
        };
        self.overflow = false;
        self.files_capped = false;
        self.last = None;
        self.first = None;
        Some(plan)
    }
}

/// ¿Es ruido propio? Sin esto, guardar el informe en una unidad vigilada volvería a provocar otra actualización.
pub fn is_ignored(root: &Path, rel: &str, own_data: &Path) -> bool {
    let path = root.join(rel).to_string_lossy().to_lowercase();
    Path::new(&path).starts_with(own_data.to_string_lossy().to_lowercase())
}

#[cfg(windows)]
mod imp {
    use super::*;
    use crate::sys::win::wide;
    use std::ffi::c_void;
    use std::ptr::null_mut;
    use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateFileW(name: *const u16, access: u32, share: u32, sa: *mut c_void, disp: u32, flags: u32, tpl: isize) -> isize;
        fn ReadDirectoryChangesW(h: isize, buf: *mut c_void, len: u32, subtree: i32, filter: u32, ret: *mut u32, ov: *mut c_void, cb: *mut c_void) -> i32;
        fn CancelIoEx(h: isize, ov: *mut c_void) -> i32;
        fn CloseHandle(h: isize) -> i32;
    }
    const FILE_LIST_DIRECTORY: u32 = 1;
    const SHARE_ALL: u32 = 7;
    const OPEN_EXISTING: u32 = 3;
    const BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const INVALID: isize = -1;
    /// Nombre de archivo o carpeta, tamaño y fecha de escritura. No incluye último acceso, que no cambia el análisis.
    const FILTER: u32 = 0x1 | 0x2 | 0x8 | 0x10;
    const BUFFER: usize = 64 * 1024;

    pub struct Watcher {
        stop: Arc<AtomicBool>,
        handle: Arc<AtomicIsize>,
    }

    impl Watcher {
        /// Vigila `root` y sus subcarpetas. No sigue enlaces y no abre ningún archivo.
        pub fn start(root: &Path, own_data: PathBuf, dirty: Arc<Mutex<Dirty>>) -> Result<Watcher, String> {
            let w = wide(root.as_os_str());
            let h = unsafe { CreateFileW(w.as_ptr(), FILE_LIST_DIRECTORY, SHARE_ALL, null_mut(), OPEN_EXISTING, BACKUP_SEMANTICS, 0) };
            if h == INVALID {
                return Err("No se pudo vigilar esta ubicación.".into());
            }
            let stop = Arc::new(AtomicBool::new(false));
            let handle = Arc::new(AtomicIsize::new(h));
            let (s, root, held) = (stop.clone(), root.to_path_buf(), handle.clone());
            let spawned = std::thread::Builder::new().name("lfind-analyzer-watch".into()).spawn(move || {
                crate::sys::background_thread();
                let mut buf = vec![0u32; BUFFER / 4];
                while !s.load(Ordering::Relaxed) {
                    let mut got = 0u32;
                    let ok = unsafe { ReadDirectoryChangesW(h, buf.as_mut_ptr() as *mut c_void, BUFFER as u32, 1, FILTER, &mut got, null_mut(), null_mut()) };
                    if s.load(Ordering::Relaxed) {
                        break;
                    }
                    let now = Instant::now();
                    let mut d = dirty.lock().unwrap_or_else(|e| e.into_inner());
                    if ok == 0 {
                        d.broken = true;
                        break;
                    }
                    if got == 0 {
                        d.mark_overflow(now);
                        continue;
                    }
                    let bytes = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u8, got as usize) };
                    let mut at = 0usize;
                    while at + 12 <= bytes.len() {
                        let next = u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]) as usize;
                        let len = u32::from_le_bytes([bytes[at + 8], bytes[at + 9], bytes[at + 10], bytes[at + 11]]) as usize;
                        if at + 12 + len > bytes.len() {
                            break;
                        }
                        let units: Vec<u16> = bytes[at + 12..at + 12 + len].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                        let rel = String::from_utf16_lossy(&units);
                        if !is_ignored(&root, &rel, &own_data) {
                            d.note(&rel, now);
                        }
                        if next == 0 {
                            break;
                        }
                        at += next;
                    }
                }
                if held.swap(INVALID, Ordering::SeqCst) != INVALID {
                    unsafe { CloseHandle(h) };
                }
            });
            if let Err(e) = spawned {
                unsafe { CloseHandle(h) };
                return Err(format!("No se pudo iniciar la vigilancia: {e}"));
            }
            Ok(Watcher { stop, handle })
        }
    }

    impl Drop for Watcher {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            let h = self.handle.load(Ordering::SeqCst);
            if h != INVALID {
                unsafe { CancelIoEx(h, null_mut()) };
            }
        }
    }
}

#[cfg(windows)]
pub use imp::Watcher;

#[cfg(not(windows))]
pub struct Watcher;

#[cfg(not(windows))]
impl Watcher {
    pub fn start(_root: &Path, _own_data: PathBuf, _dirty: Arc<Mutex<Dirty>>) -> Result<Watcher, String> {
        Err("La vigilancia de cambios solo está disponible en Windows.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_the_parent_folder_only_once() {
        let mut d = Dirty::default();
        let t = Instant::now();
        d.note("a\\b\\x.txt", t);
        d.note("a\\b\\y.txt", t);
        d.note("raiz.txt", t);
        assert_eq!(d.dirs.len(), 2);
        assert!(d.dirs.contains("a/b") && d.dirs.contains(""));
    }

    #[test]
    fn too_many_folders_become_an_overflow_with_bounded_memory() {
        let mut d = Dirty::default();
        let t = Instant::now();
        for i in 0..=MAX_DIRTY + 5 {
            d.note(&format!("d{i}\\f"), t);
        }
        assert!(d.overflow && d.dirs.is_empty() && !d.is_empty());
    }

    #[test]
    fn its_own_reports_are_not_changes() {
        let root = Path::new("C:\\");
        let own = Path::new("C:\\Users\\A\\AppData\\Local\\app\\analyzer");
        assert!(is_ignored(root, "Users\\A\\AppData\\Local\\app\\analyzer\\x.db", own));
        assert!(!is_ignored(root, "Users\\A\\Documentos\\x.db", own));
    }

    #[cfg(windows)]
    #[test]
    fn its_own_data_is_ignored_whatever_the_case() {
        let own = Path::new("C:\\Users\\A\\AppData\\Local\\App");
        assert!(is_ignored(Path::new("c:\\"), "users\\a\\appdata\\local\\APP\\index.db-wal", own));
        assert!(!is_ignored(Path::new("c:\\"), "users\\a\\appdata\\local\\App2\\x", own));
    }

    const QUIET: Duration = Duration::from_secs(60);
    const MAX_WAIT: Duration = Duration::from_secs(300);

    #[test]
    fn waits_for_calm_before_handing_over_the_changes() {
        let mut d = Dirty::default();
        let t = Instant::now();
        d.note("a\\x.txt", t);
        assert!(d.take(t + Duration::from_secs(30), QUIET, MAX_WAIT).is_none());
        match d.take(t + QUIET, QUIET, MAX_WAIT) {
            Some(Plan::Exact(c)) => assert_eq!((c.dirs, c.files), (vec!["a".to_string()], vec!["a/x.txt".to_string()])),
            other => panic!("{other:?}"),
        }
        assert!(d.is_empty() && d.first.is_none() && d.take(t + QUIET * 9, QUIET, MAX_WAIT).is_none());
    }

    #[test]
    fn constant_activity_does_not_postpone_the_update_forever() {
        let mut d = Dirty::default();
        let t = Instant::now();
        for s in (0..300).step_by(10) {
            let now = t + Duration::from_secs(s);
            d.note("a\\ruido.log", now);
            assert!(d.take(now, QUIET, MAX_WAIT).is_none(), "antes de la espera máxima, segundo {s}");
        }
        let now = t + MAX_WAIT;
        d.note("a\\ruido.log", now);
        assert!(matches!(d.take(now, QUIET, MAX_WAIT), Some(Plan::Exact(_))));
    }

    #[test]
    fn lost_notices_ask_for_a_date_comparison_and_start_clean() {
        let mut d = Dirty::default();
        let t = Instant::now();
        d.note("a\\x", t);
        d.mark_overflow(t);
        assert!(matches!(d.take(t + QUIET, QUIET, MAX_WAIT), Some(Plan::Lost)));
        assert!(!d.overflow && d.is_empty());
    }

    #[test]
    fn too_many_files_keep_their_folders_with_bounded_memory() {
        let mut d = Dirty::default();
        let t = Instant::now();
        for i in 0..=MAX_FILES + 5 {
            d.note(&format!("a\\{i}.txt"), t);
        }
        assert!(d.files.is_empty() && !d.overflow && d.dirs.contains("a"));
        match d.take(t + QUIET, QUIET, MAX_WAIT) {
            Some(Plan::Exact(c)) => assert!(c.files.is_empty() && c.dirs == vec!["a".to_string()]),
            other => panic!("{other:?}"),
        }
        d.note("a\\b.txt", t);
        assert!(d.files.contains("a/b.txt"), "tras entregar, vuelve a recordar archivos");
    }

    #[cfg(windows)]
    fn wait_until(mut ok: impl FnMut() -> bool) -> bool {
        let end = Instant::now() + Duration::from_secs(5);
        while !ok() && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(30));
        }
        ok()
    }

    #[cfg(windows)]
    #[test]
    fn a_size_only_change_is_noticed_with_its_file() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("lfind-watch-size-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub").join("log.bin"), b"x").unwrap();
        let dirty = Arc::new(Mutex::new(Dirty::default()));
        let w = Watcher::start(&dir, PathBuf::from("Z:\\nada"), dirty.clone()).unwrap();
        std::thread::sleep(Duration::from_millis(150));
        let mut f = std::fs::OpenOptions::new().append(true).open(dir.join("sub").join("log.bin")).unwrap();
        f.write_all(&[7u8; 8192]).unwrap();
        f.sync_all().unwrap();
        assert!(wait_until(|| dirty.lock().unwrap().files.contains("sub/log.bin")));
        assert!(dirty.lock().unwrap().dirs.contains("sub"));
        drop(f);
        drop(w);
        std::thread::sleep(Duration::from_millis(150));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn deleting_the_watched_folder_is_reported() {
        let dir = std::env::temp_dir().join(format!("lfind-watch-gone-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dirty = Arc::new(Mutex::new(Dirty::default()));
        let w = Watcher::start(&dir, PathBuf::from("Z:\\nada"), dirty.clone()).unwrap();
        std::thread::sleep(Duration::from_millis(150));
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(wait_until(|| dirty.lock().unwrap().broken));
        drop(w);
    }

    #[cfg(windows)]
    #[test]
    fn real_changes_are_reported_and_stopping_leaves_nothing_behind() {
        let dir = std::env::temp_dir().join(format!("lfind-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let dirty = Arc::new(Mutex::new(Dirty::default()));
        let w = Watcher::start(&dir, PathBuf::from("Z:\\nada"), dirty.clone()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(150));
        std::fs::write(dir.join("sub").join("n.txt"), b"x").unwrap();
        let end = Instant::now() + std::time::Duration::from_secs(5);
        while dirty.lock().unwrap().dirs.is_empty() && Instant::now() < end {
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        assert!(dirty.lock().unwrap().dirs.contains("sub"));
        drop(w);
        std::thread::sleep(std::time::Duration::from_millis(150));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
