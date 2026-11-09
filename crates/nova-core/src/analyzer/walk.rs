//! Recorrido de una unidad o carpeta para el analizador. Un solo hilo, solo metadatos, memoria acotada.
//!
//! * Nunca abre un archivo: en Windows, `DirEntry::metadata` devuelve lo que ya trae la enumeración de la carpeta.
//! * No sigue enlaces simbólicos, uniones ni puntos de montaje: no hay ciclos ni salidas hacia otras ubicaciones.
//! * Las carpetas cuyo contenido tendría que traerse de la nube no se enumeran (ver [`attrs_cloud_dir`]).
//! * Recorrido en profundidad con identificadores en preorden: el subárbol de una carpeta ocupa un intervalo contiguo de carpetas
//!   y de archivos, y los totales se acumulan al cerrar cada carpeta, así que en memoria solo está la rama actual y lo pendiente.

use crate::dir::attrs_cloud_only;
use crate::index::classify::{category_exts, ext_of};
use serde::Serialize;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant, UNIX_EPOCH};

/// Categorías, con los mismos identificadores que `src/lib/fileKinds.ts`.
pub const CATEGORIES: [&str; 12] = ["image", "video", "audio", "archive", "code", "doc", "sheet", "slides", "pdf", "design", "exec", "other"];
const OTHER: u8 = 11;

/// Carpeta no enumerada porque su contenido está en la nube.
pub const DIR_CLOUD_PENDING: u32 = 1;
/// Carpeta que no se pudo leer.
pub const DIR_UNREADABLE: u32 = 2;
/// Archivo cuyo contenido solo está en la nube.
pub const FILE_CLOUD: u32 = 1;

/// Extensión reservada para los archivos sin extensión.
pub const EXT_NONE: u32 = 0;
/// Extensión reservada cuando se supera [`MAX_EXTS`].
pub const EXT_MORE: u32 = 1;
const MAX_EXTS: usize = 20_000;
const MAX_EXT_LEN: usize = 16;

const TOP_FILES: usize = 24;
const TOP_DIRS: usize = 32;

pub fn category_index(ext: &str) -> u8 {
    static MAP: OnceLock<HashMap<&'static str, u8>> = OnceLock::new();
    let map = MAP.get_or_init(|| {
        let mut m = HashMap::new();
        for (i, c) in CATEGORIES.iter().enumerate() {
            for e in category_exts(c) {
                m.insert(*e, i as u8);
            }
        }
        m
    });
    map.get(ext).copied().unwrap_or(OTHER)
}

/// ¿Enumerar esta carpeta obligaría a traer su contenido de la nube? Windows lo indica con estos atributos.
pub(crate) const fn attrs_cloud_dir(attrs: u32) -> bool {
    const RECALL_ON_OPEN: u32 = 0x4_0000;
    const RECALL_ON_DATA_ACCESS: u32 = 0x40_0000;
    attrs & (RECALL_ON_OPEN | RECALL_ON_DATA_ACCESS) != 0
}

#[cfg(windows)]
fn attrs_of(m: &fs::Metadata) -> u32 {
    use std::os::windows::fs::MetadataExt;
    m.file_attributes()
}

#[cfg(not(windows))]
fn attrs_of(_m: &fs::Metadata) -> u32 {
    0
}

#[derive(Default)]
pub struct Ctl {
    pub cancel: AtomicBool,
    pub pause: AtomicBool,
}

impl Ctl {
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
    fn paused(&self) -> bool {
        self.pause.load(Ordering::Relaxed)
    }
}

pub struct FileRow<'a> {
    pub id: i64,
    pub dir: i64,
    pub name: &'a str,
    pub ext: u32,
    pub cat: u8,
    pub size: u64,
    pub modified: Option<i64>,
    pub flags: u32,
}

#[derive(Clone, Debug, Default)]
pub struct DirRow {
    pub id: i64,
    pub parent: i64,
    /// Última carpeta del subárbol (preorden).
    pub last: i64,
    pub name: String,
    /// Intervalo `[first_file, end_file)` de los archivos del subárbol.
    pub first_file: i64,
    pub end_file: i64,
    pub own_files: u64,
    pub own_bytes: u64,
    pub files: u64,
    pub dirs: u64,
    pub bytes: u64,
    pub cloud_bytes: u64,
    pub modified: Option<i64>,
    pub flags: u32,
}

/// Destino de lo encontrado.
pub trait Sink {
    fn file(&mut self, f: &FileRow) -> Result<(), String>;
    fn dir(&mut self, d: &DirRow) -> Result<(), String>;
    fn issue(&mut self, path: &str, message: &str) -> Result<(), String>;
}

#[derive(Clone, Copy)]
pub struct Options {
    /// Se cede el procesador cada tantas entradas.
    pub pace_every: u32,
    pub pace_ms: u64,
    pub emit_every: Duration,
}

impl Options {
    /// Una unidad completa va más despacio que una carpeta: el análisis no debe notarse en el resto del equipo.
    pub fn for_target(full_drive: bool) -> Self {
        Options { pace_every: if full_drive { 1024 } else { 4096 }, pace_ms: if full_drive { 2 } else { 1 }, emit_every: Duration::from_millis(250) }
    }
}

#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CatStat {
    pub cat: String,
    pub files: u64,
    pub bytes: u64,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct LiveDir {
    pub name: String,
    pub files: u64,
    pub bytes: u64,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct LiveFile {
    pub size: u64,
    pub path: String,
    pub modified: Option<i64>,
}

/// Lo que se sabe hasta ahora: pequeño y de tamaño acotado, sea cual sea el tamaño del disco.
#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub files: u64,
    pub dirs: u64,
    pub bytes: u64,
    pub cloud_bytes: u64,
    pub cloud_files: u64,
    pub links: u64,
    pub errors: u64,
    pub cloud_dirs: u64,
    pub elapsed_ms: u64,
    pub paused: bool,
    pub current: String,
    pub categories: Vec<CatStat>,
    pub top_dirs: Vec<LiveDir>,
    pub top_files: Vec<LiveFile>,
}

pub struct ExtSlot {
    pub id: u32,
    pub cat: u8,
    pub files: u64,
    pub bytes: u64,
}

pub struct Outcome {
    pub progress: Progress,
    pub exts: HashMap<String, ExtSlot>,
    pub cancelled: bool,
}

struct Pending {
    path: PathBuf,
    depth: usize,
    modified: Option<i64>,
    cloud: bool,
}

fn ms(t: std::io::Result<std::time::SystemTime>) -> Option<i64> {
    let t = t.ok()?;
    Some(match t.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_millis() as i64,
        Err(e) => -(e.duration().as_millis() as i64),
    })
}

struct Walker<'a> {
    sink: &'a mut dyn Sink,
    ctl: &'a Ctl,
    opts: Options,
    on_progress: &'a mut dyn FnMut(&Progress),
    chain: Vec<DirRow>,
    next_dir: i64,
    next_file: i64,
    p: Progress,
    cats: [(u64, u64); 12],
    exts: HashMap<String, ExtSlot>,
    heap: BinaryHeap<Reverse<LiveFile>>,
    root_kids: Vec<LiveDir>,
    since_pace: u32,
    started: Instant,
    last_emit: Instant,
}

impl Walker<'_> {
    fn intern(&mut self, ext: &str, cat: u8, size: u64) -> u32 {
        let (key, id_new) = if ext.is_empty() || ext.len() > MAX_EXT_LEN {
            ("", EXT_NONE)
        } else if !self.exts.contains_key(ext) && self.exts.len() >= MAX_EXTS {
            ("*", EXT_MORE)
        } else {
            (ext, self.exts.len() as u32 + 2)
        };
        let slot = self.exts.entry(key.to_string()).or_insert(ExtSlot { id: id_new, cat: if key.is_empty() || key == "*" { OTHER } else { cat }, files: 0, bytes: 0 });
        slot.files += 1;
        slot.bytes += size;
        slot.id
    }

    fn snapshot(&mut self, current: &Path) {
        self.p.elapsed_ms = self.started.elapsed().as_millis() as u64;
        self.p.paused = self.ctl.paused();
        self.p.current = current.to_string_lossy().into_owned();
        self.p.categories = self.cats.iter().enumerate().filter(|(_, c)| c.0 > 0).map(|(i, c)| CatStat { cat: CATEGORIES[i].to_string(), files: c.0, bytes: c.1 }).collect();
        let mut dirs = self.root_kids.clone();
        // La carpeta de primer nivel que se está recorriendo también cuenta, con lo que lleva.
        if let Some(d) = self.chain.get(1) {
            dirs.push(LiveDir { name: d.name.clone(), files: d.files + d.own_files, bytes: d.bytes + d.own_bytes });
        }
        dirs.sort_by_key(|d| Reverse(d.bytes));
        dirs.truncate(TOP_DIRS);
        self.p.top_dirs = dirs;
        let mut files: Vec<LiveFile> = self.heap.iter().map(|r| r.0.clone()).collect();
        files.sort_by_key(|f| Reverse(f.size));
        self.p.top_files = files;
        (self.on_progress)(&self.p);
        self.last_emit = Instant::now();
    }

    /// Espera mientras esté en pausa. Devuelve `true` si hay que parar.
    fn checkpoint(&mut self, current: &Path) -> bool {
        if self.ctl.paused() && !self.ctl.cancelled() {
            self.snapshot(current);
            while self.ctl.paused() && !self.ctl.cancelled() {
                std::thread::sleep(Duration::from_millis(40));
            }
            self.snapshot(current);
        }
        if self.last_emit.elapsed() >= self.opts.emit_every {
            self.snapshot(current);
        }
        self.ctl.cancelled()
    }

    fn pace(&mut self) {
        self.since_pace += 1;
        if self.since_pace >= self.opts.pace_every {
            self.since_pace = 0;
            if self.opts.pace_ms > 0 {
                std::thread::sleep(Duration::from_millis(self.opts.pace_ms));
            }
        }
    }

    /// Cierra las carpetas abiertas por debajo de `depth`, acumulando sus totales en la carpeta madre.
    fn close_to(&mut self, depth: usize) -> Result<(), String> {
        while self.chain.len() > depth {
            let mut d = self.chain.pop().expect("rama no vacía");
            d.last = self.next_dir - 1;
            d.end_file = self.next_file;
            d.files += d.own_files;
            d.bytes += d.own_bytes;
            if let Some(up) = self.chain.last_mut() {
                up.files += d.files;
                up.dirs += d.dirs + 1;
                up.bytes += d.bytes;
                up.cloud_bytes += d.cloud_bytes;
            }
            if self.chain.len() == 1 {
                self.root_kids.push(LiveDir { name: d.name.clone(), files: d.files, bytes: d.bytes });
                if self.root_kids.len() > TOP_DIRS * 2 {
                    self.root_kids.sort_by_key(|k| Reverse(k.bytes));
                    self.root_kids.truncate(TOP_DIRS);
                }
            }
            self.sink.dir(&d)?;
        }
        Ok(())
    }

    fn fail(&mut self, path: &Path, e: &std::io::Error) -> Result<(), String> {
        self.p.errors += 1;
        self.sink.issue(&path.to_string_lossy(), &e.to_string())
    }

    fn add_file(&mut self, dir: &Path, name: &str, size: u64, modified: Option<i64>, attrs: u32) -> Result<(), String> {
        let ext = ext_of(name);
        let cat = category_index(&ext);
        let ext_id = self.intern(&ext, cat, size);
        let cloud = attrs_cloud_only(attrs);
        let id = self.next_file;
        self.next_file += 1;
        let parent = self.chain.last_mut().expect("carpeta abierta");
        parent.own_files += 1;
        parent.own_bytes += size;
        if cloud {
            parent.cloud_bytes += size;
        }
        let dir_id = parent.id;
        self.p.files += 1;
        self.p.bytes += size;
        if cloud {
            self.p.cloud_files += 1;
            self.p.cloud_bytes += size;
        }
        self.cats[cat as usize].0 += 1;
        self.cats[cat as usize].1 += size;
        // La ruta completa solo se compone si el archivo entra en la lista de los mayores.
        let enters = self.heap.len() < TOP_FILES || self.heap.peek().is_some_and(|m| m.0.size < size);
        if enters {
            self.heap.push(Reverse(LiveFile { size, path: dir.join(name).to_string_lossy().into_owned(), modified }));
            if self.heap.len() > TOP_FILES {
                self.heap.pop();
            }
        }
        self.sink.file(&FileRow { id, dir: dir_id, name, ext: ext_id, cat, size, modified, flags: if cloud { FILE_CLOUD } else { 0 } })
    }

    fn read(&mut self, dir: &Path, depth: usize, stack: &mut Vec<Pending>) -> Result<bool, String> {
        let rd = match fs::read_dir(dir) {
            Ok(r) => r,
            Err(e) => {
                if depth == 0 {
                    return Err(format!("No se puede leer «{}»: {e}", dir.display()));
                }
                self.chain.last_mut().expect("carpeta abierta").flags |= DIR_UNREADABLE;
                self.fail(dir, &e)?;
                return Ok(false);
            }
        };
        for (n, ent) in rd.enumerate() {
            if n % 256 == 255 && self.checkpoint(dir) {
                return Ok(true);
            }
            self.pace();
            let ent = match ent {
                Ok(e) => e,
                Err(e) => {
                    self.fail(dir, &e)?;
                    continue;
                }
            };
            let ft = match ent.file_type() {
                Ok(t) => t,
                Err(e) => {
                    self.fail(&ent.path(), &e)?;
                    continue;
                }
            };
            // Enlaces simbólicos, uniones y puntos de montaje: se cuentan, pero ni se siguen ni suman tamaño.
            if ft.is_symlink() {
                self.p.links += 1;
                continue;
            }
            let meta = match ent.metadata() {
                Ok(m) => m,
                Err(e) => {
                    self.fail(&ent.path(), &e)?;
                    continue;
                }
            };
            if ft.is_dir() {
                self.p.dirs += 1;
                stack.push(Pending { path: ent.path(), depth: depth + 1, modified: ms(meta.modified()), cloud: attrs_cloud_dir(attrs_of(&meta)) });
            } else if ft.is_file() {
                let name = ent.file_name();
                let name = name.to_string_lossy();
                self.add_file(dir, &name, meta.len(), ms(meta.modified()), attrs_of(&meta))?;
            }
        }
        Ok(false)
    }

    fn run(&mut self, root: &Path) -> Result<bool, String> {
        let mut stack = vec![Pending { path: root.to_path_buf(), depth: 0, modified: fs::symlink_metadata(root).ok().and_then(|m| ms(m.modified())), cloud: false }];
        while let Some(p) = stack.pop() {
            if self.checkpoint(&p.path) {
                return Ok(true);
            }
            self.close_to(p.depth)?;
            let id = self.next_dir;
            self.next_dir += 1;
            let parent = self.chain.last().map_or(0, |d| d.id);
            let name = if p.depth == 0 { root.to_string_lossy().into_owned() } else { p.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default() };
            self.chain.push(DirRow { id, parent, name, first_file: self.next_file, modified: p.modified, ..Default::default() });
            if p.cloud {
                self.chain.last_mut().expect("recién añadida").flags |= DIR_CLOUD_PENDING;
                self.p.cloud_dirs += 1;
                continue;
            }
            if self.read(&p.path, p.depth, &mut stack)? {
                return Ok(true);
            }
        }
        self.close_to(0)?;
        Ok(false)
    }
}

/// Recorre `root` y entrega lo encontrado a `sink`. Si se cancela, lo escrito no tiene los totales cerrados: quien llama debe descartarlo.
pub fn walk(root: &Path, sink: &mut dyn Sink, ctl: &Ctl, opts: Options, on_progress: &mut dyn FnMut(&Progress)) -> Result<Outcome, String> {
    let meta = fs::symlink_metadata(root).map_err(|e| format!("No se puede acceder a «{}»: {e}", root.display()))?;
    if meta.file_type().is_symlink() {
        return Err("Esta ubicación es un enlace: abre y analiza la carpeta de destino.".into());
    }
    if !meta.is_dir() {
        return Err("Solo se pueden analizar unidades y carpetas.".into());
    }
    let now = Instant::now();
    let mut w = Walker {
        sink,
        ctl,
        opts,
        on_progress,
        chain: vec![],
        next_dir: 1,
        next_file: 1,
        p: Progress::default(),
        cats: [(0, 0); 12],
        exts: HashMap::new(),
        heap: BinaryHeap::with_capacity(TOP_FILES + 1),
        root_kids: vec![],
        since_pace: 0,
        started: now,
        last_emit: now,
    };
    let cancelled = w.run(root)?;
    w.snapshot(root);
    let Walker { p, exts, .. } = w;
    Ok(Outcome { progress: p, exts, cancelled })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;
    use std::sync::Arc;

    #[derive(Default)]
    struct Mem {
        files: Vec<(i64, i64, String, u64, u32)>,
        dirs: Vec<DirRow>,
        issues: Vec<String>,
    }
    impl Sink for Mem {
        fn file(&mut self, f: &FileRow) -> Result<(), String> {
            self.files.push((f.id, f.dir, f.name.to_string(), f.size, f.flags));
            Ok(())
        }
        fn dir(&mut self, d: &DirRow) -> Result<(), String> {
            self.dirs.push(d.clone());
            Ok(())
        }
        fn issue(&mut self, path: &str, _m: &str) -> Result<(), String> {
            self.issues.push(path.to_string());
            Ok(())
        }
    }

    const FAST: Options = Options { pace_every: u32::MAX, pace_ms: 0, emit_every: Duration::from_secs(3600) };

    static NEXT: AtomicU32 = AtomicU32::new(0);
    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("lfind-walk-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
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
        put(&r.join("x/y/c.TXT"), 20);
        put(&r.join("x/y/d"), 5);
        put(&r.join("z/e.zip"), 1000);
        fs::create_dir_all(r.join("vacia")).unwrap();
        r
    }
    fn run(r: &Path) -> (Mem, Outcome) {
        let mut m = Mem::default();
        let out = walk(r, &mut m, &Ctl::default(), FAST, &mut |_| {}).unwrap();
        (m, out)
    }

    #[test]
    fn totals_and_categories() {
        let r = tree();
        let (m, o) = run(&r);
        assert!(!o.cancelled);
        assert_eq!((o.progress.files, o.progress.dirs, o.progress.bytes), (5, 4, 1135));
        assert_eq!(m.dirs.len(), 5);
        let root = m.dirs.iter().find(|d| d.parent == 0).unwrap();
        assert_eq!((root.files, root.dirs, root.bytes, root.own_files, root.own_bytes), (5, 4, 1135, 1, 100));
        let cat = |c: &str| o.progress.categories.iter().find(|s| s.cat == c).map(|s| (s.files, s.bytes));
        assert_eq!(cat("doc"), Some((2, 30)));
        assert_eq!(cat("archive"), Some((1, 1000)));
        assert_eq!(cat("image"), Some((1, 100)));
        assert_eq!(cat("other"), Some((1, 5)));
        fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn subtrees_are_contiguous() {
        let r = tree();
        let (m, _) = run(&r);
        for d in &m.dirs {
            let by_range: u64 = m.files.iter().filter(|f| f.0 >= d.first_file && f.0 < d.end_file).map(|f| f.3).sum();
            assert_eq!(by_range, d.bytes, "intervalo de archivos de {}", d.name);
            let descendants = m.dirs.iter().filter(|x| x.id > d.id && x.id <= d.last).count() as u64;
            assert_eq!(descendants, d.dirs, "intervalo de carpetas de {}", d.name);
        }
        fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn extension_identifiers_are_case_insensitive_and_reserved() {
        let r = tree();
        let (_, o) = run(&r);
        assert_eq!(o.exts["txt"].files, 2);
        assert_eq!(o.exts[""].id, EXT_NONE);
        assert!(o.exts["zip"].id >= 2);
        fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn rejects_missing_or_a_file() {
        let r = tmp();
        let mut m = Mem::default();
        assert!(walk(&r.join("no"), &mut m, &Ctl::default(), FAST, &mut |_| {}).is_err());
        put(&r.join("f.txt"), 1);
        assert!(walk(&r.join("f.txt"), &mut m, &Ctl::default(), FAST, &mut |_| {}).is_err());
        fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn cancel_stops_early() {
        let r = tree();
        let ctl = Ctl::default();
        ctl.cancel.store(true, Ordering::Relaxed);
        let mut m = Mem::default();
        let o = walk(&r, &mut m, &ctl, FAST, &mut |_| {}).unwrap();
        assert!(o.cancelled);
        assert_eq!(o.progress.files, 0);
        fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn pause_holds_until_resumed_and_cancel_releases_it() {
        let r = tree();
        let ctl = Arc::new(Ctl::default());
        ctl.pause.store(true, Ordering::Relaxed);
        let (c2, r2) = (ctl.clone(), r.clone());
        let h = std::thread::spawn(move || {
            let mut m = Mem::default();
            walk(&r2, &mut m, &c2, FAST, &mut |_| {}).unwrap().progress.files
        });
        std::thread::sleep(Duration::from_millis(150));
        assert!(!h.is_finished(), "en pausa no termina");
        ctl.pause.store(false, Ordering::Relaxed);
        assert_eq!(h.join().unwrap(), 5);

        ctl.pause.store(true, Ordering::Relaxed);
        let (c3, r3) = (ctl.clone(), r.clone());
        let h = std::thread::spawn(move || walk(&r3, &mut Mem::default(), &c3, FAST, &mut |_| {}).unwrap().cancelled);
        std::thread::sleep(Duration::from_millis(80));
        ctl.cancel.store(true, Ordering::Relaxed);
        assert!(h.join().unwrap(), "cancelar libera la pausa");
        fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn progress_is_bounded() {
        let r = tmp();
        for i in 0..60 {
            put(&r.join(format!("d{i}/f{i}.bin")), i + 1);
        }
        let mut m = Mem::default();
        let mut seen = 0;
        let o = walk(&r, &mut m, &Ctl::default(), Options { emit_every: Duration::ZERO, ..FAST }, &mut |p| {
            seen += 1;
            assert!(p.top_dirs.len() <= TOP_DIRS && p.top_files.len() <= TOP_FILES);
        })
        .unwrap();
        assert!(seen > 1);
        assert_eq!(o.progress.top_files[0].size, 60, "el mayor archivo queda primero");
        assert_eq!(o.progress.top_dirs[0].bytes, 60);
        fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn cloud_attributes() {
        assert!(attrs_cloud_dir(0x10 | 0x40_0000));
        assert!(attrs_cloud_dir(0x10 | 0x4_0000));
        assert!(!attrs_cloud_dir(0x10 | 0x400)); // punto de reanálisis corriente de OneDrive, ya poblado
    }

    #[cfg(windows)]
    fn cmd(args: &[&str]) -> bool {
        std::process::Command::new("cmd").arg("/C").args(args).output().map(|o| o.status.success()).unwrap_or(false)
    }

    /// Una unión apuntando a su propia madre y otra hacia fuera: ni ciclos ni tamaño ajeno.
    #[cfg(windows)]
    #[test]
    fn junctions_are_not_followed() {
        let r = tree();
        let out = tmp();
        put(&out.join("enorme.bin"), 5000);
        assert!(cmd(&["mklink", "/J", r.join("x").join("ciclo").to_str().unwrap(), r.to_str().unwrap()]));
        assert!(cmd(&["mklink", "/J", r.join("fuera").to_str().unwrap(), out.to_str().unwrap()]));
        let (_, o) = run(&r);
        assert_eq!((o.progress.files, o.progress.bytes, o.progress.links), (5, 1135, 2));
        // Analizar directamente una unión se rechaza.
        let mut m = Mem::default();
        assert!(walk(&r.join("fuera"), &mut m, &Ctl::default(), FAST, &mut |_| {}).is_err());
        cmd(&["rmdir", r.join("x").join("ciclo").to_str().unwrap()]);
        cmd(&["rmdir", r.join("fuera").to_str().unwrap()]);
        fs::remove_dir_all(r).unwrap();
        fs::remove_dir_all(out).unwrap();
    }

    /// El atributo de «solo en la nube» se cuenta aparte, sin leer el archivo.
    #[cfg(windows)]
    #[test]
    fn cloud_only_files_are_counted_apart() {
        let r = tree();
        let f = r.join("z/e.zip");
        assert!(cmd(&["attrib", "+O", f.to_str().unwrap()]));
        let (m, o) = run(&r);
        assert_eq!((o.progress.cloud_files, o.progress.cloud_bytes), (1, 1000));
        assert_eq!(m.files.iter().find(|x| x.2 == "e.zip").unwrap().4, FILE_CLOUD);
        cmd(&["attrib", "-O", f.to_str().unwrap()]);
        fs::remove_dir_all(r).unwrap();
    }

    /// Destino que borra una carpeta en mitad del análisis: la enumeración de esa carpeta falla igual que una sin permiso.
    struct DeleteDuringWalk {
        inner: Mem,
        target: PathBuf,
    }
    impl Sink for DeleteDuringWalk {
        fn file(&mut self, f: &FileRow) -> Result<(), String> {
            if f.name == "a.png" {
                fs::remove_dir_all(&self.target).map_err(|e| e.to_string())?;
            }
            self.inner.file(f)
        }
        fn dir(&mut self, d: &DirRow) -> Result<(), String> {
            self.inner.dir(d)
        }
        fn issue(&mut self, path: &str, m: &str) -> Result<(), String> {
            self.inner.issue(path, m)
        }
    }

    /// Una carpeta ilegible se anota y el análisis sigue con las demás, en cualquier equipo y con cualquier privilegio.
    #[test]
    fn unreadable_folder_is_reported_and_skipped() {
        let r = tree();
        let mut sink = DeleteDuringWalk { inner: Mem::default(), target: r.join("x") };
        let o = walk(&r, &mut sink, &Ctl::default(), FAST, &mut |_| {}).unwrap();
        assert_eq!(o.progress.errors, 1);
        assert!(sink.inner.issues[0].ends_with('x'));
        assert!(sink.inner.dirs.iter().find(|d| d.name == "x").unwrap().flags & DIR_UNREADABLE != 0);
        assert_eq!((o.progress.files, o.progress.bytes), (2, 1100), "lo legible sigue contando");
        fs::remove_dir_all(r).unwrap();
    }

    /// Lo mismo con un permiso denegado de verdad. En algunos equipos (el de GitHub Actions, por ejemplo) la carpeta se puede
    /// leer pese a la denegación; ahí no hay nada que comprobar.
    #[cfg(windows)]
    #[test]
    fn denied_folder_is_reported_and_skipped() {
        let r = tree();
        let p = r.join("x");
        let who = std::env::var("USERNAME").unwrap();
        let deny = format!("{who}:(OI)(CI)(RD)");
        assert!(std::process::Command::new("icacls").args([p.to_str().unwrap(), "/deny", &deny]).output().unwrap().status.success());
        let enforced = fs::read_dir(&p).is_err();
        let (m, o) = run(&r);
        std::process::Command::new("icacls").args([p.to_str().unwrap(), "/remove:d", &who]).output().unwrap();
        if !enforced {
            fs::remove_dir_all(r).unwrap();
            return;
        }
        assert_eq!(o.progress.errors, 1);
        assert!(m.issues[0].ends_with('x'));
        assert!(m.dirs.iter().find(|d| d.name == "x").unwrap().flags & DIR_UNREADABLE != 0);
        assert_eq!((o.progress.files, o.progress.bytes), (2, 1100), "lo legible sigue contando");
        fs::remove_dir_all(r).unwrap();
    }
}
