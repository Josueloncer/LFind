//! Detector de archivos duplicados. Solo trabaja cuando se le pide: no hay hilos ni tareas fuera de un análisis.
//!
//! 1. Se enumeran las carpetas y se guardan los archivos en una base temporal en disco: millones de rutas no pasan por la memoria.
//! 2. Solo los tamaños repetidos siguen. De ellos se resume el principio y el final.
//! 3. Solo lo que coincide se lee entero, por bloques, con SHA-256. Dos archivos son duplicados únicamente si coincide ese hash.
//!
//! Tiene su propia base porque el índice y los informes del analizador tienen dueño y ritmo propios: no deben esperar a este trabajo.

use crate::dir::is_cloud_only;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const PAGE: i64 = 1000;
const COMMIT_EVERY: u32 = 20_000;
const READ_BUF: usize = 128 * 1024;
const EDGE: u64 = 16 * 1024;
/// Hasta aquí, resumir el principio y el final ya es leerlo todo.
const WHOLE_MAX: u64 = EDGE * 2;
/// Una fecha muy reciente no garantiza que el archivo haya dejado de cambiar: ese hash no se guarda.
const CACHE_MIN_AGE: Duration = Duration::from_secs(2);
const CACHE_KEEP_DAYS: i64 = 180;
const EMIT_EVERY: Duration = Duration::from_millis(250);
const MAX_WORKERS: usize = 4;

pub const FLAG_HIDDEN: u32 = 1;
pub const FLAG_READONLY: u32 = 2;
pub const FLAG_HARDLINK: u32 = 8;
pub const FLAG_SPECIAL: u32 = 16;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub roots: Vec<String>,
    #[serde(default)]
    pub exclusions: Vec<String>,
    #[serde(default = "one")]
    pub min_size: u64,
    /// Lectura máxima en MB/s. 0: sin límite.
    #[serde(default)]
    pub max_mbps: u32,
}

fn one() -> u64 {
    1
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub phase: &'static str,
    pub paused: bool,
    pub files: u64,
    pub candidates: u64,
    pub candidate_bytes: u64,
    pub read_bytes: u64,
    pub reused: u64,
    pub skipped_cloud: u64,
    pub skipped_protected: u64,
    pub links: u64,
    pub issues: u64,
    pub current: String,
    /// Etapa de comparación (1 resumen, 2 lectura completa, 3 verificación); 0 fuera de ellas.
    pub step: u8,
    /// Archivos en las etapas 1 y 3; bytes en la 2.
    pub step_done: u64,
    pub step_total: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub finished_at: i64,
    pub duration_ms: u64,
    pub roots: Vec<String>,
    pub files: u64,
    pub groups: u64,
    pub duplicate_files: u64,
    pub reclaimable: u64,
    pub read_bytes: u64,
    pub reused: u64,
    pub skipped_cloud: u64,
    pub skipped_protected: u64,
    pub links: u64,
    pub issues: u64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Event {
    Progress(Progress),
    Done { outcome: &'static str, message: Option<String>, summary: Option<Summary> },
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    pub path: String,
    pub modified: i64,
    pub flags: u32,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: i64,
    pub size: u64,
    pub hash: String,
    pub reclaimable: u64,
    pub members: Vec<Member>,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct RemovalCheck {
    pub allowed: Vec<String>,
    pub blocked: Vec<(String, String)>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Comparison {
    pub identical: bool,
    pub first_difference: Option<u64>,
}

pub type OnEvent = Box<dyn Fn(Event) + Send>;

fn e2s<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

fn clean(p: &str) -> String {
    let t = p.trim().replace('/', "\\");
    if t.len() == 2 && t.ends_with(':') {
        return format!("{t}\\");
    }
    if t.len() > 3 { t.trim_end_matches('\\').to_string() } else { t }
}

fn key_of(p: &str) -> String {
    clean(p).to_lowercase()
}

/// Sin repetidos ni carpetas contenidas en otra: si no, un mismo archivo aparecería dos veces y parecería duplicado de sí mismo.
pub fn normalize_roots(roots: &[String]) -> Vec<String> {
    let mut seen: Vec<(String, String)> = roots.iter().map(|r| (key_of(r), clean(r))).filter(|(k, _)| !k.is_empty()).collect();
    seen.sort_by_key(|(k, _)| k.len());
    let mut out: Vec<(String, String)> = vec![];
    for (k, r) in seen {
        let inside = out.iter().any(|(o, _)| k == *o || k.starts_with(&format!("{}\\", o.trim_end_matches('\\'))));
        if !inside {
            out.push((k, r));
        }
    }
    out.into_iter().map(|(_, r)| r).collect()
}

/// Lo que se excluye por defecto: sistema, programas y papelera. Una entrada sin barras excluye ese nombre en cualquier lugar.
pub fn default_exclusions() -> Vec<String> {
    let mut v: Vec<String> = ["$Recycle.Bin", "System Volume Information", "$WinREAgent", "$SysReset", "Config.Msi", "Recovery", "WindowsApps"].map(String::from).to_vec();
    for var in ["SystemRoot", "ProgramFiles", "ProgramFiles(x86)", "ProgramW6432", "ProgramData"] {
        if let Ok(p) = std::env::var(var) {
            if !v.iter().any(|x| x.eq_ignore_ascii_case(&p)) {
                v.push(p);
            }
        }
    }
    v
}

struct Exclusions {
    names: HashSet<String>,
    prefixes: Vec<String>,
}

impl Exclusions {
    fn new(list: &[String], own: &Path) -> Self {
        let mut e = Exclusions { names: HashSet::new(), prefixes: vec![key_of(&own.to_string_lossy())] };
        for x in list {
            let k = key_of(x);
            if k.is_empty() {
                continue;
            }
            if k.contains('\\') {
                e.prefixes.push(k);
            } else {
                e.names.insert(k);
            }
        }
        e
    }

    fn excludes(&self, dir: &Path) -> bool {
        let k = key_of(&dir.to_string_lossy());
        if self.prefixes.iter().any(|p| k == *p || k.starts_with(&format!("{}\\", p.trim_end_matches('\\')))) {
            return true;
        }
        !self.names.is_empty() && dir.file_name().is_some_and(|n| self.names.contains(&n.to_string_lossy().to_lowercase()))
    }
}

fn is_special(path: &str) -> bool {
    let k = path.to_lowercase();
    ["\\appdata\\", "\\windows\\", "\\program files", "\\programdata\\"].iter().any(|s| k.contains(s))
}

#[derive(Default)]
struct Ctl {
    cancel: AtomicBool,
    pause: AtomicBool,
    /// Al parar, tirar lo hecho. Sin esto, parar es «suspender»: se puede reanudar.
    discard: AtomicBool,
}

impl Ctl {
    /// `false` si hay que abandonar.
    fn go_on(&self) -> bool {
        while self.pause.load(Ordering::Relaxed) && !self.cancel.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(100));
        }
        !self.cancel.load(Ordering::Relaxed)
    }
}

struct Throttle {
    limit: u64,
    since: Instant,
    bytes: u64,
}

impl Throttle {
    fn new(mbps: u32) -> Self {
        Throttle { limit: mbps as u64 * 1024 * 1024, since: Instant::now(), bytes: 0 }
    }
    fn spent(&mut self, n: usize) {
        if self.limit == 0 {
            return;
        }
        self.bytes += n as u64;
        let due = Duration::from_secs_f64(self.bytes as f64 / self.limit as f64);
        if let Some(wait) = due.checked_sub(self.since.elapsed()) {
            std::thread::sleep(wait.min(Duration::from_millis(500)));
        }
        if self.since.elapsed() > Duration::from_secs(1) {
            self.since = Instant::now();
            self.bytes = 0;
        }
    }
}

#[cfg(windows)]
fn stamp(m: &fs::Metadata) -> i64 {
    use std::os::windows::fs::MetadataExt;
    m.last_write_time() as i64
}
#[cfg(not(windows))]
fn stamp(m: &fs::Metadata) -> i64 {
    m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos() as i64)
}

#[cfg(windows)]
fn attrs(m: &fs::Metadata) -> u32 {
    use std::os::windows::fs::MetadataExt;
    m.file_attributes()
}
#[cfg(not(windows))]
fn attrs(_m: &fs::Metadata) -> u32 {
    0
}

/// Lo que se sabe de un archivo sin abrirlo.
struct QuickStat {
    size: u64,
    stamp: i64,
    attrs: u32,
}

#[cfg(windows)]
fn quick_stat(p: &Path) -> Option<QuickStat> {
    #[repr(C)]
    struct Data {
        attrs: u32,
        created: [u32; 2],
        accessed: [u32; 2],
        written: [u32; 2],
        size_hi: u32,
        size_lo: u32,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetFileAttributesExW(name: *const u16, level: u32, out: *mut Data) -> i32;
    }
    let wide: Vec<u16> = p.to_string_lossy().encode_utf16().chain(Some(0)).collect();
    let mut d: Data = unsafe { std::mem::zeroed() };
    if unsafe { GetFileAttributesExW(wide.as_ptr(), 0, &mut d) } != 0 {
        return Some(QuickStat { size: ((d.size_hi as u64) << 32) | d.size_lo as u64, stamp: (((d.written[1] as u64) << 32) | d.written[0] as u64) as i64, attrs: d.attrs });
    }
    // Rutas muy largas: la vía normal las convierte.
    fs::metadata(p).ok().map(|m| QuickStat { size: m.len(), stamp: stamp(&m), attrs: attrs(&m) })
}
#[cfg(not(windows))]
fn quick_stat(p: &Path) -> Option<QuickStat> {
    fs::metadata(p).ok().map(|m| QuickStat { size: m.len(), stamp: stamp(&m), attrs: attrs(&m) })
}

/// Identidad real de un archivo abierto: comprueba que no ha cambiado y reconoce enlaces duros.
struct Identity {
    size: u64,
    stamp: i64,
    links: u32,
    key: String,
}

#[cfg(windows)]
fn identity_of(f: &File) -> Option<Identity> {
    use std::os::windows::io::AsRawHandle;
    #[repr(C)]
    struct Info {
        attrs: u32,
        times: [u32; 6],
        volume: u32,
        size_hi: u32,
        size_lo: u32,
        links: u32,
        index_hi: u32,
        index_lo: u32,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetFileInformationByHandle(h: *mut std::ffi::c_void, out: *mut Info) -> i32;
    }
    let mut i: Info = unsafe { std::mem::zeroed() };
    (unsafe { GetFileInformationByHandle(f.as_raw_handle() as *mut _, &mut i) } != 0).then(|| Identity {
        size: ((i.size_hi as u64) << 32) | i.size_lo as u64,
        stamp: (((i.times[5] as u64) << 32) | i.times[4] as u64) as i64,
        links: i.links,
        key: format!("{:x}:{:x}{:08x}", i.volume, i.index_hi, i.index_lo),
    })
}
#[cfg(not(windows))]
fn identity_of(f: &File) -> Option<Identity> {
    use std::os::unix::fs::MetadataExt;
    let m = f.metadata().ok()?;
    Some(Identity { size: m.len(), stamp: stamp(&m), links: m.nlink() as u32, key: format!("{:x}:{:x}", m.dev(), m.ino()) })
}

/// Clave de enlace duro; vacía si el archivo solo tiene un nombre.
fn link_key(i: Identity) -> String {
    if i.links > 1 { i.key } else { String::new() }
}

/// ¿Se escribió hace tan poco que podría seguir cambiando?
fn too_recent(stamp: i64) -> bool {
    #[cfg(windows)]
    let now = (SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos()) / 100) as i64 + 116_444_736_000_000_000;
    #[cfg(windows)]
    let age = Duration::from_nanos((now.saturating_sub(stamp).max(0) as u64).saturating_mul(100));
    #[cfg(not(windows))]
    let age = Duration::from_nanos((SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos()) as i64).saturating_sub(stamp).max(0) as u64);
    age < CACHE_MIN_AGE
}

/// Cuántos hilos leen a la vez. En un disco mecánico, varios a la vez solo añaden saltos del cabezal.
#[cfg(windows)]
fn has_seek_penalty(root: &str) -> Option<bool> {
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    #[allow(clashing_extern_declarations)]
    extern "system" {
        fn CreateFileW(name: *const u16, access: u32, share: u32, sa: *mut c_void, disp: u32, flags: u32, tpl: *mut c_void) -> *mut c_void;
        fn DeviceIoControl(h: *mut c_void, code: u32, input: *const u32, n: u32, out: *mut u32, m: u32, got: *mut u32, ov: *mut c_void) -> i32;
        fn CloseHandle(h: *mut c_void) -> i32;
    }
    const IOCTL_STORAGE_QUERY_PROPERTY: u32 = 0x2D_1400;
    const STORAGE_DEVICE_SEEK_PENALTY: u32 = 7;
    let letter = root.chars().next().filter(|c| c.is_ascii_alphabetic() && root[1..].starts_with(':'))?;
    let name: Vec<u16> = format!("\\\\.\\{letter}:").encode_utf16().chain(Some(0)).collect();
    unsafe {
        let h = CreateFileW(name.as_ptr(), 0, 3, std::ptr::null_mut(), 3, 0, std::ptr::null_mut());
        if h as isize == -1 {
            return None;
        }
        let query = [STORAGE_DEVICE_SEEK_PENALTY, 0, 0];
        let mut out = [0u32; 3];
        let mut got = 0u32;
        let ok = DeviceIoControl(h, IOCTL_STORAGE_QUERY_PROPERTY, query.as_ptr(), 12, out.as_mut_ptr(), 12, &mut got, std::ptr::null_mut()) != 0;
        CloseHandle(h);
        (ok && got >= 9).then_some(out[2] & 0xFF != 0)
    }
}
#[cfg(not(windows))]
fn has_seek_penalty(_root: &str) -> Option<bool> {
    None
}

fn worker_count(roots: &[String]) -> usize {
    let cores = std::thread::available_parallelism().map_or(2, |n| n.get());
    let max = (cores / 2).clamp(1, MAX_WORKERS);
    // Si algún origen es mecánico o no se puede saber (red), un solo lector.
    if roots.iter().all(|r| has_seek_penalty(r) == Some(false)) { max } else { 1 }
}

enum Hashed {
    /// El hash y la clave de enlace, que se obtiene gratis con el archivo ya abierto.
    Value(Vec<u8>, Option<String>),
    Cancelled,
    /// No se pudo leer, o cambió mientras se leía.
    Unusable,
}

struct Reader<'a> {
    ctl: &'a Ctl,
    throttle: Option<Mutex<Throttle>>,
    read: AtomicU64,
}

impl Reader<'_> {
    fn feed(&self, file: &mut File, hasher: &mut Sha256, buf: &mut [u8], mut left: u64) -> Result<(), bool> {
        while left > 0 {
            if !self.ctl.go_on() {
                return Err(true);
            }
            let want = (buf.len() as u64).min(left) as usize;
            let n = file.read(&mut buf[..want]).map_err(|_| false)?;
            if n == 0 {
                return Err(false);
            }
            hasher.update(&buf[..n]);
            left -= n as u64;
            self.read.fetch_add(n as u64, Ordering::Relaxed);
            if let Some(t) = &self.throttle {
                t.lock().unwrap().spent(n);
            }
        }
        Ok(())
    }

    fn hash(&self, path: &Path, size: u64, when: i64, whole: bool, buf: &mut [u8]) -> Hashed {
        // Antes de abrir: un archivo que ya solo está en la nube no se toca.
        let unchanged = |s: QuickStat| s.size == size && s.stamp == when && !crate::dir::attrs_cloud_only(s.attrs);
        if !quick_stat(path).is_some_and(unchanged) {
            return Hashed::Unusable;
        }
        let Ok(mut file) = File::open(path) else { return Hashed::Unusable };
        let mut hasher = Sha256::new();
        let result = if whole || size <= WHOLE_MAX {
            self.feed(&mut file, &mut hasher, buf, size)
        } else {
            self.feed(&mut file, &mut hasher, buf, EDGE).and_then(|()| {
                file.seek(SeekFrom::End(-(EDGE as i64))).map_err(|_| false)?;
                self.feed(&mut file, &mut hasher, buf, EDGE)
            })
        };
        match result {
            Err(true) => Hashed::Cancelled,
            Err(false) => Hashed::Unusable,
            // Si cambió durante la lectura, el resultado no describe a ningún contenido real.
            Ok(()) => match identity_of(&file).filter(|i| i.size == size && i.stamp == when) {
                Some(i) => Hashed::Value(hasher.finalize().to_vec(), Some(link_key(i))),
                None => Hashed::Unusable,
            },
        }
    }
}

const STORE: &str = "CREATE TABLE IF NOT EXISTS hash_cache(path TEXT PRIMARY KEY, size INTEGER NOT NULL, stamp INTEGER NOT NULL, quick BLOB NOT NULL, full BLOB, day INTEGER NOT NULL) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT NOT NULL) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS dup_groups(id INTEGER PRIMARY KEY, size INTEGER NOT NULL, hash BLOB NOT NULL, reclaimable INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS dup_files(grp INTEGER NOT NULL, path TEXT NOT NULL, modified INTEGER NOT NULL, flags INTEGER NOT NULL);
CREATE INDEX IF NOT EXISTS dup_files_grp ON dup_files(grp);
CREATE INDEX IF NOT EXISTS dup_files_path ON dup_files(path);
CREATE INDEX IF NOT EXISTS dup_groups_reclaim ON dup_groups(reclaimable DESC);";

/// Con WAL, cortar la luz deja el trabajo en la última página confirmada, sin dañar la base y sin un `fsync` por página.
const DURABLE: &str = "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA wal_autocheckpoint=4000;";

const JOB: &str = "PRAGMA cache_size=-8000; PRAGMA temp_store=FILE;
CREATE TABLE IF NOT EXISTS job(key TEXT PRIMARY KEY, value TEXT NOT NULL) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS pending(dir TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS scan(id INTEGER PRIMARY KEY, size INTEGER NOT NULL, path TEXT NOT NULL, stamp INTEGER NOT NULL, modified INTEGER NOT NULL, flags INTEGER NOT NULL, state INTEGER NOT NULL DEFAULT 0, quick BLOB, full BLOB, link_key TEXT);";

const NEEDS_QUICK: i64 = 1;
const QUICK_DONE: i64 = 2;
const NEEDS_FULL: i64 = 3;
const NEEDS_CHECK: i64 = 4;
const CHECKED: i64 = 5;
const UNUSABLE: i64 = 9;

const STAGE_SCAN: i64 = 0;
const STAGE_GROUP: i64 = 1;
const STAGE_QUICK: i64 = 2;
const STAGE_FULL: i64 = 3;
const STAGE_CHECK: i64 = 4;
const STAGE_FINISH: i64 = 5;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
struct Saved {
    files: u64,
    candidates: u64,
    candidate_bytes: u64,
    read_bytes: u64,
    reused: u64,
    skipped_cloud: u64,
    skipped_protected: u64,
    links: u64,
    issues: u64,
    step_done: u64,
    step_total: u64,
    elapsed_ms: u64,
}

/// Análisis sin terminar que se puede reanudar.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Pending {
    pub roots: Vec<String>,
    pub step: u8,
    pub step_done: u64,
    pub step_total: u64,
    pub files: u64,
    pub saved_at: i64,
}

struct Job {
    ctl: Arc<Ctl>,
}

pub struct Detector {
    dir: PathBuf,
    job: Mutex<Option<Job>>,
}

struct Run<'a> {
    cfg: &'a Config,
    ctl: &'a Ctl,
    p: Progress,
    last: Instant,
    on_event: &'a OnEvent,
    workers: usize,
    earlier_ms: u64,
    began: Instant,
}

struct Task {
    id: i64,
    path: String,
    size: u64,
    when: i64,
    /// `None`: aún no se sabe si es un enlace duro.
    link: Option<String>,
}

fn set_kv(c: &Connection, key: &str, value: impl std::fmt::Display) -> Result<(), String> {
    c.execute("INSERT OR REPLACE INTO job(key,value) VALUES(?,?)", params![key, value.to_string()]).map(|_| ()).map_err(e2s)
}

fn kv(c: &Connection, key: &str) -> Result<Option<String>, String> {
    c.query_row("SELECT value FROM job WHERE key=?", [key], |r| r.get(0)).optional().map_err(e2s)
}

impl Run<'_> {
    fn emit(&mut self, force: bool) {
        if force || self.last.elapsed() >= EMIT_EVERY {
            self.last = Instant::now();
            self.p.paused = self.ctl.pause.load(Ordering::Relaxed);
            (self.on_event)(Event::Progress(self.p.clone()));
        }
    }

    fn saved(&self) -> Saved {
        let p = &self.p;
        Saved { files: p.files, candidates: p.candidates, candidate_bytes: p.candidate_bytes, read_bytes: p.read_bytes, reused: p.reused, skipped_cloud: p.skipped_cloud, skipped_protected: p.skipped_protected, links: p.links, issues: p.issues, step_done: p.step_done, step_total: p.step_total, elapsed_ms: self.earlier_ms + self.began.elapsed().as_millis() as u64 }
    }

    /// Debe llamarse dentro de la misma transacción que lo que describe.
    fn save(&self, c: &Connection) -> Result<(), String> {
        set_kv(c, "progress", serde_json::to_string(&self.saved()).map_err(e2s)?)?;
        set_kv(c, "saved_at", now_ms())
    }

    fn stage(&self, c: &Connection, stage: i64) -> Result<(), String> {
        c.execute_batch("BEGIN").map_err(e2s)?;
        set_kv(c, "stage", stage)?;
        self.save(c)?;
        c.execute_batch("COMMIT").map_err(e2s)
    }

    fn scan(&mut self, c: &Connection, excl: &Exclusions) -> Result<bool, String> {
        self.p.phase = "scanning";
        self.p.step = 0;
        let mut stack: Vec<PathBuf> = {
            let mut st = c.prepare("SELECT dir FROM pending").map_err(e2s)?;
            let rows = st.query_map([], |r| r.get::<_, String>(0)).map_err(e2s)?;
            rows.map(|r| r.map(PathBuf::from)).collect::<rusqlite::Result<_>>().map_err(e2s)?
        };
        c.execute_batch("BEGIN").map_err(e2s)?;
        let mut st = c.prepare("INSERT INTO scan(size,path,stamp,modified,flags) VALUES(?,?,?,?,?)").map_err(e2s)?;
        let mut since = 0u32;
        while let Some(dir) = stack.pop() {
            if !self.ctl.go_on() {
                drop(st);
                c.execute_batch("ROLLBACK").map_err(e2s)?;
                return Ok(false);
            }
            if excl.excludes(&dir) {
                self.p.skipped_protected += 1;
                continue;
            }
            let Ok(rd) = fs::read_dir(&dir) else {
                self.p.issues += 1;
                continue;
            };
            self.p.current = dir.display().to_string();
            for (n, ent) in rd.enumerate() {
                if n % 4096 == 4095 {
                    std::thread::sleep(Duration::from_millis(1));
                }
                let Ok(ent) = ent else {
                    self.p.issues += 1;
                    continue;
                };
                let Ok(kind) = ent.file_type() else {
                    self.p.issues += 1;
                    continue;
                };
                if kind.is_symlink() {
                    self.p.links += 1;
                } else if kind.is_dir() {
                    stack.push(ent.path());
                } else if kind.is_file() {
                    let Ok(m) = ent.metadata() else {
                        self.p.issues += 1;
                        continue;
                    };
                    const HIDDEN: u32 = 2;
                    const SYSTEM: u32 = 4;
                    if attrs(&m) & SYSTEM != 0 {
                        self.p.skipped_protected += 1;
                    } else if is_cloud_only(&m) {
                        self.p.skipped_cloud += 1;
                    } else if m.len() >= self.cfg.min_size.max(1) {
                        let path = ent.path().to_string_lossy().into_owned();
                        let mut flags = 0;
                        if attrs(&m) & HIDDEN != 0 {
                            flags |= FLAG_HIDDEN;
                        }
                        if m.permissions().readonly() {
                            flags |= FLAG_READONLY;
                        }
                        if is_special(&path) {
                            flags |= FLAG_SPECIAL;
                        }
                        let modified = m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis() as i64);
                        st.execute(params![m.len() as i64, path, stamp(&m), modified, flags]).map_err(e2s)?;
                        self.p.files += 1;
                        since += 1;
                    }
                }
                self.emit(false);
            }
            // Solo entre carpetas: así la lista de pendientes coincide exactamente con lo ya guardado.
            if since >= COMMIT_EVERY {
                since = 0;
                self.snapshot(c, &stack)?;
                c.execute_batch("COMMIT; BEGIN").map_err(e2s)?;
            }
        }
        drop(st);
        self.snapshot(c, &stack)?;
        c.execute_batch("COMMIT").map_err(e2s)?;
        Ok(true)
    }

    fn snapshot(&self, c: &Connection, stack: &[PathBuf]) -> Result<(), String> {
        c.execute("DELETE FROM pending", []).map_err(e2s)?;
        let mut st = c.prepare_cached("INSERT INTO pending(dir) VALUES(?)").map_err(e2s)?;
        for d in stack {
            st.execute([d.to_string_lossy()]).map_err(e2s)?;
        }
        self.save(c)
    }

    fn pick_candidates(&mut self, c: &Connection) -> Result<(), String> {
        self.p.phase = "grouping";
        self.p.current.clear();
        self.emit(true);
        c.execute_batch("CREATE INDEX IF NOT EXISTS scan_size ON scan(size);").map_err(e2s)?;
        c.execute("UPDATE scan SET state=?1 WHERE state=0 AND size IN (SELECT size FROM scan GROUP BY size HAVING COUNT(*)>1)", [NEEDS_QUICK]).map_err(e2s)?;
        let (n, bytes): (i64, Option<i64>) = c.query_row("SELECT COUNT(*),SUM(size) FROM scan WHERE state=?1", [NEEDS_QUICK], |r| Ok((r.get(0)?, r.get(1)?))).map_err(e2s)?;
        self.p.candidates = n as u64;
        self.p.candidate_bytes = bytes.unwrap_or(0) as u64;
        Ok(())
    }

    /// Reparte una página entre los hilos; este sigue informando del avance.
    fn in_parallel<R: Send>(&mut self, tasks: &[Task], reader: &Reader<'_>, by_bytes: bool, work: impl Fn(&Task, &mut [u8]) -> R + Sync) -> Vec<(usize, R)> {
        if tasks.is_empty() {
            return vec![];
        }
        let (next, finished) = (AtomicUsize::new(0), AtomicUsize::new(0));
        let results = Mutex::new(Vec::with_capacity(tasks.len()));
        let ctl = self.ctl;
        let (base, read_base) = (self.p.step_done, reader.read.load(Ordering::Relaxed));
        std::thread::scope(|s| {
            for _ in 0..self.workers {
                s.spawn(|| {
                    crate::sys::background_thread();
                    let mut buf = vec![0u8; READ_BUF];
                    while ctl.go_on() {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(t) = tasks.get(i) else { break };
                        let r = work(t, &mut buf);
                        results.lock().unwrap().push((i, r));
                        finished.fetch_add(1, Ordering::Relaxed);
                    }
                });
            }
            while finished.load(Ordering::Relaxed) < tasks.len() && !ctl.cancel.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(100));
                let read = reader.read.load(Ordering::Relaxed);
                self.p.read_bytes = read;
                self.p.step_done = base + if by_bytes { read - read_base } else { finished.load(Ordering::Relaxed) as u64 };
                self.emit(false);
            }
        });
        results.into_inner().unwrap()
    }

    fn page(c: &Connection, state: i64, after: i64) -> Result<Vec<Task>, String> {
        let mut st = c.prepare_cached("SELECT id,path,size,stamp,link_key FROM scan WHERE state=?1 AND id>?2 ORDER BY id LIMIT ?3").map_err(e2s)?;
        let rows = st.query_map(params![state, after, PAGE], |r| Ok(Task { id: r.get(0)?, path: r.get(1)?, size: r.get::<_, i64>(2)? as u64, when: r.get(3)?, link: r.get(4)? })).map_err(e2s)?;
        rows.collect::<rusqlite::Result<_>>().map_err(e2s)
    }

    /// Resume cada archivo pendiente en `from`. Devuelve `false` si se interrumpió; lo ya confirmado no se repite.
    fn hash_pass(&mut self, c: &Connection, store: &Connection, from: i64, whole: bool, reader: &Reader<'_>) -> Result<bool, String> {
        let mut after = 0i64;
        let today = now_ms() / 86_400_000;
        loop {
            let page = Self::page(c, from, after)?;
            let Some(last) = page.last() else { return Ok(true) };
            after = last.id;
            self.p.current = page[0].path.clone();
            let mut hits: Vec<(i64, Option<Vec<u8>>, Option<Vec<u8>>)> = vec![];
            let mut misses: Vec<(Task, Option<Vec<u8>>, Option<Vec<u8>>)> = vec![];
            {
                let mut cached = store.prepare_cached("SELECT quick,full FROM hash_cache WHERE path=?1 AND size=?2 AND stamp=?3").map_err(e2s)?;
                for t in page {
                    let found: Option<(Vec<u8>, Option<Vec<u8>>)> = cached.query_row(params![t.path.to_lowercase(), t.size as i64, t.when], |r| Ok((r.get(0)?, r.get(1)?))).optional().map_err(e2s)?;
                    let (quick, full) = found.map_or((None, None), |(q, f)| (Some(q), f));
                    if (whole && full.is_some()) || (!whole && quick.is_some()) {
                        self.p.reused += 1;
                        self.p.step_done += if whole { t.size } else { 1 };
                        hits.push((t.id, quick, full));
                    } else {
                        misses.push((t, quick, full));
                    }
                }
            }
            let tasks: Vec<Task> = misses.iter().map(|(t, ..)| Task { id: t.id, path: t.path.clone(), size: t.size, when: t.when, link: t.link.clone() }).collect();
            let done = self.in_parallel(&tasks, reader, whole, |t, buf| reader.hash(Path::new(&t.path), t.size, t.when, whole, buf));
            self.p.read_bytes = reader.read.load(Ordering::Relaxed);
            let interrupted = self.ctl.cancel.load(Ordering::Relaxed);
            c.execute_batch("BEGIN").map_err(e2s)?;
            store.execute_batch("BEGIN").map_err(e2s)?;
            {
                let mut put = store.prepare_cached("INSERT OR REPLACE INTO hash_cache(path,size,stamp,quick,full,day) VALUES(?,?,?,?,?,?)").map_err(e2s)?;
                let mut set = c.prepare_cached("UPDATE scan SET quick=?2, full=?3, state=?4, link_key=COALESCE(?5,link_key) WHERE id=?1").map_err(e2s)?;
                for (id, quick, full) in &hits {
                    set.execute(params![id, quick, full, QUICK_DONE, Option::<String>::None]).map_err(e2s)?;
                }
                for (i, h) in done {
                    let (t, old_quick, old_full) = &misses[i];
                    match h {
                        Hashed::Cancelled => {}
                        Hashed::Unusable => {
                            self.p.issues += 1;
                            set.execute(params![t.id, Option::<Vec<u8>>::None, Option::<Vec<u8>>::None, UNUSABLE, Option::<String>::None]).map_err(e2s)?;
                        }
                        Hashed::Value(h, link) => {
                            let small = t.size <= WHOLE_MAX;
                            let full = if whole || small { Some(h.clone()) } else { old_full.clone() };
                            let quick = if whole { old_quick.clone() } else { Some(h) };
                            if !too_recent(t.when) {
                                if let Some(q) = &quick {
                                    put.execute(params![t.path.to_lowercase(), t.size as i64, t.when, q, full, today]).map_err(e2s)?;
                                }
                            }
                            set.execute(params![t.id, quick, full, QUICK_DONE, link]).map_err(e2s)?;
                        }
                    }
                }
                self.save(c)?;
            }
            // Primero la caché: si algo se corta entre medias, ese archivo se recupera de ella sin volver a leerlo.
            store.execute_batch("COMMIT").map_err(e2s)?;
            c.execute_batch("COMMIT").map_err(e2s)?;
            self.emit(true);
            if interrupted {
                return Ok(false);
            }
        }
    }

    /// Última comprobación antes de dar por bueno un grupo: el archivo sigue igual y se sabe si es un enlace duro.
    fn check_pass(&mut self, c: &Connection, reader: &Reader<'_>) -> Result<bool, String> {
        let mut after = 0i64;
        loop {
            let page = Self::page(c, NEEDS_CHECK, after)?;
            let Some(last) = page.last() else { return Ok(true) };
            after = last.id;
            let done = self.in_parallel(&page, reader, false, |t, _| {
                let path = Path::new(&t.path);
                match &t.link {
                    // Ya se abrió al leerlo: basta confirmar que sigue igual, sin volver a abrirlo.
                    Some(k) => quick_stat(path).filter(|s| s.size == t.size && s.stamp == t.when && !crate::dir::attrs_cloud_only(s.attrs)).map(|_| k.clone()),
                    None => File::open(path).ok().and_then(|f| identity_of(&f)).filter(|i| i.size == t.size && i.stamp == t.when).map(link_key),
                }
            });
            let interrupted = self.ctl.cancel.load(Ordering::Relaxed);
            c.execute_batch("BEGIN").map_err(e2s)?;
            {
                let mut good = c.prepare_cached("UPDATE scan SET state=?2, link_key=?3 WHERE id=?1").map_err(e2s)?;
                for (i, found) in done {
                    match found {
                        Some(key) => good.execute(params![page[i].id, CHECKED, key]).map_err(e2s)?,
                        None => {
                            self.p.issues += 1;
                            good.execute(params![page[i].id, UNUSABLE, Option::<String>::None]).map_err(e2s)?
                        }
                    };
                }
                self.save(c)?;
            }
            c.execute_batch("COMMIT").map_err(e2s)?;
            self.emit(true);
            if interrupted {
                return Ok(false);
            }
        }
    }

    fn begin_step(&mut self, step: u8, total: u64) {
        self.p.phase = "hashing";
        self.p.step = step;
        self.p.step_done = 0;
        self.p.step_total = total;
        self.emit(true);
    }

    fn finish(&mut self, c: &Connection, store: &Connection) -> Result<Summary, String> {
        self.p.phase = "finishing";
        self.p.step = 0;
        self.p.current.clear();
        self.emit(true);
        c.execute_batch("CREATE INDEX IF NOT EXISTS scan_full ON scan(size, full) WHERE state=5;").map_err(e2s)?;
        store.execute_batch("BEGIN; DELETE FROM dup_files; DELETE FROM dup_groups;").map_err(e2s)?;
        let mut s = Summary { files: self.p.files, roots: normalize_roots(&self.cfg.roots), ..Default::default() };
        {
            let mut groups = c.prepare("SELECT size, full FROM scan WHERE state=5 GROUP BY size, full HAVING COUNT(*)>1 ORDER BY size DESC").map_err(e2s)?;
            let mut members = c.prepare("SELECT path,modified,flags,link_key FROM scan WHERE state=5 AND size=?1 AND full=?2").map_err(e2s)?;
            let mut add_group = store.prepare("INSERT INTO dup_groups(size,hash,reclaimable) VALUES(?,?,?)").map_err(e2s)?;
            let mut add_file = store.prepare("INSERT INTO dup_files(grp,path,modified,flags) VALUES(?,?,?,?)").map_err(e2s)?;
            let mut rows = groups.query([]).map_err(e2s)?;
            while let Some(r) = rows.next().map_err(e2s)? {
                if !self.ctl.go_on() {
                    drop(rows);
                    store.execute_batch("ROLLBACK").map_err(e2s)?;
                    return Err("cancelado".into());
                }
                let (size, full): (i64, Vec<u8>) = (r.get(0).map_err(e2s)?, r.get(1).map_err(e2s)?);
                let list: Vec<(String, i64, u32, Option<String>)> = {
                    let rows = members.query_map(params![size, full], |m| Ok((m.get(0)?, m.get(1)?, m.get(2)?, m.get::<_, Option<String>>(3)?.filter(|k| !k.is_empty())))).map_err(e2s)?;
                    rows.collect::<rusqlite::Result<_>>().map_err(e2s)?
                };
                let mut shared: HashMap<&str, u32> = HashMap::new();
                for (.., key) in &list {
                    if let Some(k) = key {
                        *shared.entry(k).or_insert(0) += 1;
                    }
                }
                let distinct = list.iter().filter(|(.., k)| k.is_none()).count() as u64 + shared.len() as u64;
                if distinct < 2 {
                    continue; // todas las rutas son el mismo archivo físico
                }
                let reclaim = size as u64 * (distinct - 1);
                add_group.execute(params![size, full, reclaim as i64]).map_err(e2s)?;
                let grp = store.last_insert_rowid();
                for (path, modified, flags, key) in &list {
                    let linked = key.as_deref().is_some_and(|k| shared.get(k).is_some_and(|n| *n > 1));
                    add_file.execute(params![grp, path, modified, flags | if linked { FLAG_HARDLINK } else { 0 }]).map_err(e2s)?;
                }
                s.groups += 1;
                s.duplicate_files += list.len() as u64;
                s.reclaimable += reclaim;
                self.emit(false);
            }
        }
        s.finished_at = now_ms();
        s.duration_ms = self.earlier_ms + self.began.elapsed().as_millis() as u64;
        s.read_bytes = self.p.read_bytes;
        s.reused = self.p.reused;
        s.skipped_cloud = self.p.skipped_cloud;
        s.skipped_protected = self.p.skipped_protected;
        s.links = self.p.links;
        s.issues = self.p.issues;
        store.execute("INSERT OR REPLACE INTO meta(key,value) VALUES('summary',?)", [serde_json::to_string(&s).map_err(e2s)?]).map_err(e2s)?;
        store.execute("DELETE FROM hash_cache WHERE day<?", [now_ms() / 86_400_000 - CACHE_KEEP_DAYS]).map_err(e2s)?;
        store.execute_batch("COMMIT").map_err(e2s)?;
        Ok(s)
    }
}

impl Detector {
    pub fn new(dir: PathBuf) -> Self {
        Detector { dir, job: Mutex::new(None) }
    }

    fn store_path(&self) -> PathBuf {
        self.dir.join("duplicates.db")
    }

    fn job_path(&self) -> PathBuf {
        self.dir.join("job.db")
    }

    fn remove_job(&self) {
        let job = self.job_path();
        let _ = fs::remove_file(self.dir.join("scan.tmp")); // de versiones anteriores
        for suffix in ["", "-wal", "-shm"] {
            let _ = fs::remove_file(format!("{}{suffix}", job.display()));
        }
    }

    fn open_read(&self) -> Result<Option<Connection>, String> {
        let p = self.store_path();
        if !p.exists() {
            return Ok(None);
        }
        Connection::open_with_flags(p, OpenFlags::SQLITE_OPEN_READ_ONLY).map(Some).map_err(e2s)
    }

    /// Lo que queda por hacer de un análisis interrumpido. Consultarlo no reanuda nada.
    pub fn pending(&self) -> Result<Option<Pending>, String> {
        if self.running() || !self.job_path().exists() {
            return Ok(None);
        }
        let c = Connection::open_with_flags(self.job_path(), OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(e2s)?;
        let Some(cfg) = kv(&c, "config")? else { return Ok(None) };
        let cfg: Config = serde_json::from_str(&cfg).map_err(e2s)?;
        let saved: Saved = kv(&c, "progress")?.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let stage: i64 = kv(&c, "stage")?.and_then(|s| s.parse().ok()).unwrap_or(STAGE_SCAN);
        Ok(Some(Pending {
            roots: normalize_roots(&cfg.roots),
            step: if (STAGE_QUICK..=STAGE_CHECK).contains(&stage) { (stage - 1) as u8 } else { 0 },
            step_done: saved.step_done,
            step_total: saved.step_total,
            files: saved.files,
            saved_at: kv(&c, "saved_at")?.and_then(|s| s.parse().ok()).unwrap_or(0),
        }))
    }

    fn execute(&self, resume: Option<&Config>, fresh: Option<&Config>, ctl: &Ctl, on_event: &OnEvent) -> Result<Option<Summary>, String> {
        fs::create_dir_all(&self.dir).map_err(e2s)?;
        if fresh.is_some() {
            self.remove_job();
        }
        let store = Connection::open(self.store_path()).map_err(e2s)?;
        store.execute_batch(DURABLE).map_err(e2s)?;
        store.execute_batch(STORE).map_err(e2s)?;
        let job = Connection::open(self.job_path()).map_err(e2s)?;
        job.execute_batch(DURABLE).map_err(e2s)?;
        job.execute_batch(JOB).map_err(e2s)?;
        let cfg = fresh.or(resume).ok_or("sin configuración")?;
        if fresh.is_some() {
            job.execute_batch("BEGIN").map_err(e2s)?;
            set_kv(&job, "config", serde_json::to_string(cfg).map_err(e2s)?)?;
            set_kv(&job, "stage", STAGE_SCAN)?;
            set_kv(&job, "saved_at", now_ms())?;
            for root in normalize_roots(&cfg.roots) {
                job.execute("INSERT INTO pending(dir) VALUES(?)", [root]).map_err(e2s)?;
            }
            job.execute_batch("COMMIT").map_err(e2s)?;
        }
        let saved: Saved = kv(&job, "progress")?.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let stage: i64 = kv(&job, "stage")?.and_then(|s| s.parse().ok()).unwrap_or(STAGE_SCAN);
        let own = self.dir.parent().unwrap_or(&self.dir);
        let excl = Exclusions::new(&cfg.exclusions, own);
        let p = Progress { phase: "scanning", files: saved.files, candidates: saved.candidates, candidate_bytes: saved.candidate_bytes, read_bytes: saved.read_bytes, reused: saved.reused, skipped_cloud: saved.skipped_cloud, skipped_protected: saved.skipped_protected, links: saved.links, issues: saved.issues, step_done: saved.step_done, step_total: saved.step_total, ..Default::default() };
        let mut run = Run { cfg, ctl, p, last: Instant::now() - EMIT_EVERY, on_event, workers: worker_count(&normalize_roots(&cfg.roots)), earlier_ms: saved.elapsed_ms, began: Instant::now() };
        let reader = Reader { ctl, throttle: (cfg.max_mbps > 0).then(|| Mutex::new(Throttle::new(cfg.max_mbps))), read: AtomicU64::new(saved.read_bytes) };

        if stage <= STAGE_SCAN {
            if !run.scan(&job, &excl)? {
                return Ok(None);
            }
            run.stage(&job, STAGE_GROUP)?;
        }
        if stage <= STAGE_GROUP {
            run.pick_candidates(&job)?;
            run.p.step_done = 0;
            run.p.step_total = run.p.candidates;
            run.stage(&job, STAGE_QUICK)?;
        }
        if stage <= STAGE_QUICK {
            let total = run.p.step_total;
            let done = if stage == STAGE_QUICK { run.p.step_done } else { 0 };
            run.begin_step(1, total);
            run.p.step_done = done;
            if !run.hash_pass(&job, &store, NEEDS_QUICK, false, &reader)? {
                return Ok(None);
            }
            // Solo se lee entero lo que coincide en tamaño, principio y final, y aún no tiene hash completo.
            job.execute_batch("CREATE INDEX IF NOT EXISTS scan_quick ON scan(size, quick) WHERE quick IS NOT NULL;").map_err(e2s)?;
            job.execute("UPDATE scan SET state=?1 WHERE state=?2 AND full IS NULL AND EXISTS(SELECT 1 FROM scan o WHERE o.size=scan.size AND o.quick=scan.quick AND o.id<>scan.id)", [NEEDS_FULL, QUICK_DONE]).map_err(e2s)?;
            let bytes: Option<i64> = job.query_row("SELECT SUM(size) FROM scan WHERE state=?1", [NEEDS_FULL], |r| r.get(0)).map_err(e2s)?;
            run.p.step_done = 0;
            run.p.step_total = bytes.unwrap_or(0) as u64;
            run.stage(&job, STAGE_FULL)?;
        }
        if stage <= STAGE_FULL {
            let (total, done) = (run.p.step_total, run.p.step_done);
            run.begin_step(2, total);
            run.p.step_done = done;
            if !run.hash_pass(&job, &store, NEEDS_FULL, true, &reader)? {
                return Ok(None);
            }
            job.execute_batch("CREATE INDEX IF NOT EXISTS scan_hash ON scan(size, full) WHERE full IS NOT NULL;").map_err(e2s)?;
            job.execute("UPDATE scan SET state=?1 WHERE full IS NOT NULL AND state=?2 AND EXISTS(SELECT 1 FROM scan o WHERE o.size=scan.size AND o.full=scan.full AND o.id<>scan.id)", [NEEDS_CHECK, QUICK_DONE]).map_err(e2s)?;
            let n: i64 = job.query_row("SELECT COUNT(*) FROM scan WHERE state=?1", [NEEDS_CHECK], |r| r.get(0)).map_err(e2s)?;
            run.p.step_done = 0;
            run.p.step_total = n as u64;
            run.stage(&job, STAGE_CHECK)?;
        }
        if stage <= STAGE_CHECK {
            let (total, done) = (run.p.step_total, run.p.step_done);
            run.begin_step(3, total);
            run.p.step_done = done;
            if !run.check_pass(&job, &reader)? {
                return Ok(None);
            }
            run.stage(&job, STAGE_FINISH)?;
        }
        run.finish(&job, &store).map(Some)
    }

    fn launch(self: &Arc<Self>, resume: Option<Config>, fresh: Option<Config>, on_event: OnEvent) -> Result<(), String> {
        let mut job = self.job.lock().unwrap();
        if job.is_some() {
            return Err("Ya hay un análisis de duplicados en curso.".into());
        }
        let ctl = Arc::new(Ctl::default());
        *job = Some(Job { ctl: ctl.clone() });
        drop(job);
        let me = self.clone();
        let spawned = std::thread::Builder::new().name("lfind-duplicates".into()).spawn(move || {
            crate::sys::background_thread();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| me.execute(resume.as_ref(), fresh.as_ref(), &ctl, &on_event)));
            let finished = matches!(result, Ok(Ok(Some(_))));
            let discard = ctl.discard.load(Ordering::Relaxed);
            let interrupted = ctl.cancel.load(Ordering::Relaxed);
            if finished || discard {
                me.remove_job();
            }
            *me.job.lock().unwrap() = None;
            on_event(match result {
                Ok(Ok(Some(summary))) => Event::Done { outcome: "completed", message: None, summary: Some(summary) },
                Ok(Ok(None)) if discard => Event::Done { outcome: "cancelled", message: None, summary: None },
                Ok(Ok(None)) => Event::Done { outcome: "suspended", message: None, summary: None },
                Ok(Err(_)) if discard => Event::Done { outcome: "cancelled", message: None, summary: None },
                Ok(Err(_)) if interrupted => Event::Done { outcome: "suspended", message: None, summary: None },
                Ok(Err(m)) => Event::Done { outcome: "failed", message: Some(m), summary: None },
                Err(_) => Event::Done { outcome: "failed", message: Some("Error interno del detector.".into()), summary: None },
            });
        });
        if let Err(e) = spawned {
            *self.job.lock().unwrap() = None;
            return Err(format!("No se pudo iniciar el análisis: {e}"));
        }
        Ok(())
    }

    /// Empieza un análisis nuevo, descartando uno interrumpido. Nunca se llama sin que el usuario pulse «Detectar duplicados».
    pub fn start(self: &Arc<Self>, cfg: Config, on_event: OnEvent) -> Result<(), String> {
        if normalize_roots(&cfg.roots).is_empty() {
            return Err("Elige al menos una carpeta o unidad.".into());
        }
        self.launch(None, Some(cfg), on_event)
    }

    /// Continúa el análisis interrumpido. Solo a petición del usuario.
    pub fn resume(self: &Arc<Self>, on_event: OnEvent) -> Result<(), String> {
        if self.running() {
            return Err("Ya hay un análisis de duplicados en curso.".into());
        }
        if !self.job_path().exists() {
            return Err("No hay ningún análisis que reanudar.".into());
        }
        let c = Connection::open_with_flags(self.job_path(), OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(e2s)?;
        let cfg: Config = serde_json::from_str(&kv(&c, "config")?.ok_or("El análisis guardado no es válido.")?).map_err(e2s)?;
        drop(c);
        self.launch(Some(cfg), None, on_event)
    }

    pub fn discard_pending(&self) -> Result<(), String> {
        if self.running() {
            return Err("Espera a que termine el análisis.".into());
        }
        self.remove_job();
        Ok(())
    }

    pub fn running(&self) -> bool {
        self.job.lock().unwrap().is_some()
    }

    pub fn pause(&self, paused: bool) {
        if let Some(j) = self.job.lock().unwrap().as_ref() {
            j.ctl.pause.store(paused, Ordering::Relaxed);
        }
    }

    /// Abandona el análisis y tira lo hecho. Los resultados anteriores se conservan.
    pub fn cancel(&self) {
        if let Some(j) = self.job.lock().unwrap().as_ref() {
            j.ctl.discard.store(true, Ordering::Relaxed);
            j.ctl.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Para el análisis guardando el avance, para poder reanudarlo.
    pub fn suspend(&self) {
        if let Some(j) = self.job.lock().unwrap().as_ref() {
            j.ctl.cancel.store(true, Ordering::Relaxed);
        }
    }

    pub fn summary(&self) -> Result<Option<Summary>, String> {
        let Some(c) = self.open_read()? else { return Ok(None) };
        let v: Option<String> = c.query_row("SELECT value FROM meta WHERE key='summary'", [], |r| r.get(0)).optional().map_err(e2s)?;
        Ok(v.and_then(|s| serde_json::from_str(&s).ok()))
    }

    /// Una página de grupos, los que más espacio liberarían primero.
    pub fn groups(&self, offset: u32, limit: u32) -> Result<Vec<Group>, String> {
        let Some(c) = self.open_read()? else { return Ok(vec![]) };
        let mut st = c.prepare("SELECT id,size,hash,reclaimable FROM dup_groups ORDER BY reclaimable DESC, id LIMIT ? OFFSET ?").map_err(e2s)?;
        let mut mem = c.prepare("SELECT path,modified,flags FROM dup_files WHERE grp=? ORDER BY path LIMIT 500").map_err(e2s)?;
        let rows = st.query_map(params![limit.min(500), offset], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, Vec<u8>>(2)?, r.get::<_, i64>(3)?))).map_err(e2s)?;
        let mut out = vec![];
        for row in rows {
            let (id, size, hash, reclaim) = row.map_err(e2s)?;
            let members = mem.query_map([id], |r| Ok(Member { path: r.get(0)?, modified: r.get(1)?, flags: r.get(2)? })).map_err(e2s)?.collect::<rusqlite::Result<Vec<_>>>().map_err(e2s)?;
            out.push(Group { id, size: size as u64, hash: hash.iter().map(|b| format!("{b:02x}")).collect(), reclaimable: reclaim as u64, members });
        }
        Ok(out)
    }

    /// Última barrera antes de la papelera: en cada grupo debe quedar al menos una copia que siga en disco y sin cambios.
    pub fn check_removal(&self, paths: &[String]) -> Result<RemovalCheck, String> {
        let Some(c) = self.open_read()? else { return Err("No hay resultados guardados.".into()) };
        let chosen: HashSet<&str> = paths.iter().map(String::as_str).collect();
        let mut find = c.prepare("SELECT grp FROM dup_files WHERE path=?").map_err(e2s)?;
        let mut members = c.prepare("SELECT f.path, g.size FROM dup_files f JOIN dup_groups g ON g.id=f.grp WHERE f.grp=?").map_err(e2s)?;
        let mut out = RemovalCheck::default();
        let mut safe: HashMap<i64, bool> = HashMap::new();
        for p in paths {
            let Some(grp) = find.query_row([p], |r| r.get::<_, i64>(0)).optional().map_err(e2s)? else {
                out.blocked.push((p.clone(), "No forma parte de los resultados.".into()));
                continue;
            };
            let ok = match safe.get(&grp) {
                Some(v) => *v,
                None => {
                    let list: Vec<(String, i64)> = members.query_map([grp], |r| Ok((r.get(0)?, r.get(1)?))).map_err(e2s)?.collect::<rusqlite::Result<_>>().map_err(e2s)?;
                    let v = list.iter().any(|(path, size)| !chosen.contains(path.as_str()) && fs::metadata(path).is_ok_and(|m| m.len() == *size as u64));
                    safe.insert(grp, v);
                    v
                }
            };
            if ok {
                out.allowed.push(p.clone());
            } else {
                out.blocked.push((p.clone(), "Se quedaría sin ninguna copia de este archivo.".into()));
            }
        }
        Ok(out)
    }

    /// Retira de los resultados lo que ya no existe. Un grupo con una sola copia deja de ser un grupo.
    pub fn prune(&self, paths: &[String]) -> Result<(), String> {
        if !self.store_path().exists() {
            return Ok(());
        }
        let mut c = Connection::open(self.store_path()).map_err(e2s)?;
        let tx = c.transaction().map_err(e2s)?;
        for p in paths.iter().filter(|p| fs::symlink_metadata(p).is_err()) {
            tx.execute("DELETE FROM dup_files WHERE path=?", [p]).map_err(e2s)?;
        }
        tx.execute_batch(
            "DELETE FROM dup_files WHERE grp IN (SELECT grp FROM dup_files GROUP BY grp HAVING COUNT(*)<2);
             DELETE FROM dup_groups WHERE id NOT IN (SELECT grp FROM dup_files);
             UPDATE dup_groups SET reclaimable=size*((SELECT COUNT(*) FROM dup_files f WHERE f.grp=dup_groups.id)-1);",
        )
        .map_err(e2s)?;
        tx.commit().map_err(e2s)
    }

    /// Olvida los resultados y la caché de hashes.
    pub fn forget(&self) -> Result<(), String> {
        if self.running() {
            return Err("Espera a que termine el análisis.".into());
        }
        match fs::remove_file(self.store_path()) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e2s(e)),
            _ => Ok(()),
        }
    }
}

/// Compara dos archivos byte a byte, por bloques. No descarga lo que solo está en la nube.
pub fn compare(a: &Path, b: &Path, cancel: &AtomicBool) -> Result<Option<Comparison>, String> {
    let (ma, mb) = (fs::metadata(a).map_err(e2s)?, fs::metadata(b).map_err(e2s)?);
    if is_cloud_only(&ma) || is_cloud_only(&mb) {
        return Err("Uno de los archivos solo está en la nube: compararlo obligaría a descargarlo.".into());
    }
    if ma.len() != mb.len() {
        return Ok(Some(Comparison { identical: false, first_difference: Some(ma.len().min(mb.len())) }));
    }
    let (mut fa, mut fb) = (File::open(a).map_err(e2s)?, File::open(b).map_err(e2s)?);
    let (mut ba, mut bb) = (vec![0u8; READ_BUF], vec![0u8; READ_BUF]);
    let mut at = 0u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let n = fa.read(&mut ba).map_err(e2s)?;
        if n == 0 {
            return Ok(Some(Comparison { identical: true, first_difference: None }));
        }
        fb.read_exact(&mut bb[..n]).map_err(e2s)?;
        if let Some(i) = ba[..n].iter().zip(&bb[..n]).position(|(x, y)| x != y) {
            return Ok(Some(Comparison { identical: false, first_difference: Some(at + i as u64) }));
        }
        at += n as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;
    use std::sync::mpsc;

    static N: AtomicU32 = AtomicU32::new(0);
    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("lfind-dup-{tag}-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir_all(&d).unwrap();
        d
    }
    fn put(p: &Path, data: &[u8]) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, data).unwrap();
    }
    fn cfg(root: &Path) -> Config {
        Config { roots: vec![root.to_string_lossy().into_owned()], exclusions: vec![], min_size: 1, max_mbps: 0 }
    }
    fn detector() -> Arc<Detector> {
        Arc::new(Detector::new(tmp("store").join("sub")))
    }
    fn run(d: &Arc<Detector>, c: Config) -> (&'static str, Option<Summary>) {
        let (tx, rx) = mpsc::channel();
        d.start(c, Box::new(move |e| {
            if let Event::Done { outcome, summary, .. } = e {
                let _ = tx.send((outcome, summary));
            }
        }))
        .unwrap();
        rx.recv_timeout(Duration::from_secs(60)).unwrap()
    }
    fn wait_idle(d: &Detector) {
        for _ in 0..200 {
            if !d.running() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    /// Sus fechas quedan lo bastante viejas para poder guardar el hash.
    fn age(p: &Path) {
        let f = File::options().write(true).open(p).unwrap();
        f.set_modified(SystemTime::now() - Duration::from_secs(3600)).unwrap();
    }

    #[test]
    fn only_equal_content_counts_as_a_duplicate() {
        let r = tmp("a");
        put(&r.join("x/uno.txt"), b"contenido igual");
        put(&r.join("y/otro-nombre.bin"), b"contenido igual");
        put(&r.join("y/mismo-tamano.txt"), b"contenido distin");
        put(&r.join("z/uno.txt"), b"mismo nombre, otro contenido");
        put(&r.join("vacio-a"), b"");
        put(&r.join("vacio-b"), b"");
        let d = detector();
        let (out, s) = run(&d, cfg(&r));
        let s = s.unwrap();
        assert_eq!((out, s.groups, s.duplicate_files, s.reclaimable), ("completed", 1, 2, 15));
        let g = d.groups(0, 10).unwrap();
        let mut names: Vec<String> = g[0].members.iter().map(|m| Path::new(&m.path).file_name().unwrap().to_string_lossy().into_owned()).collect();
        names.sort();
        assert_eq!(names, ["otro-nombre.bin", "uno.txt"]);
        assert_eq!(g[0].hash.len(), 64);
    }

    #[test]
    fn large_files_with_the_same_edges_are_compared_entirely() {
        let r = tmp("edges");
        let mut a = vec![5u8; (EDGE * 8) as usize];
        put(&r.join("a.bin"), &a);
        put(&r.join("b.bin"), &a);
        a[(EDGE * 4) as usize] = 9; // igual principio y final, distinto en medio
        put(&r.join("c.bin"), &a);
        let d = detector();
        let s = run(&d, cfg(&r)).1.unwrap();
        assert_eq!(s.groups, 1);
        assert_eq!(d.groups(0, 5).unwrap()[0].members.len(), 2);
    }

    #[test]
    fn different_edges_avoid_reading_the_whole_file() {
        let r = tmp("skip");
        let n = (EDGE * 200) as usize;
        let mut a = vec![1u8; n];
        put(&r.join("a.bin"), &a);
        a[0] = 2;
        put(&r.join("b.bin"), &a);
        let s = run(&detector(), cfg(&r)).1.unwrap();
        assert_eq!(s.groups, 0);
        assert_eq!(s.read_bytes, EDGE * 4, "solo se leyeron los extremos de los dos archivos");
    }

    #[test]
    fn nested_or_repeated_roots_do_not_count_a_file_twice() {
        assert_eq!(normalize_roots(&["C:\\a".into(), "c:\\A\\b".into(), "C:\\a\\".into(), "C:\\ab".into()]), vec!["C:\\a".to_string(), "C:\\ab".to_string()]);
        assert_eq!(normalize_roots(&["C:\\".into(), "C:\\x".into(), "D:".into()]), vec!["C:\\".to_string(), "D:\\".to_string()]);
        let r = tmp("nest");
        put(&r.join("sub/a.txt"), b"unico");
        let mut c = cfg(&r);
        c.roots.push(r.join("sub").to_string_lossy().into_owned());
        assert_eq!(run(&detector(), c).1.unwrap().groups, 0);
    }

    #[test]
    fn several_roots_are_searched_together() {
        let (a, b) = (tmp("r1"), tmp("r2"));
        put(&a.join("x.txt"), b"entre carpetas");
        put(&b.join("y.txt"), b"entre carpetas");
        let mut c = cfg(&a);
        c.roots.push(b.to_string_lossy().into_owned());
        assert_eq!(run(&detector(), c).1.unwrap().groups, 1);
    }

    #[test]
    fn exclusions_by_path_and_by_name() {
        let r = tmp("ex");
        put(&r.join("a.txt"), b"misma cosa");
        put(&r.join("privado/b.txt"), b"misma cosa");
        put(&r.join("x/node_modules/c.txt"), b"misma cosa");
        let mut c = cfg(&r);
        c.exclusions = vec![r.join("privado").to_string_lossy().into_owned(), "node_modules".into()];
        assert_eq!(run(&detector(), c).1.unwrap().groups, 0);
    }

    #[test]
    fn default_exclusions_cover_the_critical_places() {
        let v = default_exclusions();
        assert!(v.iter().any(|x| x == "$Recycle.Bin") && v.iter().any(|x| x == "System Volume Information"));
        #[cfg(windows)]
        assert!(v.iter().any(|x| x.to_lowercase().ends_with("\\windows")));
    }

    #[test]
    fn the_second_run_reuses_hashes_and_a_change_invalidates_them() {
        let r = tmp("cache");
        let body = vec![3u8; 100_000];
        for n in ["a.bin", "b.bin"] {
            put(&r.join(n), &body);
            age(&r.join(n));
        }
        let d = detector();
        let first = run(&d, cfg(&r)).1.unwrap();
        assert!(first.read_bytes >= 200_000 && first.reused == 0);
        let second = run(&d, cfg(&r)).1.unwrap();
        assert_eq!((second.read_bytes, second.groups), (0, 1), "no se vuelve a leer nada");
        assert!(second.reused >= 2);
        // Mismo tamaño, contenido y fecha nuevos: la caché no vale.
        let mut other = body.clone();
        other[50_000] = 9;
        put(&r.join("a.bin"), &other);
        age(&r.join("a.bin"));
        let f = File::options().write(true).open(r.join("a.bin")).unwrap();
        f.set_modified(SystemTime::now() - Duration::from_secs(1800)).unwrap();
        drop(f);
        let third = run(&d, cfg(&r)).1.unwrap();
        assert_eq!(third.groups, 0);
        assert!(third.read_bytes > 0);
    }

    #[test]
    fn a_just_written_file_is_not_cached() {
        let r = tmp("fresh");
        put(&r.join("a.bin"), &[1u8; 40_000]);
        put(&r.join("b.bin"), &[1u8; 40_000]);
        let d = detector();
        run(&d, cfg(&r));
        let again = run(&d, cfg(&r)).1.unwrap();
        assert!(again.read_bytes > 0 || again.reused > 0);
    }

    #[test]
    fn hard_links_are_not_duplicates() {
        let r = tmp("hl");
        put(&r.join("a.txt"), b"un solo archivo");
        fs::hard_link(r.join("a.txt"), r.join("b.txt")).unwrap();
        assert_eq!(run(&detector(), cfg(&r)).1.unwrap().groups, 0);
        put(&r.join("copia.txt"), b"un solo archivo");
        let d = detector();
        let s = run(&d, cfg(&r)).1.unwrap();
        assert_eq!((s.groups, s.reclaimable), (1, 15), "solo la copia real libera espacio");
        let g = d.groups(0, 1).unwrap();
        assert_eq!(g[0].members.iter().filter(|m| m.flags & FLAG_HARDLINK != 0).count(), 2);
    }

    #[cfg(windows)]
    #[test]
    fn cloud_only_and_system_files_are_left_out_untouched() {
        let r = tmp("nube");
        for n in ["a.txt", "b.txt", "c.txt"] {
            put(&r.join(n), b"lo mismo");
        }
        let flag = |f: &str, p: PathBuf| assert!(std::process::Command::new("attrib").arg(f).arg(p).status().unwrap().success());
        flag("+O", r.join("b.txt"));
        flag("+S", r.join("c.txt"));
        let s = run(&detector(), cfg(&r)).1.unwrap();
        assert_eq!((s.groups, s.read_bytes, s.skipped_cloud), (0, 0, 1));
        assert!(s.skipped_protected >= 1);
        flag("-O", r.join("b.txt"));
        flag("-S", r.join("c.txt"));
    }

    #[test]
    fn the_last_copy_of_a_group_can_never_be_removed() {
        let r = tmp("del");
        for n in ["a.txt", "b.txt", "c.txt"] {
            put(&r.join(n), b"tres copias");
        }
        let d = detector();
        run(&d, cfg(&r));
        let p = |n: &str| r.join(n).to_string_lossy().into_owned();
        let all = d.check_removal(&[p("a.txt"), p("b.txt"), p("c.txt")]).unwrap();
        assert!(all.allowed.is_empty() && all.blocked.len() == 3);
        let two = d.check_removal(&[p("a.txt"), p("b.txt")]).unwrap();
        assert_eq!((two.allowed.len(), two.blocked.len()), (2, 0));
        // La copia que iba a quedar ya no existe: tampoco se permite.
        fs::remove_file(r.join("c.txt")).unwrap();
        assert_eq!(d.check_removal(&[p("a.txt"), p("b.txt")]).unwrap().allowed.len(), 0);
        let other = d.check_removal(&["C:\\no-es-del-resultado.txt".to_string()]).unwrap();
        assert_eq!(other.blocked.len(), 1);
    }

    #[test]
    fn prune_keeps_the_results_consistent() {
        let r = tmp("prune");
        for n in ["a.txt", "b.txt", "c.txt"] {
            put(&r.join(n), b"tres copias");
        }
        let d = detector();
        run(&d, cfg(&r));
        fs::remove_file(r.join("a.txt")).unwrap();
        d.prune(&[r.join("a.txt").to_string_lossy().into_owned()]).unwrap();
        let g = d.groups(0, 5).unwrap();
        assert_eq!((g[0].members.len(), g[0].reclaimable), (2, 11));
        fs::remove_file(r.join("b.txt")).unwrap();
        d.prune(&[r.join("b.txt").to_string_lossy().into_owned()]).unwrap();
        assert!(d.groups(0, 5).unwrap().is_empty(), "una sola copia ya no es un grupo");
    }

    #[test]
    fn cancelling_keeps_the_previous_results_and_leaves_no_temporary_file() {
        let r = tmp("cancel");
        for i in 0..3000 {
            put(&r.join(format!("d{}/f{i}.txt", i % 30)), b"contenido repetido");
        }
        let d = detector();
        let done = run(&d, cfg(&r)).1.unwrap();
        assert_eq!(done.groups, 1);
        let (tx, rx) = mpsc::channel();
        d.start(cfg(&r), Box::new(move |e| {
            if let Event::Done { outcome, .. } = e {
                let _ = tx.send(outcome);
            }
        }))
        .unwrap();
        d.cancel();
        assert_eq!(rx.recv_timeout(Duration::from_secs(30)).unwrap(), "cancelled");
        wait_idle(&d);
        assert_eq!(d.summary().unwrap().unwrap().groups, 1);
        assert!(!d.job_path().exists(), "cancelar no deja nada que reanudar");
        assert!(d.pending().unwrap().is_none());
    }

    #[test]
    fn pause_holds_the_work_and_resume_finishes_it() {
        let r = tmp("pause");
        put(&r.join("a.txt"), b"igual igual");
        put(&r.join("b.txt"), b"igual igual");
        let d = detector();
        let (tx, rx) = mpsc::channel();
        d.start(cfg(&r), Box::new(move |e| {
            if let Event::Done { outcome, .. } = e {
                let _ = tx.send(outcome);
            }
        }))
        .unwrap();
        d.pause(true);
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err(), "en pausa no termina");
        d.pause(false);
        assert_eq!(rx.recv_timeout(Duration::from_secs(30)).unwrap(), "completed");
    }

    #[test]
    fn only_one_analysis_at_a_time_and_needs_a_root() {
        let d = detector();
        assert!(d.start(Config { roots: vec![], exclusions: vec![], min_size: 1, max_mbps: 0 }, Box::new(|_| {})).is_err());
        let r = tmp("uno");
        put(&r.join("a"), b"x");
        d.start(cfg(&r), Box::new(|_| {})).unwrap();
        d.pause(true);
        assert!(d.start(cfg(&r), Box::new(|_| {})).is_err());
        d.cancel();
        wait_idle(&d);
    }

    #[test]
    fn the_read_limit_is_respected() {
        let r = tmp("lim");
        let body = vec![8u8; 3 * 1024 * 1024];
        put(&r.join("a.bin"), &body);
        put(&r.join("b.bin"), &body);
        let mut c = cfg(&r);
        c.max_mbps = 8;
        let t = Instant::now();
        let s = run(&detector(), c).1.unwrap();
        assert_eq!(s.groups, 1);
        assert!(t.elapsed() >= Duration::from_millis(500), "{:?}", t.elapsed());
    }

    #[test]
    fn compares_byte_by_byte() {
        let r = tmp("cmp");
        put(&r.join("a"), b"hola mundo");
        put(&r.join("b"), b"hola mundo");
        put(&r.join("c"), b"hola Mundo");
        put(&r.join("d"), b"hola");
        let never = AtomicBool::new(false);
        assert!(compare(&r.join("a"), &r.join("b"), &never).unwrap().unwrap().identical);
        assert_eq!(compare(&r.join("a"), &r.join("c"), &never).unwrap().unwrap().first_difference, Some(5));
        assert!(!compare(&r.join("a"), &r.join("d"), &never).unwrap().unwrap().identical);
        assert!(compare(&r.join("a"), &r.join("b"), &AtomicBool::new(true)).unwrap().is_none());
    }

    fn outcome_of(rx: mpsc::Receiver<Event>) -> (&'static str, Option<Summary>) {
        loop {
            if let Event::Done { outcome, summary, .. } = rx.recv_timeout(Duration::from_secs(60)).unwrap() {
                return (outcome, summary);
            }
        }
    }

    fn start_tx(d: &Arc<Detector>, c: Config) -> mpsc::Receiver<Event> {
        let (tx, rx) = mpsc::channel();
        d.start(c, Box::new(move |e| { let _ = tx.send(e); })).unwrap();
        rx
    }

    fn resume_tx(d: &Arc<Detector>) -> mpsc::Receiver<Event> {
        let (tx, rx) = mpsc::channel();
        d.resume(Box::new(move |e| { let _ = tx.send(e); })).unwrap();
        rx
    }

    fn tree(r: &Path, groups: usize) {
        for i in 0..groups {
            let body = vec![i as u8; 2048 + i];
            put(&r.join(format!("a/{i}.bin")), &body);
            put(&r.join(format!("b/{i}.bin")), &body);
        }
    }

    #[test]
    fn suspending_keeps_the_work_and_resume_finishes_it_correctly() {
        let r = tmp("susp");
        tree(&r, 300);
        let d = detector();
        let rx = start_tx(&d, cfg(&r));
        d.suspend();
        assert_eq!(outcome_of(rx).0, "suspended");
        wait_idle(&d);
        let p = d.pending().unwrap().expect("queda para reanudar");
        assert_eq!(p.roots.len(), 1);
        let (out, s) = outcome_of(resume_tx(&d));
        let s = s.unwrap();
        assert_eq!((out, s.groups, s.duplicate_files), ("completed", 300, 600));
        wait_idle(&d);
        assert!(d.pending().unwrap().is_none() && !d.job_path().exists(), "al terminar no queda nada pendiente");
    }

    #[test]
    fn a_new_instance_can_resume_after_closing_the_app_in_every_stage() {
        // Cada stage se interrumpe en un punto distinto; el resultado debe ser el de un análisis sin cortes.
        for delay in [0u64, 5, 25, 60, 120] {
            let r = tmp("cierre");
            tree(&r, 400);
            let store = tmp("cierre-st").join("sub");
            let a = Arc::new(Detector::new(store.clone()));
            let rx = start_tx(&a, cfg(&r));
            std::thread::sleep(Duration::from_millis(delay));
            a.suspend();
            let first = outcome_of(rx).0;
            wait_idle(&a);
            drop(a);
            let b = Arc::new(Detector::new(store));
            let s = if first == "suspended" { outcome_of(resume_tx(&b)).1.unwrap() } else { b.summary().unwrap().unwrap() };
            assert_eq!((s.groups, s.duplicate_files, s.files), (400, 800, 800), "cortando a los {delay} ms");
        }
    }

    #[test]
    fn nothing_is_resumed_on_its_own() {
        let r = tmp("auto");
        tree(&r, 50);
        let d = detector();
        let rx = start_tx(&d, cfg(&r));
        d.suspend();
        outcome_of(rx);
        wait_idle(&d);
        let store = d.dir.clone();
        let b = Arc::new(Detector::new(store));
        std::thread::sleep(Duration::from_millis(200));
        assert!(!b.running() && b.pending().unwrap().is_some());
    }

    #[test]
    fn a_file_changed_while_suspended_is_not_reported_as_a_duplicate() {
        let r = tmp("cambio");
        put(&r.join("a.bin"), &[1u8; 40_000]);
        put(&r.join("b.bin"), &[1u8; 40_000]);
        put(&r.join("c.bin"), &[1u8; 40_000]);
        let d = detector();
        let rx = start_tx(&d, cfg(&r));
        d.pause(true);
        // Puede haber llegado a cualquier etapa: lo importante es que el resultado final describa el disco de ahora.
        std::thread::sleep(Duration::from_millis(50));
        put(&r.join("c.bin"), &[2u8; 40_000]);
        let f = File::options().write(true).open(r.join("c.bin")).unwrap();
        f.set_modified(SystemTime::now() - Duration::from_secs(4000)).unwrap();
        drop(f);
        d.pause(false);
        let (_, s) = outcome_of(rx);
        if let Some(s) = s {
            let g = d.groups(0, 5).unwrap();
            let listed: Vec<&str> = g.iter().flat_map(|x| x.members.iter().map(|m| m.path.as_str())).collect();
            assert!(!listed.iter().any(|p| p.ends_with("c.bin")), "{listed:?} {s:?}");
        }
    }

    #[test]
    fn progress_reports_the_three_steps() {
        let r = tmp("prog");
        tree(&r, 20);
        let big = vec![4u8; (EDGE * 8) as usize];
        put(&r.join("g1.bin"), &big);
        put(&r.join("g2.bin"), &big);
        let d = detector();
        let rx = start_tx(&d, cfg(&r));
        let (mut steps, mut totals) = (HashSet::new(), HashMap::new());
        loop {
            match rx.recv_timeout(Duration::from_secs(30)).unwrap() {
                Event::Progress(p) => {
                    steps.insert(p.step);
                    if p.step > 0 {
                        totals.insert(p.step, p.step_total);
                        assert!(p.step_done <= p.step_total, "{p:?}");
                    }
                }
                Event::Done { .. } => break,
            }
        }
        assert!(steps.contains(&1) && steps.contains(&3), "{steps:?}");
        assert_eq!(totals[&1], 42, "todos los candidatos");
        assert_eq!(totals[&3], 42, "se verifica cada miembro de un grupo");
        assert_eq!(totals.get(&2), Some(&(EDGE * 16)), "bytes de los archivos que hay que leer enteros");
    }

    #[test]
    fn workers_are_bounded() {
        assert!(worker_count(&["C:\\".to_string()]) <= MAX_WORKERS);
        assert_eq!(worker_count(&["\\\\servidor\\c".to_string()]), 1, "en red no se abusa");
    }

    #[test]
    fn nothing_runs_until_asked() {
        let d = detector();
        assert!(!d.running());
        assert!(d.summary().unwrap().is_none() && d.groups(0, 5).unwrap().is_empty());
        assert!(!d.dir.exists(), "crear el detector no toca el disco");
    }
}
