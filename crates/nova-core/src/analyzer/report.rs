//! Informes del analizador: una base de datos por ubicación, en una carpeta propia.
//!
//! El análisis escribe en un archivo temporal y solo sustituye al informe anterior cuando termina bien: cancelar o fallar deja intacto
//! el último informe válido. Los archivos se guardan en orden de recorrido, de modo que el contenido de una carpeta es un intervalo de
//! identificadores: las consultas no necesitan un índice por carpeta y la interfaz solo recibe páginas.

use super::walk::*;
use super::watch::{Changes, Dirty, Plan, Watcher};
use crate::index::classify::path_key;
use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const SCHEMA: i64 = 1;
const COMMIT_EVERY: u32 = 20_000;
const MAX_ISSUES: u32 = 500;
/// Informes conservados; al superarse se retira el analizado hace más tiempo.
const MAX_REPORTS: usize = 24;
/// Por debajo de estos archivos es más barato ordenar el intervalo que recorrer el índice por tamaño.
const SMALL_RANGE: i64 = 50_000;
const MAX_PAGE: u32 = 500;
/// Filas copiadas de una vez al actualizar un informe; entre bloques se cede el procesador y se atiende la cancelación.
const COPY_CHUNK: i64 = 50_000;
const VERIFY_PACE: u32 = 512;
/// Sin actividad durante este tiempo se considera que los cambios han terminado.
const WATCH_QUIET: Duration = Duration::from_secs(60);
/// Si la actividad no cesa, lo anotado no espera más que esto.
const WATCH_MAX_WAIT: Duration = Duration::from_secs(5 * 60);
/// Cada cuánto se comparan las fechas aunque no haya avisos: cubre lo que el sistema no notifique.
const WATCH_RECONCILE_FOLDER: Duration = Duration::from_secs(60 * 60);
const WATCH_RECONCILE_DRIVE: Duration = Duration::from_secs(6 * 60 * 60);
/// Cada cuánto se intenta recuperar una vigilancia caída.
const WATCH_RETRY: Duration = Duration::from_secs(5 * 60);
/// Tope de entradas al comprobar si lo avisado cambió de verdad; pasado el tope se relee sin comparar.
const VERIFY_MAX_ENTRIES: usize = 400_000;
const VERIFY_MAX_FILES_IN_DIR: u32 = 200_000;
const WATCH_TICK: Duration = Duration::from_secs(5);
/// Espera mínima entre actualizaciones automáticas: en una unidad completa siempre hay actividad.
const WATCH_MIN_FOLDER: Duration = Duration::from_secs(5 * 60);
const WATCH_MIN_DRIVE: Duration = Duration::from_secs(30 * 60);
/// Al empezar a vigilar, si el informe es más antiguo, se comprueba lo cambiado mientras LFind estaba cerrado.
const WATCH_VERIFY_AFTER_MS: i64 = 15 * 60 * 1000;
const WATCH_FIRST_CHECK: Duration = Duration::from_secs(20);

const CREATE: &str = "
PRAGMA journal_mode=OFF; PRAGMA synchronous=OFF; PRAGMA locking_mode=EXCLUSIVE; PRAGMA temp_store=MEMORY; PRAGMA cache_size=-8000;
CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL) WITHOUT ROWID;
CREATE TABLE exts(id INTEGER PRIMARY KEY, ext TEXT NOT NULL, cat INTEGER NOT NULL, files INTEGER NOT NULL, bytes INTEGER NOT NULL);
CREATE TABLE issues(path TEXT NOT NULL, message TEXT NOT NULL);
CREATE TABLE dirs(id INTEGER PRIMARY KEY, parent INTEGER NOT NULL, last INTEGER NOT NULL, name TEXT NOT NULL, first_file INTEGER NOT NULL, end_file INTEGER NOT NULL,
  own_files INTEGER NOT NULL, own_bytes INTEGER NOT NULL, files INTEGER NOT NULL, dirs INTEGER NOT NULL, bytes INTEGER NOT NULL, cloud_bytes INTEGER NOT NULL, mtime INTEGER, flags INTEGER NOT NULL);
CREATE TABLE files(id INTEGER PRIMARY KEY, dir INTEGER NOT NULL, name TEXT NOT NULL, ext INTEGER NOT NULL, cat INTEGER NOT NULL, size INTEGER NOT NULL, mtime INTEGER, flags INTEGER NOT NULL);";

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
    /// Última vez que se comprobaron cambios, aunque no hubiera ninguno.
    #[serde(default)]
    pub checked_at: i64,
    /// Fecha del último recorrido completo. Se conserva al actualizar por partes.
    #[serde(default)]
    pub full_scan_at: i64,
    /// Carpetas releídas en la última actualización parcial. 0: el informe viene de un recorrido completo.
    #[serde(default)]
    pub updated_dirs: u64,
    /// Puede faltar algún cambio: un archivo modificado sin que cambie la fecha de su carpeta solo se ve con la vigilancia o un análisis completo.
    #[serde(default)]
    pub partial: bool,
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
    /// Al actualizar: carpetas del informe anterior que hay que releer, ordenadas.
    dirty: Option<Vec<i64>>,
    ctl: Option<Arc<Ctl>>,
    rescanned: u64,
    /// Archivos avisados por la vigilancia, para leerlos directamente.
    fresh: HashSet<String>,
}

impl Writer {
    fn create(path: &Path) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(e2s)?;
        conn.execute_batch(CREATE).map_err(e2s)?;
        conn.execute_batch("BEGIN;").map_err(e2s)?;
        Ok(Writer { conn, since: 0, issues: 0, dirty: None, ctl: None, rescanned: 0, fresh: HashSet::new() })
    }

    /// Prepara un informe que parte de otro. `ATTACH` no se admite dentro de una transacción.
    fn update_from(path: &Path, old: &Path, dirty: Vec<i64>, ctl: Arc<Ctl>) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(e2s)?;
        conn.execute_batch(CREATE).map_err(e2s)?;
        conn.execute("ATTACH DATABASE ? AS o", [old.to_string_lossy()]).map_err(e2s)?;
        conn.execute_batch("BEGIN; INSERT INTO exts SELECT id,ext,cat,0,0 FROM o.exts; INSERT INTO issues SELECT path,message FROM o.issues;").map_err(e2s)?;
        let issues = conn.query_row("SELECT COUNT(*) FROM issues", [], |r| r.get::<_, u32>(0)).map_err(e2s)?;
        Ok(Writer { conn, since: 0, issues, dirty: Some(dirty), ctl: Some(ctl), rescanned: 0, fresh: HashSet::new() })
    }

    /// Inserta en bloques: ni una copia grande retiene el disco de golpe ni impide cancelar.
    fn copy_files(&self, first: i64, end: i64, sql: &str, args: (i64, i64)) -> Result<(), String> {
        let mut from = first;
        let mut st = self.conn.prepare_cached(sql).map_err(e2s)?;
        while from < end {
            if self.ctl.as_ref().is_some_and(|c| c.cancelled()) {
                return Err("cancelado".into());
            }
            let to = (from + COPY_CHUNK).min(end);
            st.execute(params![args.0, args.1, from, to]).map_err(e2s)?;
            from = to;
            std::thread::sleep(Duration::from_millis(1));
        }
        Ok(())
    }

    fn tick(&mut self) -> Result<(), String> {
        self.since += 1;
        if self.since >= COMMIT_EVERY {
            self.since = 0;
            self.conn.execute_batch("COMMIT; BEGIN;").map_err(e2s)?;
        }
        Ok(())
    }

    fn finish(self, info: &mut ReportInfo, exts: &HashMap<String, ExtSlot>, ctl: &Arc<Ctl>) -> Result<(), String> {
        let updating = self.dirty.is_some();
        let c = self.conn;
        if updating {
            c.flush_prepared_statement_cache();
            c.execute_batch("COMMIT; DETACH DATABASE o; BEGIN;").map_err(e2s)?;
        }
        {
            let mut st = c.prepare("INSERT OR REPLACE INTO exts VALUES(?,?,?,?,?)").map_err(e2s)?;
            for (ext, s) in exts {
                st.execute(params![s.id, ext, s.cat, s.files as i64, s.bytes as i64]).map_err(e2s)?;
            }
        }
        c.execute("INSERT OR IGNORE INTO exts VALUES(?,'',11,0,0)", [EXT_NONE]).map_err(e2s)?;
        c.execute("INSERT OR IGNORE INTO exts VALUES(?,'*',11,0,0)", [EXT_MORE]).map_err(e2s)?;
        if updating {
            // Las estadísticas se recalculan sobre lo que ha quedado: copiado y releído.
            c.execute_batch(
                "UPDATE exts SET files=0,bytes=0;
                 CREATE TEMP TABLE t(id INTEGER PRIMARY KEY,n INTEGER,b INTEGER);
                 INSERT INTO t SELECT ext,COUNT(*),SUM(size) FROM files GROUP BY ext;
                 UPDATE exts SET files=(SELECT n FROM t WHERE t.id=exts.id),bytes=(SELECT b FROM t WHERE t.id=exts.id) WHERE id IN (SELECT id FROM t);
                 DROP TABLE t;",
            )
            .map_err(e2s)?;
            let (n, b): (i64, Option<i64>) = c.query_row("SELECT COUNT(*),SUM(size) FROM files WHERE flags&1", [], |r| Ok((r.get(0)?, r.get(1)?))).map_err(e2s)?;
            info.cloud_files = n as u64;
            info.cloud_bytes = b.unwrap_or(0) as u64;
            let (bad, pending): (i64, i64) = c.query_row("SELECT COALESCE(SUM(flags&2>0),0),COALESCE(SUM(flags&1>0),0) FROM dirs", [], |r| Ok((r.get(0)?, r.get(1)?))).map_err(e2s)?;
            info.errors = bad as u64;
            info.cloud_dirs = pending as u64;
        }
        let json = serde_json::to_string(info).map_err(|e| e.to_string())?;
        c.execute("INSERT INTO meta VALUES('report',?)", [json]).map_err(e2s)?;
        let flag = ctl.clone();
        c.progress_handler(20_000, Some(move || flag.cancel.load(Ordering::Relaxed)));
        let _ = &info;
        c.execute_batch(FINISH).map_err(e2s)?;
        c.execute_batch(&format!("COMMIT; PRAGMA user_version={SCHEMA};")).map_err(e2s)?;
        c.progress_handler(0, None::<fn() -> bool>);
        drop(c);
        Ok(())
    }
}

fn fresh_key(p: &Path) -> String {
    p.to_string_lossy().replace('/', "\\").to_lowercase()
}

/// De dónde sale la lista de carpetas que hay que releer.
enum Source {
    /// Comparación de fechas. `covered`: la vigilancia no ha perdido avisos, así que no falta nada y el informe no pasa a parcial.
    Dates { covered: bool },
    /// Carpetas indicadas por quien llama.
    Listed(Vec<String>),
    /// Avisos de la vigilancia: se comprueba antes si cambió algo de verdad.
    Watched(Changes),
}

fn old_dir_row(c: &Connection, id: i64) -> rusqlite::Result<DirRow> {
    c.prepare_cached("SELECT id,parent,last,name,first_file,end_file,own_files,own_bytes,files,dirs,bytes,cloud_bytes,mtime,flags FROM o.dirs WHERE id=?")?.query_row([id], |r| {
        Ok(DirRow {
            id: r.get(0)?,
            parent: r.get(1)?,
            last: r.get(2)?,
            name: r.get(3)?,
            first_file: r.get(4)?,
            end_file: r.get(5)?,
            own_files: r.get::<_, i64>(6)? as u64,
            own_bytes: r.get::<_, i64>(7)? as u64,
            files: r.get::<_, i64>(8)? as u64,
            dirs: r.get::<_, i64>(9)? as u64,
            bytes: r.get::<_, i64>(10)? as u64,
            cloud_bytes: r.get::<_, i64>(11)? as u64,
            modified: r.get(12)?,
            flags: r.get(13)?,
        })
    })
}

impl Sink for Writer {
    fn fresh(&self, dir: &Path, name: &std::ffi::OsStr) -> bool {
        !self.fresh.is_empty() && self.fresh.contains(&fresh_key(&dir.join(name)))
    }

    fn visit(&mut self, old: i64) -> Result<Visit, String> {
        let dirty = self.dirty.as_ref().ok_or("sin informe anterior")?;
        if dirty.binary_search(&old).is_ok() {
            self.rescanned += 1;
            return Ok(Visit::Rescan);
        }
        let last: i64 = self.conn.prepare_cached("SELECT last FROM o.dirs WHERE id=?").and_then(|mut s| s.query_row([old], |r| r.get(0))).map_err(e2s)?;
        let at = dirty.partition_point(|d| *d <= old);
        Ok(if dirty.get(at).is_some_and(|d| *d <= last) { Visit::Keep } else { Visit::Copy })
    }

    fn old_child(&mut self, old: i64, name: &str) -> Result<Option<i64>, String> {
        match self.conn.prepare_cached("SELECT id FROM o.dirs WHERE parent=? AND name=?").and_then(|mut s| s.query_row(params![old, name], |r| r.get(0))) {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e2s(e)),
        }
    }

    fn old_children(&mut self, old: i64) -> Result<Vec<OldChild>, String> {
        let mut st = self.conn.prepare_cached("SELECT id,name,mtime,flags FROM o.dirs WHERE parent=?").map_err(e2s)?;
        let rows = st.query_map([old], |r| Ok(OldChild { id: r.get(0)?, name: r.get(1)?, modified: r.get(2)?, flags: r.get(3)? })).map_err(e2s)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(e2s)
    }

    fn old_row(&mut self, old: i64) -> Result<DirRow, String> {
        old_dir_row(&self.conn, old).map_err(e2s)
    }

    fn keep_files(&mut self, o: &DirRow, new_dir: i64, first_file: i64) -> Result<u64, String> {
        let end = o.first_file + o.own_files as i64;
        self.copy_files(o.first_file, end, "INSERT INTO files SELECT id+?1,?2,name,ext,cat,size,mtime,flags FROM o.files WHERE id>=?3 AND id<?4", (first_file - o.first_file, new_dir))?;
        let cloud: Option<i64> = self.conn.prepare_cached("SELECT SUM(size) FROM o.files WHERE id>=? AND id<? AND flags&1").and_then(|mut s| s.query_row([o.first_file, end], |r| r.get(0))).map_err(e2s)?;
        Ok(cloud.unwrap_or(0) as u64)
    }

    fn copy_subtree(&mut self, old: i64, new_parent: i64, new_id: i64, first_file: i64) -> Result<DirRow, String> {
        let o = old_dir_row(&self.conn, old).map_err(e2s)?;
        let (d, f) = (new_id - o.id, first_file - o.first_file);
        self.conn
            .prepare_cached("INSERT INTO dirs SELECT id+?1,CASE WHEN id=?2 THEN ?3 ELSE parent+?1 END,last+?1,name,first_file+?4,end_file+?4,own_files,own_bytes,files,dirs,bytes,cloud_bytes,mtime,flags FROM o.dirs WHERE id>=?2 AND id<=?5")
            .and_then(|mut s| s.execute(params![d, o.id, new_parent, f, o.last]))
            .map_err(e2s)?;
        self.copy_files(o.first_file, o.end_file, "INSERT INTO files SELECT id+?1,dir+?2,name,ext,cat,size,mtime,flags FROM o.files WHERE id>=?3 AND id<?4", (f, d))?;
        Ok(o)
    }

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

struct Watching {
    root: String,
    stop: Arc<AtomicBool>,
    _watcher: Watcher,
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
    watching: Mutex<Option<Watching>>,
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
        Analyzer { dir, job: Mutex::new(None), reader: Mutex::new(None), watching: Mutex::new(None) }
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
            checked_at: 0,
            full_scan_at: 0,
            updated_dirs: 0,
            partial: false,
        };
        info.checked_at = info.finished_at;
        info.full_scan_at = info.finished_at;
        writer.finish(&mut info, &out.exts, ctl)?;
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

    /// Actualiza un informe releyendo solo lo que cambió. `changed`: carpetas conocidas por la vigilancia (rutas relativas con `/`);
    /// `None`: se comparan las fechas de todas las carpetas guardadas, lo que no enumera ningún archivo.
    pub fn update(self: &Arc<Self>, root: &str, changed: Option<Vec<String>>, on_event: OnEvent) -> Result<(), String> {
        self.start_update(root, changed.map_or(Source::Dates { covered: false }, Source::Listed), on_event)
    }

    fn start_update(self: &Arc<Self>, root: &str, source: Source, on_event: OnEvent) -> Result<(), String> {
        let root = clean_root(root);
        let key = key_of(&root);
        if !self.db_path(&key).exists() {
            return Err("No hay ningún informe de esta ubicación.".into());
        }
        let mut job = self.job.lock().unwrap();
        if let Some(j) = job.as_ref() {
            return Err(format!("Ya hay un análisis en curso de «{}». Espera a que termine o cancélalo.", j.root));
        }
        let ctl = Arc::new(Ctl::default());
        *job = Some(Job { root: root.clone(), ctl: ctl.clone() });
        drop(job);
        let me = self.clone();
        let spawned = std::thread::Builder::new().name("lfind-analyzer-update".into()).spawn(move || {
            crate::sys::background_thread();
            let tmp = me.dir.join(format!("{key}.tmp"));
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| me.run_update(&root, &key, source, &tmp, &ctl)));
            let _ = fs::remove_file(&tmp);
            *me.job.lock().unwrap() = None;
            on_event(match result {
                Ok(Ok((outcome, report))) => AnalyzeEvent::Done { outcome, message: None, report },
                Ok(Err(_)) if ctl.cancelled() => AnalyzeEvent::Done { outcome: "cancelled", message: None, report: None },
                Ok(Err(m)) => AnalyzeEvent::Done { outcome: "failed", message: Some(m), report: None },
                Err(_) => AnalyzeEvent::Done { outcome: "failed", message: Some("Error interno del analizador.".into()), report: None },
            });
        });
        if let Err(e) = spawned {
            *self.job.lock().unwrap() = None;
            return Err(format!("No se pudo iniciar la actualización: {e}"));
        }
        Ok(())
    }

    /// Carpetas cuya fecha ya no coincide con la del informe. Una carpeta desaparecida marca a su madre.
    fn changed_since(c: &Connection, root: &str, ctl: &Ctl) -> Result<Vec<i64>, String> {
        let mut st = c.prepare("SELECT id,parent,name,mtime FROM dirs ORDER BY id").map_err(e2s)?;
        let mut rows = st.query([]).map_err(e2s)?;
        let mut path: Vec<(i64, PathBuf)> = vec![];
        let mut out = vec![];
        let mut n = 0u32;
        while let Some(r) = rows.next().map_err(e2s)? {
            let (id, parent): (i64, i64) = (r.get(0).map_err(e2s)?, r.get(1).map_err(e2s)?);
            let name: String = r.get(2).map_err(e2s)?;
            let mtime: Option<i64> = r.get(3).map_err(e2s)?;
            while path.last().is_some_and(|(p, _)| *p != parent) {
                path.pop();
            }
            let here = path.last().map_or_else(|| PathBuf::from(root), |(_, p)| p.join(&name));
            n += 1;
            if n % VERIFY_PACE == 0 {
                if ctl.cancelled() {
                    return Err("cancelado".into());
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            match fs::symlink_metadata(&here) {
                Ok(m) => {
                    let now = m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64);
                    if now != mtime && now.is_some() {
                        out.push(id);
                    }
                }
                Err(_) => {
                    if parent > 0 {
                        out.push(parent);
                    }
                }
            }
            path.push((id, here));
        }
        out.sort_unstable();
        out.dedup();
        Ok(out)
    }

    /// Identificador de cada carpeta indicada; si ya no está en el informe, el de la más cercana que sí esté.
    fn resolve_dirs(c: &Connection, changed: &[String]) -> Result<Vec<i64>, String> {
        let mut st = c.prepare("SELECT id FROM dirs WHERE parent=? AND name=?").map_err(e2s)?;
        let mut out = vec![];
        for rel in changed {
            let mut id = 1i64;
            for part in rel.split('/').filter(|p| !p.is_empty()) {
                match st.query_row(params![id, part], |r| r.get::<_, i64>(0)) {
                    Ok(n) => id = n,
                    Err(rusqlite::Error::QueryReturnedNoRows) => break,
                    Err(e) => return Err(e2s(e)),
                }
            }
            out.push(id);
        }
        out.sort_unstable();
        out.dedup();
        Ok(out)
    }

    fn dir_path(c: &Connection, root: &str, mut id: i64) -> Result<Option<PathBuf>, String> {
        let mut st = c.prepare_cached("SELECT parent,name FROM dirs WHERE id=?").map_err(e2s)?;
        let mut parts = vec![];
        loop {
            match st.query_row([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))) {
                Ok((0, _)) => break,
                Ok((parent, name)) => {
                    parts.push(name);
                    id = parent;
                }
                Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
                Err(e) => return Err(e2s(e)),
            }
        }
        let mut out = PathBuf::from(root);
        out.extend(parts.iter().rev());
        Ok(Some(out))
    }

    /// ¿El contenido directo de la carpeta difiere del informe? No baja a las subcarpetas.
    fn dir_differs(c: &Connection, root: &str, id: i64, fresh: &HashSet<String>, seen: &mut usize) -> Result<bool, String> {
        let Some(path) = Self::dir_path(c, root, id)? else { return Ok(true) };
        let (first, own): (i64, u32) = c.prepare_cached("SELECT first_file,own_files FROM dirs WHERE id=?").and_then(|mut s| s.query_row([id], |r| Ok((r.get(0)?, r.get(1)?)))).map_err(e2s)?;
        if own > VERIFY_MAX_FILES_IN_DIR {
            return Ok(true);
        }
        let Ok(listing) = fs::read_dir(&path) else { return Ok(true) };
        let mut files: HashMap<String, (i64, Option<i64>)> = HashMap::with_capacity(own as usize);
        {
            let mut st = c.prepare_cached("SELECT name,size,mtime FROM files WHERE id>=? AND id<?").map_err(e2s)?;
            let rows = st.query_map([first, first + own as i64], |r| Ok((r.get::<_, String>(0)?, (r.get::<_, i64>(1)?, r.get::<_, Option<i64>>(2)?)))).map_err(e2s)?;
            for row in rows {
                let (name, data) = row.map_err(e2s)?;
                files.insert(name, data);
            }
        }
        let mut dirs: HashSet<String> = {
            let mut st = c.prepare_cached("SELECT name FROM dirs WHERE parent=?").map_err(e2s)?;
            let rows = st.query_map([id], |r| r.get::<_, String>(0)).map_err(e2s)?;
            rows.collect::<rusqlite::Result<_>>().map_err(e2s)?
        };
        for ent in listing {
            *seen += 1;
            if *seen % 1024 == 0 {
                std::thread::sleep(Duration::from_millis(2));
            }
            let Ok(ent) = ent else { return Ok(true) };
            let Ok(kind) = ent.file_type() else { return Ok(true) };
            if kind.is_symlink() {
                continue;
            }
            let name = ent.file_name().to_string_lossy().into_owned();
            if kind.is_dir() {
                if !dirs.remove(&name) {
                    return Ok(true);
                }
            } else if kind.is_file() {
                let read = if !fresh.is_empty() && fresh.contains(&fresh_key(&path.join(&name))) { fs::symlink_metadata(ent.path()) } else { ent.metadata() };
                let Ok(meta) = read else { return Ok(true) };
                if files.remove(&name) != Some((meta.len() as i64, ms(meta.modified()))) {
                    return Ok(true);
                }
            }
        }
        Ok(!files.is_empty() || !dirs.is_empty())
    }

    /// Descarta las carpetas avisadas que siguen igual que en el informe (archivos temporales, fechas de carpeta...): guardar un informe obliga a copiarlo entero.
    fn really_changed(c: &Connection, root: &str, ids: Vec<i64>, fresh: &HashSet<String>, ctl: &Ctl) -> Result<Vec<i64>, String> {
        let mut out = vec![];
        let mut seen = 0usize;
        for id in ids {
            if ctl.cancelled() {
                return Err("cancelado".into());
            }
            // Una carpeta que ya no existe solo se puede corregir releyendo a su madre.
            let gone = Self::dir_path(c, root, id)?.is_some_and(|p| fs::symlink_metadata(p).is_err());
            let parent: i64 = if gone { c.query_row("SELECT parent FROM dirs WHERE id=?", [id], |r| r.get(0)).map_err(e2s)? } else { 0 };
            if parent > 0 {
                out.push(parent);
            } else if seen > VERIFY_MAX_ENTRIES || Self::dir_differs(c, root, id, fresh, &mut seen)? {
                out.push(id);
            }
        }
        out.sort_unstable();
        out.dedup();
        Ok(out)
    }

    fn run_update(&self, root: &str, key: &str, source: Source, tmp: &Path, ctl: &Arc<Ctl>) -> Result<(&'static str, Option<ReportInfo>), String> {
        let _ = fs::remove_file(tmp);
        let dest = self.db_path(key);
        let started_at = now_ms();
        let began = Instant::now();
        let may_miss = matches!(source, Source::Dates { covered: false });
        let old = open_read(&dest).map_err(e2s)?;
        let prev = read_info(&old, Some(key)).ok_or("El informe guardado no es válido. Analiza de nuevo la ubicación.")?;
        if fs::symlink_metadata(root).is_err() {
            return Err("La ubicación ya no está disponible.".into());
        }
        let mut fresh = HashSet::new();
        let dirty = match &source {
            Source::Listed(list) => Self::resolve_dirs(&old, list)?,
            Source::Dates { .. } => Self::changed_since(&old, root, ctl)?,
            Source::Watched(ch) => {
                fresh = ch.files.iter().map(|f| fresh_key(&Path::new(root).join(f))).collect();
                let ids = Self::resolve_dirs(&old, &ch.dirs)?;
                Self::really_changed(&old, root, ids, &fresh, ctl)?
            }
        };
        if ctl.cancelled() {
            return Ok(("cancelled", None));
        }
        if dirty.is_empty() {
            drop(old);
            let mut info = prev;
            info.checked_at = now_ms();
            info.partial = info.partial || may_miss;
            let json = serde_json::to_string(&info).map_err(|e| e.to_string())?;
            let mut reader = self.reader.lock().unwrap();
            if reader.as_ref().is_some_and(|o| o.key == key) {
                *reader = None;
            }
            let rw = Connection::open(&dest).map_err(e2s)?;
            rw.execute("UPDATE meta SET value=? WHERE key='report'", [json]).map_err(e2s)?;
            drop(rw);
            return Ok(("unchanged", Some(info)));
        }
        let mut known = HashMap::new();
        {
            let mut st = old.prepare("SELECT ext,id,cat FROM exts WHERE files>0").map_err(e2s)?;
            let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?, r.get::<_, u8>(2)?))).map_err(e2s)?;
            for row in rows {
                let (ext, id, cat) = row.map_err(e2s)?;
                known.insert(ext, ExtSlot { id, cat, files: 0, bytes: 0 });
            }
        }
        drop(old);
        let mut writer = Writer::update_from(tmp, &dest, dirty.clone(), ctl.clone())?;
        writer.fresh = fresh;
        let opts = Options::for_target(is_drive_root(root));
        let out = walk_update(Path::new(root), &mut writer, ctl, opts, known)?;
        if out.cancelled {
            return Ok(("cancelled", None));
        }
        let p = &out.progress;
        let mut info = ReportInfo { started_at, finished_at: now_ms(), duration_ms: began.elapsed().as_millis() as u64, files: p.files, dirs: p.dirs, bytes: p.bytes, checked_at: now_ms(), updated_dirs: writer.rescanned, partial: prev.partial || may_miss, links: prev.links + p.links, ..prev };
        writer.finish(&mut info, &out.exts, ctl)?;
        if ctl.cancelled() {
            return Ok(("cancelled", None));
        }
        {
            let mut reader = self.reader.lock().unwrap();
            if reader.as_ref().is_some_and(|o| o.key == key) {
                *reader = None;
            }
            fs::rename(tmp, &dest).map_err(|e| format!("No se pudo guardar el informe: {e}"))?;
        }
        info.db_bytes = fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
        Ok(("updated", Some(info)))
    }

    /// Empieza a vigilar una ubicación con informe. Solo hay una vigilada a la vez y el hilo se limita a anotar y, cuando hay calma, actualizar.
    pub fn watch(self: &Arc<Self>, root: &str, on_event: Arc<dyn Fn(AnalyzeEvent) + Send + Sync>) -> Result<(), String> {
        let root = clean_root(root);
        self.unwatch();
        let info = self.report(&root).ok_or("No hay ningún informe de esta ubicación.")?;
        let dirty = Arc::new(Mutex::new(Dirty::default()));
        // Los datos propios de LFind (índice, miniaturas, informes) cambian solos: salvo que sea justo eso lo que se analiza, no cuentan.
        let own = self.dir.parent().filter(|p| !Path::new(&root).starts_with(p)).unwrap_or(&self.dir).to_path_buf();
        let watcher = Watcher::start(Path::new(&root), own.clone(), dirty.clone())?;
        let stop = Arc::new(AtomicBool::new(false));
        *self.watching.lock().unwrap() = Some(Watching { root: root.clone(), stop: stop.clone(), _watcher: watcher });
        let me = self.clone();
        let min_gap = if is_drive_root(&root) { WATCH_MIN_DRIVE } else { WATCH_MIN_FOLDER };
        let reconcile_every = if is_drive_root(&root) { WATCH_RECONCILE_DRIVE } else { WATCH_RECONCILE_FOLDER };
        let check_on_start = now_ms() - info.checked_at.max(info.finished_at) > WATCH_VERIFY_AFTER_MS;
        let spawned = std::thread::Builder::new().name("lfind-analyzer-auto".into()).spawn(move || {
            crate::sys::background_thread();
            let started = Instant::now();
            let mut pending_first = check_on_start;
            let mut last_run: Option<Instant> = None;
            let mut last_reconcile = Instant::now();
            let mut retried: Option<Instant> = None;
            fn lock(d: &Mutex<Dirty>) -> std::sync::MutexGuard<'_, Dirty> {
                d.lock().unwrap_or_else(|e| e.into_inner())
            }
            while !stop.load(Ordering::Relaxed) {
                std::thread::sleep(WATCH_TICK);
                if me.job.lock().unwrap().is_some() {
                    continue;
                }
                let broken = lock(&dirty).broken;
                if broken && retried.map_or(true, |t| t.elapsed() >= WATCH_RETRY) {
                    retried = Some(Instant::now());
                    if let Ok(w) = Watcher::start(Path::new(&root), own.clone(), dirty.clone()) {
                        if let Some(cur) = me.watching.lock().unwrap().as_mut().filter(|c| Arc::ptr_eq(&c.stop, &stop)) {
                            cur._watcher = w;
                        }
                        let mut d = lock(&dirty);
                        d.broken = false;
                        d.mark_overflow(Instant::now()); // mientras estuvo caída no se vio nada
                    }
                }
                // Si una actualización no llega a guardarse, lo que traía se recupera comparando fechas.
                let forward = {
                    let (f, again) = (on_event.clone(), dirty.clone());
                    Box::new(move |e: AnalyzeEvent| {
                        if matches!(&e, AnalyzeEvent::Done { outcome: "failed" | "cancelled", .. }) {
                            again.lock().unwrap_or_else(|p| p.into_inner()).mark_overflow(Instant::now());
                        }
                        f(e)
                    }) as OnEvent
                };
                if pending_first && started.elapsed() >= WATCH_FIRST_CHECK {
                    pending_first = false;
                    last_run = Some(Instant::now());
                    let _ = me.update(&root, None, forward);
                    continue;
                }
                if last_run.is_some_and(|t| t.elapsed() < min_gap) {
                    continue;
                }
                let plan = lock(&dirty).take(Instant::now(), WATCH_QUIET, WATCH_MAX_WAIT);
                let source = match plan {
                    Some(Plan::Exact(changes)) => Source::Watched(changes),
                    Some(Plan::Lost) => {
                        last_reconcile = Instant::now();
                        Source::Dates { covered: false }
                    }
                    None if last_reconcile.elapsed() >= reconcile_every => {
                        last_reconcile = Instant::now();
                        Source::Dates { covered: !broken }
                    }
                    None => continue,
                };
                last_run = Some(Instant::now());
                if me.start_update(&root, source, forward).is_err() {
                    lock(&dirty).mark_overflow(Instant::now());
                }
            }
        });
        if let Err(e) = spawned {
            self.unwatch();
            return Err(format!("No se pudo iniciar la vigilancia: {e}"));
        }
        Ok(())
    }

    pub fn unwatch(&self) {
        if let Some(w) = self.watching.lock().unwrap().take() {
            w.stop.store(true, Ordering::Relaxed);
        }
    }

    /// Ubicación vigilada ahora mismo.
    pub fn watched(&self) -> Option<String> {
        self.watching.lock().unwrap().as_ref().map(|w| w.root.clone())
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

    /// Lo que debe coincidir entre un informe actualizado y uno hecho desde cero.
    fn snapshot(a: &Arc<Analyzer>, root: &str) -> String {
        let info = a.report(root).unwrap();
        let mut dirs: Vec<String> = a.dirs(root, &DirsQuery { dir: 1, tree: true, sort: "name".into(), limit: 500, ..Default::default() }).unwrap().items.into_iter().map(|d| format!("{}:{}:{}:{}:{}:{}", d.path, d.bytes, d.files, d.dirs, d.own_files, d.own_bytes)).collect();
        dirs.sort();
        let mut files: Vec<String> = a.files(root, &FilesQuery { dir: 1, limit: 500, ..Default::default() }).unwrap().items.into_iter().map(|f| format!("{}:{}:{}", f.path, f.size, f.ext)).collect();
        files.sort();
        let cats = a.categories(root, 1).unwrap();
        let mut exts: Vec<String> = a.exts(root, 1, 0, 100).unwrap().items.into_iter().map(|e| format!("{}:{}:{}", e.ext, e.files, e.bytes)).collect();
        exts.sort();
        format!("{}/{}/{}\n{dirs:?}\n{files:?}\n{cats:?}\n{exts:?}", info.files, info.dirs, info.bytes)
    }

    fn update(a: &Arc<Analyzer>, root: &str, changed: Option<Vec<String>>) -> &'static str {
        let (tx, rx) = mpsc::channel();
        a.update(root, changed, Box::new(move |e| {
            if let AnalyzeEvent::Done { outcome, .. } = e {
                let _ = tx.send(outcome);
            }
        }))
        .unwrap();
        rx.recv_timeout(Duration::from_secs(30)).unwrap()
    }

    /// Hace una fotografía de un análisis completo limpio de `r`, en otro almacén.
    fn fresh(r: &Path) -> String {
        let b = Arc::new(Analyzer::new(tmp()));
        run(&b, r);
        snapshot(&b, r.to_str().unwrap())
    }

    /// Da tiempo a que cambie la fecha de la carpeta y la deja distinta aunque el reloj sea poco preciso.
    fn touch_later() {
        std::thread::sleep(Duration::from_millis(30));
    }

    #[test]
    fn update_equals_a_full_scan_after_adding_and_removing() {
        let (a, r, root) = fixture();
        touch_later();
        put(&r.join("x/y/nuevo.mp4"), 300);
        fs::remove_file(r.join("x/b.txt")).unwrap();
        fs::create_dir_all(r.join("z/carpeta/nueva")).unwrap();
        put(&r.join("z/carpeta/nueva/f.pdf"), 77);
        fs::remove_dir_all(r.join("x/y/nada")).ok();
        assert_eq!(update(&a, &root, None), "updated");
        assert_eq!(snapshot(&a, &root), fresh(&r));
        let i = a.report(&root).unwrap();
        assert!(i.updated_dirs >= 2 && i.partial, "por fechas: puede faltar algo y se indica");
        assert!(i.full_scan_at <= i.finished_at);
        fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn removing_a_whole_folder_is_detected_by_dates() {
        let (a, r, root) = fixture();
        touch_later();
        fs::remove_dir_all(r.join("x")).unwrap();
        assert_eq!(update(&a, &root, None), "updated");
        assert_eq!(snapshot(&a, &root), fresh(&r));
        assert!(a.node(&root, "x").unwrap().is_none());
        fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn known_folders_update_catches_a_size_change_that_dates_would_miss() {
        let (a, r, root) = fixture();
        touch_later();
        put(&r.join("x/y/c.txt"), 999);
        assert_eq!(update(&a, &root, Some(vec!["x/y".into()])), "updated");
        assert_eq!(snapshot(&a, &root), fresh(&r));
        let i = a.report(&root).unwrap();
        assert!(!i.partial, "con la lista exacta de carpetas no falta nada");
        assert_eq!(i.updated_dirs, 1, "solo se releyó la carpeta afectada");
        fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn a_change_in_a_folder_unknown_to_the_report_rescans_its_nearest_ancestor() {
        let (a, r, root) = fixture();
        touch_later();
        put(&r.join("z/otra/f.png"), 11);
        assert_eq!(update(&a, &root, Some(vec!["z/otra".into()])), "updated");
        assert_eq!(snapshot(&a, &root), fresh(&r));
        fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn nothing_changed_only_records_the_check() {
        let (a, r, root) = fixture();
        let before = a.report(&root).unwrap();
        let snap = snapshot(&a, &root);
        std::thread::sleep(Duration::from_millis(5));
        assert_eq!(update(&a, &root, None), "unchanged");
        let after = a.report(&root).unwrap();
        assert_eq!(snapshot(&a, &root), snap);
        assert_eq!(after.finished_at, before.finished_at);
        assert!(after.checked_at > before.checked_at);
        fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn repeated_updates_keep_the_tree_consistent() {
        let (a, r, root) = fixture();
        for i in 0..4 {
            touch_later();
            put(&r.join(format!("x/n{i}.bin")), 10 + i);
            put(&r.join(format!("z/s{i}/m{i}.zip")), 20 + i);
            assert_eq!(update(&a, &root, None), "updated", "vuelta {i}");
            assert_eq!(snapshot(&a, &root), fresh(&r), "vuelta {i}");
        }
        let x = a.node(&root, "x").unwrap().unwrap();
        assert!(a.files(&root, &FilesQuery { dir: x.id, limit: 100, ..Default::default() }).unwrap().items.iter().all(|f| Path::new(&f.path).starts_with(r.join("x"))), "el intervalo de archivos sigue siendo exacto");
        fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn a_new_extension_during_an_update_gets_its_own_identity() {
        let (a, r, root) = fixture();
        touch_later();
        put(&r.join("x/raro.zzq"), 5);
        assert_eq!(update(&a, &root, None), "updated");
        assert_eq!(a.files(&root, &FilesQuery { dir: 1, exts: vec!["zzq".into()], limit: 5, ..Default::default() }).unwrap().items.len(), 1);
        assert_eq!(a.files(&root, &FilesQuery { dir: 1, exts: vec!["zip".into()], limit: 5, ..Default::default() }).unwrap().items.len(), 1);
        fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn a_failed_or_impossible_update_keeps_the_report() {
        let (a, r, root) = fixture();
        let snap = snapshot(&a, &root);
        let saved = tmp().join("r");
        fs::rename(&r, &saved).unwrap();
        assert_eq!(update(&a, &root, None), "failed");
        fs::rename(&saved, &r).unwrap();
        assert_eq!(snapshot(&a, &root), snap);
        assert!(!fs::read_dir(&a.dir).unwrap().flatten().any(|e| e.path().extension().is_some_and(|x| x == "tmp")));
        fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn a_full_scan_clears_the_partial_mark() {
        let (a, r, root) = fixture();
        touch_later();
        put(&r.join("x/q.txt"), 1);
        update(&a, &root, None);
        assert!(a.report(&root).unwrap().partial);
        run(&a, &r);
        let i = a.report(&root).unwrap();
        assert!(!i.partial && i.updated_dirs == 0 && i.full_scan_at == i.finished_at);
        fs::remove_dir_all(r).unwrap();
    }

    fn update_from(a: &Arc<Analyzer>, root: &str, source: Source) -> &'static str {
        let (tx, rx) = mpsc::channel();
        a.start_update(root, source, Box::new(move |e| {
            if let AnalyzeEvent::Done { outcome, .. } = e {
                let _ = tx.send(outcome);
            }
        }))
        .unwrap();
        rx.recv_timeout(Duration::from_secs(30)).unwrap()
    }

    fn watched(dirs: &[&str], files: &[&str]) -> Source {
        Source::Watched(Changes { dirs: dirs.iter().map(|s| s.to_string()).collect(), files: files.iter().map(|s| s.to_string()).collect() })
    }

    #[test]
    fn notices_without_a_real_change_do_not_rewrite_the_report() {
        let (a, r, root) = fixture();
        let before = snapshot(&a, &root);
        let db = a.db_path(&key_of(&root));
        let written = fs::metadata(&db).unwrap().len();
        let tmp_file = r.join("x").join("~temporal.tmp");
        fs::write(&tmp_file, b"abc").unwrap();
        fs::remove_file(&tmp_file).unwrap();
        assert_eq!(update_from(&a, &root, watched(&["x", "x/y", ""], &["x/~temporal.tmp"])), "unchanged");
        let i = a.report(&root).unwrap();
        assert_eq!((i.updated_dirs, i.partial, fs::metadata(&db).unwrap().len()), (0, false, written));
        assert!(i.checked_at >= i.finished_at);
        assert_eq!(snapshot(&a, &root), before);
        fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn a_size_change_noticed_by_the_watcher_matches_a_full_scan() {
        let (a, r, root) = fixture();
        touch_later();
        fs::write(r.join("x").join("b.txt"), vec![1u8; 4096]).unwrap();
        assert_eq!(update_from(&a, &root, watched(&["x"], &["x/b.txt"])), "updated");
        let i = a.report(&root).unwrap();
        assert_eq!((i.updated_dirs, i.partial, i.bytes), (1, false, 1135 - 10 + 4096));
        assert_eq!(snapshot(&a, &root), fresh(&r));
        fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn a_new_or_removed_folder_counts_as_a_change() {
        let (a, r, root) = fixture();
        fs::create_dir_all(r.join("x").join("nueva")).unwrap();
        fs::write(r.join("x").join("nueva").join("n.txt"), b"12345").unwrap();
        assert_eq!(update_from(&a, &root, watched(&["x/nueva"], &["x/nueva/n.txt"])), "updated");
        assert_eq!(snapshot(&a, &root), fresh(&r));
        fs::remove_dir_all(r.join("x").join("nueva")).unwrap();
        assert_eq!(update_from(&a, &root, watched(&["x/nueva"], &[])), "updated");
        assert_eq!(snapshot(&a, &root), fresh(&r));
        fs::remove_dir_all(r).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn a_noticed_file_reports_its_real_size_while_still_open() {
        use std::io::Write;
        let (a, r, root) = fixture();
        let path = r.join("x").join("b.txt");
        let mut held = fs::OpenOptions::new().append(true).open(&path).unwrap();
        held.write_all(&[9u8; 70_000]).unwrap();
        assert_eq!(update_from(&a, &root, watched(&["x"], &["x/b.txt"])), "updated");
        assert_eq!(a.report(&root).unwrap().bytes, 1135 + 70_000);
        drop(held);
        fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn a_covered_reconciliation_keeps_the_report_exact() {
        let (a, r, root) = fixture();
        touch_later();
        fs::write(r.join("x").join("nuevo.txt"), b"1234").unwrap();
        assert_eq!(update_from(&a, &root, Source::Dates { covered: true }), "updated");
        assert!(!a.report(&root).unwrap().partial);
        assert_eq!(update_from(&a, &root, Source::Dates { covered: true }), "unchanged");
        assert!(!a.report(&root).unwrap().partial, "ni actualizando ni sin cambios");
        assert_eq!(update_from(&a, &root, Source::Dates { covered: false }), "unchanged");
        assert!(a.report(&root).unwrap().partial, "sin vigilancia continua puede faltar algo");
        fs::remove_dir_all(r).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn watching_can_stop_and_start_without_leaks() {
        let (a, r, root) = fixture();
        let f: Arc<dyn Fn(AnalyzeEvent) + Send + Sync> = Arc::new(|_| {});
        a.watch(&root, f.clone()).unwrap();
        assert!(a.watched().is_some());
        a.watch(&root, f).unwrap();
        a.unwatch();
        assert!(a.watched().is_none());
        std::thread::sleep(Duration::from_millis(100));
        fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn roots() {
        assert!(is_drive_root("C:\\") && is_drive_root("C:") && !is_drive_root("C:\\a"));
        assert_eq!(key_of("C:\\Users\\A\\"), key_of("c:/users/a"));
        assert_ne!(key_of("C:\\a"), key_of("C:\\b"));
    }
}
