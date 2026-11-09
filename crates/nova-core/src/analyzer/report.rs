//! Informes del analizador: una base de datos por ubicación, en una carpeta propia.
//!
//! El análisis escribe en un archivo temporal y solo sustituye al informe anterior cuando termina bien: cancelar o fallar deja intacto
//! el último informe válido. Los archivos se guardan en orden de recorrido, de modo que el contenido de una carpeta es un intervalo de
//! identificadores: las consultas no necesitan un índice por carpeta y la interfaz solo recibe páginas.

use super::walk::*;
use crate::index::classify::path_key;
use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

const SCHEMA: i64 = 1;
const COMMIT_EVERY: u32 = 20_000;
const MAX_ISSUES: u32 = 500;
/// Informes conservados; al superarse se retira el analizado hace más tiempo.
const MAX_REPORTS: usize = 24;
/// Por debajo de estos archivos es más barato ordenar el intervalo que recorrer el índice por tamaño.
const SMALL_RANGE: i64 = 50_000;
const MAX_PAGE: u32 = 500;

const CREATE: &str = "
PRAGMA journal_mode=OFF; PRAGMA synchronous=OFF; PRAGMA locking_mode=EXCLUSIVE; PRAGMA temp_store=MEMORY; PRAGMA cache_size=-8000;
CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL) WITHOUT ROWID;
CREATE TABLE exts(id INTEGER PRIMARY KEY, ext TEXT NOT NULL, cat INTEGER NOT NULL, files INTEGER NOT NULL, bytes INTEGER NOT NULL);
CREATE TABLE issues(path TEXT NOT NULL, message TEXT NOT NULL);
CREATE TABLE dirs(id INTEGER PRIMARY KEY, parent INTEGER NOT NULL, last INTEGER NOT NULL, name TEXT NOT NULL, first_file INTEGER NOT NULL, end_file INTEGER NOT NULL,
  own_files INTEGER NOT NULL, own_bytes INTEGER NOT NULL, files INTEGER NOT NULL, dirs INTEGER NOT NULL, bytes INTEGER NOT NULL, cloud_bytes INTEGER NOT NULL, mtime INTEGER, flags INTEGER NOT NULL);
CREATE TABLE files(id INTEGER PRIMARY KEY, dir INTEGER NOT NULL, name TEXT NOT NULL, ext INTEGER NOT NULL, cat INTEGER NOT NULL, size INTEGER NOT NULL, mtime INTEGER, flags INTEGER NOT NULL);
BEGIN;";

const FINISH: &str = "
CREATE INDEX files_size ON files(size DESC, ext, cat);
CREATE INDEX dirs_parent ON dirs(parent, bytes DESC);
CREATE INDEX dirs_bytes ON dirs(bytes DESC);";

fn e2s(e: rusqlite::Error) -> String {
    e.to_string()
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Identificador estable de una ubicación (FNV-1a): la misma ruta, escrita como sea, da el mismo informe.
fn key_of(root: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in path_key(root).bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// `C:` se convierte en `C:\`: sin la barra sería la carpeta actual de esa unidad.
fn clean_root(root: &str) -> String {
    let t = root.trim();
    if t.len() == 2 && t.ends_with(':') {
        format!("{t}{}", std::path::MAIN_SEPARATOR)
    } else {
        t.to_string()
    }
}

fn is_drive_root(root: &str) -> bool {
    let b = root.as_bytes();
    b.len() <= 3 && b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

/// Número de serie del volumen de la unidad con letra: distingue dos discos que usan la misma letra.
#[cfg(windows)]
fn volume_serial(root: &str) -> Option<u32> {
    use crate::sys::win;
    if root.as_bytes().get(1) != Some(&b':') {
        return None;
    }
    let w = win::wide(std::ffi::OsStr::new(&format!("{}\\", &root[..2])));
    let mut serial = 0u32;
    let ok = unsafe { win::GetVolumeInformationW(w.as_ptr(), std::ptr::null_mut(), 0, &mut serial, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), 0) };
    (ok != 0).then_some(serial)
}

#[cfg(not(windows))]
fn volume_serial(_root: &str) -> Option<u32> {
    None
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct ReportInfo {
    pub key: String,
    pub root: String,
    pub root_key: String,
    pub started_at: i64,
    pub finished_at: i64,
    pub duration_ms: u64,
    pub files: u64,
    pub dirs: u64,
    pub bytes: u64,
    pub cloud_bytes: u64,
    pub cloud_files: u64,
    pub links: u64,
    pub errors: u64,
    pub cloud_dirs: u64,
    pub volume_serial: Option<u32>,
    /// Tamaño del informe en disco.
    #[serde(default)]
    pub db_bytes: u64,
    /// `ok`, `missing` (la ubicación ya no existe), `otherVolume` (hay otro disco en esa letra) o `unchecked`.
    #[serde(default)]
    pub state: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AnalyzeEvent {
    Progress(Progress),
    Finishing,
    Done { outcome: &'static str, message: Option<String>, report: Option<ReportInfo> },
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DirHit {
    pub id: i64,
    pub parent: i64,
    pub name: String,
    pub path: String,
    pub bytes: u64,
    pub cloud_bytes: u64,
    pub files: u64,
    pub dirs: u64,
    pub own_files: u64,
    pub own_bytes: u64,
    pub modified: Option<i64>,
    pub cloud_pending: bool,
    pub unreadable: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FileHit {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub ext: String,
    pub cat: String,
    pub modified: Option<i64>,
    pub cloud: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ExtStat {
    pub ext: String,
    pub cat: String,
    pub files: u64,
    pub bytes: u64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TreeNode {
    pub id: i64,
    pub name: String,
    pub bytes: u64,
    pub files: u64,
    pub kids: Vec<TreeNode>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Issue {
    pub path: String,
    pub message: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub more: bool,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct FilesQuery {
    pub dir: i64,
    pub cat: Option<String>,
    pub exts: Vec<String>,
    pub min: Option<u64>,
    pub max: Option<u64>,
    pub offset: u32,
    pub limit: u32,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct DirsQuery {
    pub dir: i64,
    /// Todas las subcarpetas en lugar de solo las de primer nivel.
    pub tree: bool,
    /// `size`, `name` o `files`.
    pub sort: String,
    pub desc: bool,
    pub min: Option<u64>,
    pub max: Option<u64>,
    pub offset: u32,
    pub limit: u32,
}

struct Writer {
    conn: Connection,
    since: u32,
    issues: u32,
}

impl Writer {
    fn create(path: &Path) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(e2s)?;
        conn.execute_batch(CREATE).map_err(e2s)?;
        Ok(Writer { conn, since: 0, issues: 0 })
    }

    fn tick(&mut self) -> Result<(), String> {
        self.since += 1;
        if self.since >= COMMIT_EVERY {
            self.since = 0;
            self.conn.execute_batch("COMMIT; BEGIN;").map_err(e2s)?;
        }
        Ok(())
    }

    fn finish(self, info: &ReportInfo, exts: &HashMap<String, ExtSlot>, ctl: &Arc<Ctl>) -> Result<(), String> {
        let c = self.conn;
        {
            let mut st = c.prepare("INSERT INTO exts VALUES(?,?,?,?,?)").map_err(e2s)?;
            for (ext, s) in exts {
                st.execute(params![s.id, ext, s.cat, s.files as i64, s.bytes as i64]).map_err(e2s)?;
            }
        }
        c.execute("INSERT OR IGNORE INTO exts VALUES(?,'',11,0,0)", [EXT_NONE]).map_err(e2s)?;
        c.execute("INSERT OR IGNORE INTO exts VALUES(?,'*',11,0,0)", [EXT_MORE]).map_err(e2s)?;
        let json = serde_json::to_string(info).map_err(|e| e.to_string())?;
        c.execute("INSERT INTO meta VALUES('report',?)", [json]).map_err(e2s)?;
        let flag = ctl.clone();
        c.progress_handler(20_000, Some(move || flag.cancel.load(Ordering::Relaxed)));
        c.execute_batch(FINISH).map_err(e2s)?;
        c.execute_batch(&format!("COMMIT; PRAGMA user_version={SCHEMA};")).map_err(e2s)?;
        c.progress_handler(0, None::<fn() -> bool>);
        drop(c);
        Ok(())
    }
}

impl Sink for Writer {
    fn file(&mut self, f: &FileRow) -> Result<(), String> {
        self.conn
            .prepare_cached("INSERT INTO files VALUES(?,?,?,?,?,?,?,?)")
            .and_then(|mut s| s.execute(params![f.id, f.dir, f.name, f.ext, f.cat, f.size as i64, f.modified, f.flags]))
            .map_err(e2s)?;
        self.tick()
    }

    fn dir(&mut self, d: &DirRow) -> Result<(), String> {
        self.conn
            .prepare_cached("INSERT INTO dirs VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .and_then(|mut s| s.execute(params![d.id, d.parent, d.last, d.name, d.first_file, d.end_file, d.own_files as i64, d.own_bytes as i64, d.files as i64, d.dirs as i64, d.bytes as i64, d.cloud_bytes as i64, d.modified, d.flags]))
            .map_err(e2s)?;
        self.tick()
    }

    fn issue(&mut self, path: &str, message: &str) -> Result<(), String> {
        if self.issues < MAX_ISSUES {
            self.issues += 1;
            self.conn.prepare_cached("INSERT INTO issues VALUES(?,?)").and_then(|mut s| s.execute(params![path, message])).map_err(e2s)?;
        }
        Ok(())
    }
}

struct Job {
    root: String,
    ctl: Arc<Ctl>,
}

struct Open {
    key: String,
    conn: Connection,
    info: ReportInfo,
}

pub struct Analyzer {
    dir: PathBuf,
    job: Mutex<Option<Job>>,
    reader: Mutex<Option<Open>>,
}

/// Quien espera un evento de un análisis.
pub type OnEvent = Box<dyn Fn(AnalyzeEvent) + Send + 'static>;

fn open_read(path: &Path) -> rusqlite::Result<Connection> {
    let c = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
    c.execute_batch("PRAGMA query_only=ON; PRAGMA cache_size=-4000; PRAGMA temp_store=MEMORY;")?;
    Ok(c)
}

/// Lee los datos de un informe, o `None` si no es válido para esa ruta o es de otra versión.
fn read_info(c: &Connection, expected_key: Option<&str>) -> Option<ReportInfo> {
    let v: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0)).ok()?;
    if v != SCHEMA {
        return None;
    }
    let json: String = c.query_row("SELECT value FROM meta WHERE key='report'", [], |r| r.get(0)).ok()?;
    let info: ReportInfo = serde_json::from_str(&json).ok()?;
    if expected_key.is_some_and(|k| k != key_of(&info.root) || info.root_key != path_key(&info.root)) {
        return None;
    }
    Some(info)
}

fn location_state(info: &ReportInfo) -> String {
    match fs::symlink_metadata(&info.root) {
        Err(_) => "missing",
        Ok(_) => match (info.volume_serial, volume_serial(&info.root)) {
            (Some(a), Some(b)) if a != b => "otherVolume",
            _ => "ok",
        },
    }
    .to_string()
}

fn dir_path(c: &Connection, root: &str, id: i64, cache: &mut HashMap<i64, String>) -> rusqlite::Result<String> {
    if let Some(p) = cache.get(&id) {
        return Ok(p.clone());
    }
    let mut names = vec![];
    let mut cur = id;
    let mut base = PathBuf::from(root);
    while cur > 1 {
        if let Some(p) = cache.get(&cur) {
            base = PathBuf::from(p);
            break;
        }
        let (parent, name): (i64, String) = c.prepare_cached("SELECT parent,name FROM dirs WHERE id=?")?.query_row([cur], |r| Ok((r.get(0)?, r.get(1)?)))?;
        names.push(name);
        cur = parent;
    }
    for n in names.iter().rev() {
        base.push(n);
    }
    let s = base.to_string_lossy().into_owned();
    cache.insert(id, s.clone());
    Ok(s)
}

const DIR_COLS: &str = "id,parent,name,bytes,cloud_bytes,files,dirs,own_files,own_bytes,mtime,flags";

fn dir_hit(c: &Connection, root: &str, r: &rusqlite::Row, cache: &mut HashMap<i64, String>) -> rusqlite::Result<DirHit> {
    let id: i64 = r.get(0)?;
    let flags: u32 = r.get(10)?;
    Ok(DirHit {
        id,
        parent: r.get(1)?,
        name: r.get(2)?,
        path: dir_path(c, root, id, cache)?,
        bytes: r.get::<_, i64>(3)? as u64,
        cloud_bytes: r.get::<_, i64>(4)? as u64,
        files: r.get::<_, i64>(5)? as u64,
        dirs: r.get::<_, i64>(6)? as u64,
        own_files: r.get::<_, i64>(7)? as u64,
        own_bytes: r.get::<_, i64>(8)? as u64,
        modified: r.get(9)?,
        cloud_pending: flags & DIR_CLOUD_PENDING != 0,
        unreadable: flags & DIR_UNREADABLE != 0,
    })
}

fn page_limit(limit: u32) -> u32 {
    limit.clamp(1, MAX_PAGE)
}

fn cat_name(i: i64) -> String {
    CATEGORIES.get(i as usize).copied().unwrap_or("other").to_string()
}

fn cat_index(name: &str) -> Option<i64> {
    CATEGORIES.iter().position(|c| *c == name).map(|i| i as i64)
}

fn range_of(c: &Connection, dir: i64) -> rusqlite::Result<(i64, i64)> {
    c.prepare_cached("SELECT first_file,end_file FROM dirs WHERE id=?")?.query_row([dir], |r| Ok((r.get(0)?, r.get(1)?)))
}

impl Analyzer {
    /// `dir` es de uso exclusivo del analizador. Lo que dejó a medias una ejecución interrumpida se retira aquí.
    pub fn new(dir: PathBuf) -> Self {
        if let Ok(rd) = fs::read_dir(&dir) {
            for e in rd.flatten() {
                if e.path().extension().is_some_and(|x| x == "tmp") {
                    let _ = fs::remove_file(e.path());
                }
            }
        }
        Analyzer { dir, job: Mutex::new(None), reader: Mutex::new(None) }
    }

    fn db_path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.db"))
    }

    /// Inicia un análisis en su propio hilo, con prioridad baja. Solo hay uno a la vez: nunca se acumulan recorridos.
    pub fn start(self: &Arc<Self>, root: &str, on_event: OnEvent) -> Result<(), String> {
        let root = clean_root(root);
        if !Path::new(&root).is_absolute() {
            return Err("La ruta debe ser absoluta.".into());
        }
        let mut job = self.job.lock().unwrap();
        if let Some(j) = job.as_ref() {
            return Err(format!("Ya hay un análisis en curso de «{}». Espera a que termine o cancélalo.", j.root));
        }
        fs::create_dir_all(&self.dir).map_err(|e| format!("No se pudo crear la carpeta de informes: {e}"))?;
        let ctl = Arc::new(Ctl::default());
        *job = Some(Job { root: root.clone(), ctl: ctl.clone() });
        drop(job);
        let me = self.clone();
        let spawned = std::thread::Builder::new().name("lfind-analyzer".into()).spawn(move || {
            crate::sys::background_thread();
            let tmp = me.dir.join(format!("{}.tmp", key_of(&root)));
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| me.run_job(&root, &tmp, &ctl, &on_event)));
            let _ = fs::remove_file(&tmp);
            *me.job.lock().unwrap() = None;
            on_event(match result {
                Ok(Ok(Some(report))) => AnalyzeEvent::Done { outcome: "completed", message: None, report: Some(report) },
                Ok(Ok(None)) => AnalyzeEvent::Done { outcome: "cancelled", message: None, report: None },
                Ok(Err(m)) if ctl.cancelled() => {
                    let _ = m;
                    AnalyzeEvent::Done { outcome: "cancelled", message: None, report: None }
                }
                Ok(Err(m)) => AnalyzeEvent::Done { outcome: "failed", message: Some(m), report: None },
                Err(_) => AnalyzeEvent::Done { outcome: "failed", message: Some("Error interno del analizador.".into()), report: None },
            });
        });
        if let Err(e) = spawned {
            *self.job.lock().unwrap() = None;
            return Err(format!("No se pudo iniciar el análisis: {e}"));
        }
        Ok(())
    }

    fn run_job(&self, root: &str, tmp: &Path, ctl: &Arc<Ctl>, on_event: &OnEvent) -> Result<Option<ReportInfo>, String> {
        let _ = fs::remove_file(tmp);
        let started_at = now_ms();
        let serial = volume_serial(root);
        let mut writer = Writer::create(tmp)?;
        let opts = Options::for_target(is_drive_root(root));
        let out = walk(Path::new(root), &mut writer, ctl, opts, &mut |p| on_event(AnalyzeEvent::Progress(p.clone())))?;
        if out.cancelled {
            return Ok(None);
        }
        on_event(AnalyzeEvent::Finishing);
        let p = &out.progress;
        let key = key_of(root);
        let mut info = ReportInfo {
            key: key.clone(),
            root: root.to_string(),
            root_key: path_key(root),
            started_at,
            finished_at: now_ms(),
            duration_ms: p.elapsed_ms,
            files: p.files,
            dirs: p.dirs,
            bytes: p.bytes,
            cloud_bytes: p.cloud_bytes,
            cloud_files: p.cloud_files,
            links: p.links,
            errors: p.errors,
            cloud_dirs: p.cloud_dirs,
            volume_serial: serial,
            db_bytes: 0,
            state: "ok".into(),
        };
        writer.finish(&info, &out.exts, ctl)?;
        if ctl.cancelled() {
            return Ok(None);
        }
        let dest = self.db_path(&key);
        // En Windows no se puede sustituir un archivo abierto: la conexión de lectura se cierra antes.
        {
            let mut reader = self.reader.lock().unwrap();
            if reader.as_ref().is_some_and(|o| o.key == key) {
                *reader = None;
            }
            fs::rename(tmp, &dest).map_err(|e| format!("No se pudo guardar el informe: {e}"))?;
        }
        info.db_bytes = fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
        self.prune(&key);
        Ok(Some(info))
    }

    fn prune(&self, keep: &str) {
        let Ok(rd) = fs::read_dir(&self.dir) else { return };
        let mut all: Vec<(SystemTime, PathBuf)> = rd
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "db") && e.path().file_stem().is_some_and(|s| s != keep))
            .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
            .collect();
        if all.len() < MAX_REPORTS {
            return;
        }
        all.sort();
        let extra = all.len() + 1 - MAX_REPORTS;
        let mut reader = self.reader.lock().unwrap();
        *reader = None;
        for (_, p) in all.into_iter().take(extra) {
            let _ = fs::remove_file(p);
        }
    }

    fn with_job(&self, f: impl FnOnce(&Ctl)) {
        if let Some(j) = self.job.lock().unwrap().as_ref() {
            f(&j.ctl);
        }
    }

    pub fn pause(&self) {
        self.with_job(|c| c.pause.store(true, Ordering::Relaxed));
    }

    pub fn resume(&self) {
        self.with_job(|c| c.pause.store(false, Ordering::Relaxed));
    }

    pub fn cancel(&self) {
        self.with_job(|c| {
            c.cancel.store(true, Ordering::Relaxed);
            c.pause.store(false, Ordering::Relaxed);
        });
    }

    /// Los informes guardados, los más recientes primero. No toca las ubicaciones analizadas (una unidad caída no retrasa la lista).
    pub fn reports(&self) -> Vec<ReportInfo> {
        let Ok(rd) = fs::read_dir(&self.dir) else { return vec![] };
        let mut out: Vec<ReportInfo> = rd
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "db"))
            .filter_map(|e| {
                let c = open_read(&e.path()).ok()?;
                let mut i = read_info(&c, None)?;
                i.db_bytes = e.metadata().map(|m| m.len()).unwrap_or(0);
                i.state = "unchecked".into();
                Some(i)
            })
            .collect();
        out.sort_by_key(|r| std::cmp::Reverse(r.finished_at));
        out
    }

    /// El informe de una ubicación, con el estado actual de esa ubicación. `None` si no hay uno válido.
    pub fn report(&self, root: &str) -> Option<ReportInfo> {
        let root = clean_root(root);
        let key = key_of(&root);
        let path = self.db_path(&key);
        let c = open_read(&path).ok()?;
        let mut info = read_info(&c, Some(&key))?;
        info.db_bytes = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        info.state = location_state(&info);
        Some(info)
    }

    /// Retira un informe. Es una acción explícita: solo borra el archivo propio del analizador.
    pub fn forget(&self, root: &str) -> Result<(), String> {
        let root = clean_root(root);
        let key = key_of(&root);
        if self.job.lock().unwrap().as_ref().is_some_and(|j| key_of(&j.root) == key) {
            return Err("Esa ubicación se está analizando.".into());
        }
        let mut reader = self.reader.lock().unwrap();
        if reader.as_ref().is_some_and(|o| o.key == key) {
            *reader = None;
        }
        match fs::remove_file(self.db_path(&key)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(format!("No se pudo quitar el informe: {e}")),
            _ => Ok(()),
        }
    }

    fn with<R>(&self, root: &str, f: impl FnOnce(&Connection, &ReportInfo) -> rusqlite::Result<R>) -> Result<R, String> {
        let root = clean_root(root);
        let key = key_of(&root);
        let mut guard = self.reader.lock().unwrap();
        if guard.as_ref().map(|o| o.key.as_str()) != Some(key.as_str()) {
            *guard = None;
            let conn = open_read(&self.db_path(&key)).map_err(|_| "No hay ningún informe de esta ubicación.".to_string())?;
            let info = read_info(&conn, Some(&key)).ok_or("El informe guardado no es válido. Analiza de nuevo la ubicación.")?;
            *guard = Some(Open { key, conn, info });
        }
        let o = guard.as_ref().expect("recién abierto");
        f(&o.conn, &o.info).map_err(e2s)
    }

    /// Carpeta indicada por su ruta relativa (`a/b`). Cadena vacía: la raíz. `None` si no está en el informe.
    pub fn node(&self, root: &str, at: &str) -> Result<Option<DirHit>, String> {
        self.with(root, |c, info| {
            let mut id = 1i64;
            for part in at.split(['/', '\\']).filter(|p| !p.is_empty()) {
                let next = c.prepare_cached("SELECT id FROM dirs WHERE parent=? AND name=?")?.query_row(params![id, part], |r| r.get::<_, i64>(0));
                match next {
                    Ok(n) => id = n,
                    Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
                    Err(e) => return Err(e),
                }
            }
            let mut cache = HashMap::new();
            let sql = format!("SELECT {DIR_COLS} FROM dirs WHERE id=?");
            let mut st = c.prepare_cached(&sql)?;
            let hit = st.query_row([id], |r| dir_hit(c, &info.root, r, &mut cache))?;
            Ok(Some(hit))
        })
    }

    pub fn dirs(&self, root: &str, q: &DirsQuery) -> Result<Page<DirHit>, String> {
        self.with(root, |c, info| {
            let limit = page_limit(q.limit);
            let mut wh = String::new();
            let mut args: Vec<Value> = vec![];
            if q.tree {
                let last: i64 = c.prepare_cached("SELECT last FROM dirs WHERE id=?")?.query_row([q.dir], |r| r.get(0))?;
                wh.push_str("id>? AND id<=?");
                args.push(q.dir.into());
                args.push(last.into());
            } else {
                wh.push_str("parent=?");
                args.push(q.dir.into());
            }
            if let Some(v) = q.min {
                wh.push_str(" AND bytes>=?");
                args.push((v as i64).into());
            }
            if let Some(v) = q.max {
                wh.push_str(" AND bytes<=?");
                args.push((v as i64).into());
            }
            let col = match q.sort.as_str() {
                "name" => "name COLLATE NOCASE",
                "files" => "files",
                _ => "bytes",
            };
            let dir = if q.desc { "DESC" } else { "ASC" };
            let sql = format!("SELECT {DIR_COLS} FROM dirs WHERE {wh} ORDER BY {col} {dir}, id LIMIT {} OFFSET {}", limit + 1, q.offset);
            let mut st = c.prepare(&sql)?;
            let mut cache = HashMap::new();
            let mut items = st.query_map(params_from_iter(args), |r| dir_hit(c, &info.root, r, &mut cache))?.collect::<rusqlite::Result<Vec<_>>>()?;
            let more = items.len() as u32 > limit;
            items.truncate(limit as usize);
            Ok(Page { items, more })
        })
    }

    pub fn files(&self, root: &str, q: &FilesQuery) -> Result<Page<FileHit>, String> {
        self.with(root, |c, info| {
            let limit = page_limit(q.limit);
            let (first, end) = range_of(c, q.dir)?;
            let mut wh = "f.id>=? AND f.id<?".to_string();
            let mut args: Vec<Value> = vec![first.into(), end.into()];
            if let Some(cat) = q.cat.as_deref().and_then(cat_index) {
                wh.push_str(" AND f.cat=?");
                args.push(cat.into());
            }
            if let Some(v) = q.min {
                wh.push_str(" AND f.size>=?");
                args.push((v as i64).into());
            }
            if let Some(v) = q.max {
                wh.push_str(" AND f.size<=?");
                args.push((v as i64).into());
            }
            if !q.exts.is_empty() {
                let mut ids = vec![];
                for e in &q.exts {
                    if let Ok(id) = c.prepare_cached("SELECT id FROM exts WHERE ext=?")?.query_row([e.to_lowercase()], |r| r.get::<_, i64>(0)) {
                        ids.push(id.to_string());
                    }
                }
                if ids.is_empty() {
                    return Ok(Page { items: vec![], more: false });
                }
                wh.push_str(&format!(" AND f.ext IN ({})", ids.join(",")));
            }
            let (from, order) = if end - first <= SMALL_RANGE { ("files f NOT INDEXED", "f.size DESC, f.id") } else { ("files f INDEXED BY files_size", "f.size DESC") };
            let sql = format!("SELECT f.dir,f.name,e.ext,f.cat,f.size,f.mtime,f.flags FROM {from} CROSS JOIN exts e ON e.id=f.ext WHERE {wh} ORDER BY {order} LIMIT {} OFFSET {}", limit + 1, q.offset);
            let mut st = c.prepare(&sql)?;
            let mut cache = HashMap::new();
            let mut items = st
                .query_map(params_from_iter(args), |r| {
                    let dir: i64 = r.get(0)?;
                    let name: String = r.get(1)?;
                    let ext: String = r.get(2)?;
                    let base = dir_path(c, &info.root, dir, &mut cache)?;
                    Ok(FileHit {
                        path: Path::new(&base).join(&name).to_string_lossy().into_owned(),
                        name,
                        size: r.get::<_, i64>(4)? as u64,
                        ext: if ext == "*" { String::new() } else { ext },
                        cat: cat_name(r.get(3)?),
                        modified: r.get(5)?,
                        cloud: r.get::<_, u32>(6)? & FILE_CLOUD != 0,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let more = items.len() as u32 > limit;
            items.truncate(limit as usize);
            Ok(Page { items, more })
        })
    }

    pub fn categories(&self, root: &str, dir: i64) -> Result<Vec<CatStat>, String> {
        self.with(root, |c, _| {
            let map = |r: &rusqlite::Row| Ok(CatStat { cat: cat_name(r.get(0)?), files: r.get::<_, i64>(1)? as u64, bytes: r.get::<_, i64>(2)? as u64 });
            if dir == 1 {
                let mut st = c.prepare("SELECT cat,SUM(files) f,SUM(bytes) b FROM exts GROUP BY cat HAVING f>0 ORDER BY b DESC")?;
                let out = st.query_map([], map)?.collect();
                return out;
            }
            let (first, end) = range_of(c, dir)?;
            let mut st = c.prepare("SELECT cat,COUNT(*),SUM(size) b FROM files WHERE id>=? AND id<? GROUP BY cat ORDER BY b DESC")?;
            let out = st.query_map([first, end], map)?.collect();
            out
        })
    }

    pub fn exts(&self, root: &str, dir: i64, offset: u32, limit: u32) -> Result<Page<ExtStat>, String> {
        self.with(root, |c, _| {
            let limit = page_limit(limit);
            let map = |r: &rusqlite::Row| {
                let ext: String = r.get(0)?;
                Ok(ExtStat { ext, cat: cat_name(r.get(1)?), files: r.get::<_, i64>(2)? as u64, bytes: r.get::<_, i64>(3)? as u64 })
            };
            let mut items: Vec<ExtStat> = if dir == 1 {
                let mut st = c.prepare("SELECT ext,cat,files,bytes FROM exts WHERE files>0 ORDER BY bytes DESC, ext LIMIT ? OFFSET ?")?;
                let v = st.query_map(params![limit + 1, offset], map)?.collect::<rusqlite::Result<_>>()?;
                v
            } else {
                let (first, end) = range_of(c, dir)?;
                let mut st = c.prepare("SELECT e.ext,e.cat,x.n,x.b FROM (SELECT ext id,COUNT(*) n,SUM(size) b FROM files WHERE id>=? AND id<? GROUP BY ext) x JOIN exts e ON e.id=x.id ORDER BY x.b DESC, e.ext LIMIT ? OFFSET ?")?;
                let v = st.query_map(params![first, end, limit + 1, offset], map)?.collect::<rusqlite::Result<_>>()?;
                v
            };
            let more = items.len() as u32 > limit;
            items.truncate(limit as usize);
            Ok(Page { items, more })
        })
    }

    /// Las carpetas mayores de `dir` y, de las primeras, sus propias subcarpetas mayores.
    pub fn treemap(&self, root: &str, dir: i64) -> Result<Vec<TreeNode>, String> {
        self.with(root, |c, _| {
            let kids = |id: i64, n: u32| -> rusqlite::Result<Vec<TreeNode>> {
                let mut st = c.prepare_cached("SELECT id,name,bytes,files FROM dirs WHERE parent=? AND bytes>0 ORDER BY bytes DESC LIMIT ?")?;
                let v = st.query_map(params![id, n], |r| Ok(TreeNode { id: r.get(0)?, name: r.get(1)?, bytes: r.get::<_, i64>(2)? as u64, files: r.get::<_, i64>(3)? as u64, kids: vec![] }))?.collect();
                v
            };
            let mut top = kids(dir, 40)?;
            for t in top.iter_mut().take(12) {
                t.kids = kids(t.id, 10)?;
            }
            Ok(top)
        })
    }

    pub fn issues(&self, root: &str) -> Result<Vec<Issue>, String> {
        self.with(root, |c, _| {
            let mut st = c.prepare("SELECT path,message FROM issues LIMIT ?")?;
            let v = st.query_map([MAX_ISSUES], |r| Ok(Issue { path: r.get(0)?, message: r.get(1)? }))?.collect();
            v
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;
    use std::sync::mpsc;
    use std::time::Duration;

    static NEXT: AtomicU32 = AtomicU32::new(0);
    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("lfind-report-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir_all(&d).unwrap();
        d
    }
    fn put(p: &Path, n: usize) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, vec![0u8; n]).unwrap();
    }
    fn tree() -> PathBuf {
        let r = tmp();
        put(&r.join("a.png"), 100);
        put(&r.join("x/b.txt"), 10);
        put(&r.join("x/y/c.txt"), 20);
        put(&r.join("x/y/d"), 5);
        put(&r.join("z/e.zip"), 1000);
        r
    }
    fn run(a: &Arc<Analyzer>, root: &Path) -> AnalyzeEvent {
        let (tx, rx) = mpsc::channel();
        a.start(root.to_str().unwrap(), Box::new(move |e| {
            if matches!(e, AnalyzeEvent::Done { .. }) {
                let _ = tx.send(e);
            }
        }))
        .unwrap();
        rx.recv_timeout(Duration::from_secs(30)).unwrap()
    }
    fn fixture() -> (Arc<Analyzer>, PathBuf, String) {
        let r = tree();
        let a = Arc::new(Analyzer::new(tmp()));
        assert!(matches!(run(&a, &r), AnalyzeEvent::Done { outcome: "completed", .. }));
        let root = r.to_str().unwrap().to_string();
        (a, r, root)
    }

    #[test]
    fn report_totals_and_navigation() {
        let (a, r, root) = fixture();
        let info = a.report(&root).unwrap();
        assert_eq!((info.files, info.dirs, info.bytes, info.state.as_str()), (5, 3, 1135, "ok"));
        let top = a.node(&root, "").unwrap().unwrap();
        assert_eq!(top.bytes, 1135);
        let y = a.node(&root, "x/y").unwrap().unwrap();
        assert_eq!((y.bytes, y.files), (25, 2));
        assert_eq!(Path::new(&y.path), r.join("x").join("y"));
        assert!(a.node(&root, "x/no").unwrap().is_none());
        fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn same_location_written_differently_is_the_same_report() {
        let (a, r, root) = fixture();
        let upper = format!("{}{}", root.to_uppercase(), std::path::MAIN_SEPARATOR);
        if cfg!(windows) {
            assert_eq!(a.report(&upper).unwrap().key, a.report(&root).unwrap().key);
        }
        assert!(a.report(r.join("x").to_str().unwrap()).is_none(), "otra ubicación no hereda el informe");
        fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn folder_listing_sort_and_paging() {
        let (a, r, root) = fixture();
        let q = |sort: &str, desc, tree| DirsQuery { dir: 1, tree, sort: sort.into(), desc, limit: 50, ..Default::default() };
        let names = |p: Page<DirHit>| p.items.into_iter().map(|d| d.name).collect::<Vec<_>>();
        assert_eq!(names(a.dirs(&root, &q("size", true, false)).unwrap()), ["z", "x"]);
        assert_eq!(names(a.dirs(&root, &q("name", false, false)).unwrap()), ["x", "z"]);
        assert_eq!(names(a.dirs(&root, &q("size", true, true)).unwrap()), ["z", "x", "y"]);
        assert_eq!(names(a.dirs(&root, &q("files", true, true)).unwrap())[0], "x");
        let first = a.dirs(&root, &DirsQuery { limit: 1, dir: 1, tree: true, sort: "size".into(), desc: true, ..Default::default() }).unwrap();
        assert!(first.more && first.items.len() == 1);
        let f = a.dirs(&root, &DirsQuery { dir: 1, min: Some(100), sort: "size".into(), desc: true, limit: 9, ..Default::default() }).unwrap();
        assert_eq!(names(f), ["z"]);
        fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn file_filters_and_paging() {
        let (a, r, root) = fixture();
        let names = |q: FilesQuery| a.files(&root, &FilesQuery { limit: 50, ..q }).unwrap().items.into_iter().map(|f| f.name).collect::<Vec<_>>();
        assert_eq!(names(FilesQuery { dir: 1, ..Default::default() }), ["e.zip", "a.png", "c.txt", "b.txt", "d"]);
        assert_eq!(names(FilesQuery { dir: 1, cat: Some("doc".into()), ..Default::default() }), ["c.txt", "b.txt"]);
        assert_eq!(names(FilesQuery { dir: 1, exts: vec!["PNG".into(), "zip".into()], ..Default::default() }), ["e.zip", "a.png"]);
        assert_eq!(names(FilesQuery { dir: 1, min: Some(10), max: Some(100), ..Default::default() }), ["a.png", "c.txt", "b.txt"]);
        assert!(names(FilesQuery { dir: 1, exts: vec!["nada".into()], ..Default::default() }).is_empty());
        let x = a.node(&root, "x").unwrap().unwrap();
        assert_eq!(names(FilesQuery { dir: x.id, ..Default::default() }), ["c.txt", "b.txt", "d"]);
        let p1 = a.files(&root, &FilesQuery { dir: 1, limit: 2, ..Default::default() }).unwrap();
        let p2 = a.files(&root, &FilesQuery { dir: 1, limit: 2, offset: 2, ..Default::default() }).unwrap();
        assert!(p1.more && p2.more);
        assert_eq!(p2.items[0].name, "c.txt");
        assert!(p1.items[0].path.ends_with("e.zip"));
        fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn large_range_uses_the_size_index_and_gives_the_same_order() {
        let (a, r, root) = fixture();
        // Consulta directa con la variante del índice: debe poder prepararse y ordenar igual.
        a.with(&root, |c, _| {
            let v: Vec<String> = c.prepare("SELECT f.name FROM files f INDEXED BY files_size CROSS JOIN exts e ON e.id=f.ext WHERE f.id>=1 AND f.id<100 ORDER BY f.size DESC LIMIT 2")?.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
            assert_eq!(v, ["e.zip", "a.png"]);
            Ok(())
        })
        .unwrap();
        fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn distribution_and_treemap() {
        let (a, r, root) = fixture();
        let c = a.categories(&root, 1).unwrap();
        assert_eq!(c[0], CatStat { cat: "archive".into(), files: 1, bytes: 1000 });
        let x = a.node(&root, "x").unwrap().unwrap();
        let cx = a.categories(&root, x.id).unwrap();
        assert_eq!(cx[0], CatStat { cat: "doc".into(), files: 2, bytes: 30 });
        let e = a.exts(&root, 1, 0, 10).unwrap();
        assert_eq!((e.items[0].ext.as_str(), e.items[0].bytes), ("zip", 1000));
        assert_eq!(a.exts(&root, x.id, 0, 10).unwrap().items[0].ext, "txt");
        let t = a.treemap(&root, 1).unwrap();
        assert_eq!(t[0].name, "z");
        assert_eq!(t[1].kids[0].name, "y");
        fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn missing_location_is_flagged_and_report_survives() {
        let r = tree();
        let a = Arc::new(Analyzer::new(tmp()));
        run(&a, &r);
        let root = r.to_str().unwrap().to_string();
        fs::remove_dir_all(&r).unwrap();
        assert_eq!(a.report(&root).unwrap().state, "missing");
        assert_eq!(a.reports().len(), 1);
    }

    #[test]
    fn rescan_replaces_and_cancel_keeps_the_previous_report() {
        let (a, r, root) = fixture();
        a.node(&root, "").unwrap(); // deja abierta la conexión de lectura: no debe impedir sustituir el informe
        put(&r.join("nuevo.bin"), 7);
        assert!(matches!(run(&a, &r), AnalyzeEvent::Done { outcome: "completed", .. }));
        assert_eq!(a.report(&root).unwrap().bytes, 1142);

        let (tx, rx) = mpsc::channel();
        let flag = a.clone();
        a.start(&root, Box::new(move |e| {
            if matches!(e, AnalyzeEvent::Done { .. }) {
                let _ = tx.send(e);
            }
        }))
        .unwrap();
        flag.cancel();
        let done = rx.recv_timeout(Duration::from_secs(30)).unwrap();
        // Con tan pocos archivos puede terminar antes de que llegue la cancelación: en ambos casos el informe es válido.
        if matches!(done, AnalyzeEvent::Done { outcome: "cancelled", .. }) {
            assert_eq!(a.report(&root).unwrap().bytes, 1142, "cancelar conserva el informe anterior");
        }
        assert!(!fs::read_dir(&a.dir).unwrap().flatten().any(|e| e.path().extension().is_some_and(|x| x == "tmp")), "no quedan temporales");
        fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn only_one_job_at_a_time() {
        let r = tree();
        let a = Arc::new(Analyzer::new(tmp()));
        // Un trabajo registrado impide iniciar otro, sin depender de la velocidad del disco.
        *a.job.lock().unwrap() = Some(Job { root: "C:\\x".into(), ctl: Arc::new(Ctl::default()) });
        let err = a.start(r.to_str().unwrap(), Box::new(|_| {})).unwrap_err();
        assert!(err.contains("en curso"));
        *a.job.lock().unwrap() = None;
        fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn failed_start_reports_and_leaves_nothing() {
        let r = tmp();
        let a = Arc::new(Analyzer::new(tmp()));
        let done = run(&a, &r.join("no-existe"));
        assert!(matches!(done, AnalyzeEvent::Done { outcome: "failed", .. }));
        assert_eq!(fs::read_dir(&a.dir).unwrap().count(), 0);
    }

    #[test]
    fn invalid_or_foreign_reports_are_ignored() {
        let (a, r, root) = fixture();
        let key = key_of(&root);
        let foreign = a.db_path(&key_of("C:\\otra"));
        fs::copy(a.db_path(&key), &foreign).unwrap();
        assert!(a.report("C:\\otra").is_none(), "un informe de otra ruta no se acepta");
        fs::write(a.db_path(&key_of("C:\\rota")), b"no es una base de datos").unwrap();
        assert!(a.report("C:\\rota").is_none());
        fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn forget_removes_only_that_report() {
        let (a, r, root) = fixture();
        a.node(&root, "").unwrap();
        a.forget(&root).unwrap();
        assert!(a.report(&root).is_none());
        assert!(r.join("a.png").exists(), "los archivos analizados no se tocan");
        fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn leftovers_of_an_interrupted_run_are_swept() {
        let d = tmp();
        fs::write(d.join("abc.tmp"), b"x").unwrap();
        fs::write(d.join("abc.db"), b"x").unwrap();
        Analyzer::new(d.clone());
        assert!(!d.join("abc.tmp").exists() && d.join("abc.db").exists());
    }

    #[test]
    fn roots() {
        assert!(is_drive_root("C:\\") && is_drive_root("C:") && !is_drive_root("C:\\a"));
        assert_eq!(key_of("C:\\Users\\A\\"), key_of("c:/users/a"));
        assert_ne!(key_of("C:\\a"), key_of("C:\\b"));
    }
}
