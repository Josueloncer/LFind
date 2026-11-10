//! Operaciones de archivo reales: copiar, mover, eliminar, renombrar y crear.
//!
//! Principios:
//!  * Nada bloquea la interfaz: todo recibe una bandera de cancelación y notifica progreso.
//!  * Un error en un elemento no detiene el resto; se acumulan y se informan al final.
//!  * Las copias se escriben a un temporal y se renombran al terminar: una cancelación o un
//!    fallo nunca deja un archivo a medias con el nombre final, y «Reemplazar» no pierde el
//!    original si la copia falla.
//!  * No se siguen enlaces a carpetas (evita bucles y copiar medio disco por una unión).
//!  * Mover/eliminar/renombrar se rechazan sobre carpetas del sistema (ver `safety`).

use crate::names::{unique_name, validate_name};
use crate::safety;
use crate::undo::UndoEntry;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

const BUF: usize = 1 << 20;
/// Hasta este tamaño se usa `fs::copy` (CopyFileEx en Windows: rápido y conserva metadatos);
/// por encima se copia por bloques para poder informar progreso y cancelar a mitad de archivo.
pub const FAST_COPY_MAX: u64 = 8 << 20;
static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

// ───────────────────────── tipos públicos ─────────────────────────

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ErrKind {
    Denied,
    InUse,
    NotFound,
    Exists,
    Invalid,
    DiskFull,
    Blocked,
    TooLong,
    Other,
}

#[derive(Serialize, Clone, Debug)]
pub struct OpError {
    pub path: String,
    pub message: String,
    pub kind: ErrKind,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub files_done: u64,
    pub files_total: u64,
    pub current: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ItemInfo {
    pub path: String,
    pub size: u64,
    pub modified: Option<i64>,
    pub is_dir: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct ConflictInfo {
    pub src: ItemInfo,
    pub dst: ItemInfo,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Decision {
    Replace,
    Skip,
    KeepBoth,
    Cancel,
}

pub struct Ctx<'a> {
    pub cancel: &'a AtomicBool,
    /// Mientras esté activo, la operación espera antes del siguiente bloque o archivo.
    pub paused: Option<&'a AtomicBool>,
    pub on_progress: &'a mut dyn FnMut(&Progress),
    pub on_conflict: &'a mut dyn FnMut(&ConflictInfo) -> Decision,
}

impl Ctx<'_> {
    /// Sin ocupar procesador. La cancelación siempre puede con la pausa.
    pub fn wait_if_paused(&self) {
        while self.paused.is_some_and(|p| p.load(Ordering::Relaxed)) && !self.cancel.load(Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(80));
        }
    }
}

#[derive(Debug, Default)]
pub struct OpOutcome {
    /// Elementos de primer nivel completados sin errores.
    pub completed: u64,
    pub skipped: u64,
    pub errors: Vec<OpError>,
    pub cancelled: bool,
    pub undo: Option<UndoEntry>,
}

// ───────────────────────── errores ─────────────────────────

pub fn classify(e: &io::Error) -> (ErrKind, String) {
    use io::ErrorKind::*;
    let raw = e.raw_os_error();
    if cfg!(windows) {
        match raw {
            Some(32) | Some(33) => return (ErrKind::InUse, "El archivo está en uso por otro programa.".into()),
            Some(112) => return (ErrKind::DiskFull, "No hay espacio suficiente en el destino.".into()),
            Some(206) => return (ErrKind::TooLong, "La ruta es demasiado larga.".into()),
            Some(123) | Some(161) => return (ErrKind::Invalid, "El nombre o la ruta no son válidos.".into()),
            _ => {}
        }
    } else if raw == Some(28) {
        return (ErrKind::DiskFull, "No hay espacio suficiente en el destino.".into());
    }
    match e.kind() {
        PermissionDenied => (ErrKind::Denied, "Acceso denegado (permisos insuficientes o elemento protegido).".into()),
        NotFound => (ErrKind::NotFound, "No se encontró el elemento (¿se movió o se eliminó?).".into()),
        AlreadyExists => (ErrKind::Exists, "Ya existe un elemento con ese nombre.".into()),
        _ => (ErrKind::Other, e.to_string()),
    }
}

pub fn op_error(p: &Path, e: &io::Error) -> OpError {
    let (kind, message) = classify(e);
    OpError { path: p.display().to_string(), message, kind }
}

enum Fail {
    Cancelled,
    Io(io::Error),
    Msg(ErrKind, String),
}
impl From<io::Error> for Fail {
    fn from(e: io::Error) -> Self {
        Fail::Io(e)
    }
}
type R<T = ()> = Result<T, Fail>;

// ───────────────────────── estado de una operación ─────────────────────────

struct St<'c, 'a> {
    ctx: &'c mut Ctx<'a>,
    prog: Progress,
    skipped: u64,
    errors: Vec<OpError>,
    created: Vec<PathBuf>,
    moved: Vec<(PathBuf, PathBuf)>,
}

impl<'c, 'a> St<'c, 'a> {
    fn new(ctx: &'c mut Ctx<'a>, files: u64, bytes: u64) -> Self {
        St { ctx, prog: Progress { files_total: files, bytes_total: bytes, ..Default::default() }, skipped: 0, errors: vec![], created: vec![], moved: vec![] }
    }
    fn check(&self) -> R {
        self.ctx.wait_if_paused();
        if self.ctx.cancel.load(Ordering::Relaxed) { Err(Fail::Cancelled) } else { Ok(()) }
    }
    fn emit(&mut self) {
        (self.ctx.on_progress)(&self.prog);
    }
    /// Registra el fallo de un elemento y sigue; solo la cancelación se propaga.
    fn record(&mut self, p: &Path, f: Fail) -> R {
        match f {
            Fail::Cancelled => Err(Fail::Cancelled),
            Fail::Io(e) => {
                self.errors.push(op_error(p, &e));
                Ok(())
            }
            Fail::Msg(kind, message) => {
                self.errors.push(OpError { path: p.display().to_string(), message, kind });
                Ok(())
            }
        }
    }
    fn finish(self, undo: Option<UndoEntry>, completed: u64) -> OpOutcome {
        let cancelled = self.ctx.cancel.load(Ordering::Relaxed);
        OpOutcome { completed, skipped: self.skipped, errors: self.errors, cancelled, undo }
    }
}

fn ms(t: Option<std::time::SystemTime>) -> Option<i64> {
    t.and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64)
}

fn item_info(p: &Path, m: &fs::Metadata) -> ItemInfo {
    ItemInfo { path: p.display().to_string(), size: if m.is_dir() { 0 } else { m.len() }, modified: ms(m.modified().ok()), is_dir: m.is_dir() }
}

fn name_of(p: &Path) -> R<String> {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).ok_or_else(|| Fail::Msg(ErrKind::Invalid, "La ruta no tiene nombre de elemento.".into()))
}

/// Total de archivos y bytes (sin seguir enlaces a carpetas). Respeta la cancelación.
pub fn scan(paths: &[PathBuf], cancel: &AtomicBool) -> (u64, u64) {
    let (mut files, mut bytes) = (0u64, 0u64);
    let mut stack: Vec<PathBuf> = paths.to_vec();
    while let Some(p) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let Ok(m) = fs::symlink_metadata(&p) else { continue };
        if m.is_dir() {
            if let Ok(rd) = fs::read_dir(&p) {
                stack.extend(rd.flatten().map(|e| e.path()));
            }
        } else if m.file_type().is_symlink() {
            if let Ok(t) = fs::metadata(&p) {
                if !t.is_dir() {
                    files += 1;
                    bytes += t.len();
                }
            }
        } else {
            files += 1;
            bytes += m.len();
        }
    }
    (files, bytes)
}

fn make_writable(p: &Path) {
    #[cfg(windows)]
    if let Ok(m) = fs::metadata(p) {
        let mut perm = m.permissions();
        if perm.readonly() {
            #[allow(clippy::permissions_set_readonly_false)]
            perm.set_readonly(false);
            let _ = fs::set_permissions(p, perm);
        }
    }
    #[cfg(not(windows))]
    let _ = p;
}

fn is_cross_device(e: &io::Error) -> bool {
    e.raw_os_error() == Some(if cfg!(windows) { 17 } else { 18 })
}

// ───────────────────────── copiar ─────────────────────────

fn tmp_path(final_path: &Path) -> PathBuf {
    let n = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let name: String = final_path.file_name().map(|s| s.to_string_lossy().chars().take(80).collect()).unwrap_or_default();
    final_path.with_file_name(format!(".{name}.nova-{}-{n}.tmp", std::process::id()))
}

fn preserve(tmp: &Path, meta: &fs::Metadata) {
    if let Ok(m) = meta.modified() {
        if let Ok(f) = OpenOptions::new().write(true).open(tmp).or_else(|_| File::open(tmp)) {
            let _ = f.set_modified(m);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        crate::sys::copy_basic_attrs(meta.file_attributes(), tmp);
    }
}

fn write_tmp(src: &Path, tmp: &Path, meta: &fs::Metadata, st: &mut St) -> R {
    st.check()?;
    if meta.len() <= FAST_COPY_MAX {
        fs::copy(src, tmp)?;
        st.prog.bytes_done += meta.len();
    } else {
        let mut r = File::open(src)?;
        let mut w = OpenOptions::new().write(true).create_new(true).open(tmp)?;
        let mut buf = vec![0u8; BUF];
        loop {
            st.check()?;
            let n = r.read(&mut buf)?;
            if n == 0 {
                break;
            }
            w.write_all(&buf[..n])?;
            st.prog.bytes_done += n as u64;
            st.emit();
        }
        w.flush()?;
    }
    preserve(tmp, meta);
    Ok(())
}

fn copy_contents(src: &Path, final_path: &Path, replace: bool, st: &mut St) -> R {
    let meta = fs::metadata(src)?;
    let tmp = tmp_path(final_path);
    if let Err(f) = write_tmp(src, &tmp, &meta, st) {
        let _ = fs::remove_file(&tmp);
        return Err(f);
    }
    if replace {
        make_writable(final_path);
    }
    if let Err(e) = fs::rename(&tmp, final_path) {
        let _ = fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(())
}

fn skip_dir_link(p: &Path, st: &mut St) {
    st.skipped += 1;
    st.errors.push(OpError { path: p.display().to_string(), message: "Enlace simbólico a carpeta omitido (no se siguen enlaces).".into(), kind: ErrKind::Blocked });
}

fn copy_file(src: &Path, target: &Path, st: &mut St) -> R {
    st.prog.current = name_of(src)?;
    let mut final_path = target.to_path_buf();
    let mut replace = false;
    if let Ok(tm) = fs::symlink_metadata(&final_path) {
        if tm.is_dir() {
            return Err(Fail::Msg(ErrKind::Exists, "Ya existe una carpeta con el nombre de este archivo.".into()));
        }
        let sm = fs::metadata(src)?;
        let info = ConflictInfo { src: item_info(src, &sm), dst: item_info(&final_path, &tm) };
        match (st.ctx.on_conflict)(&info) {
            Decision::Replace => replace = true,
            Decision::Skip => {
                st.skipped += 1;
                st.prog.files_done += 1;
                st.prog.bytes_done += sm.len();
                st.emit();
                return Ok(());
            }
            Decision::KeepBoth => {
                let dir = final_path.parent().unwrap_or(Path::new("."));
                final_path = dir.join(unique_name(dir, &name_of(&final_path)?, false));
            }
            Decision::Cancel => return Err(Fail::Cancelled),
        }
    }
    copy_contents(src, &final_path, replace, st)?;
    if !replace {
        st.created.push(final_path);
    }
    st.prog.files_done += 1;
    st.emit();
    Ok(())
}

fn copy_dir(src: &Path, target: &Path, st: &mut St) -> R {
    match fs::symlink_metadata(target) {
        Ok(m) if m.is_dir() => {} // la carpeta existe: se combinan contenidos
        Ok(_) => return Err(Fail::Msg(ErrKind::Exists, "Ya existe un archivo con el nombre de esta carpeta.".into())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            fs::create_dir(target)?;
            st.created.push(target.to_path_buf());
        }
        Err(e) => return Err(e.into()),
    }
    for ent in fs::read_dir(src)? {
        st.check()?;
        let ent = match ent {
            Ok(e) => e,
            Err(e) => {
                st.record(src, Fail::Io(e))?;
                continue;
            }
        };
        let (p, t) = (ent.path(), target.join(ent.file_name()));
        let r = match fs::symlink_metadata(&p) {
            Err(e) => Err(Fail::Io(e)),
            Ok(m) if m.is_dir() => copy_dir(&p, &t, st),
            Ok(m) if m.file_type().is_symlink() => match fs::metadata(&p) {
                Ok(t2) if t2.is_dir() => {
                    skip_dir_link(&p, st);
                    Ok(())
                }
                Ok(_) => copy_file(&p, &t, st),
                Err(e) => Err(Fail::Io(e)),
            },
            Ok(_) => copy_file(&p, &t, st),
        };
        if let Err(f) = r {
            st.record(&p, f)?;
        }
    }
    Ok(())
}

fn copy_top(src: &Path, dest_dir: &Path, st: &mut St) -> R {
    let name = name_of(src)?;
    let meta = fs::symlink_metadata(src)?;
    let mut is_dir = meta.is_dir();
    if meta.file_type().is_symlink() {
        if fs::metadata(src)?.is_dir() {
            skip_dir_link(src, st);
            return Ok(());
        }
        is_dir = false;
    }
    if is_dir {
        safety::check_not_inside(dest_dir, src).map_err(|m| Fail::Msg(ErrKind::Invalid, m))?;
    }
    let same_dir = src.parent().map(|p| safety::same_path(p, dest_dir)).unwrap_or(false);
    let target = if same_dir { dest_dir.join(unique_name(dest_dir, &name, is_dir)) } else { dest_dir.join(&name) };
    if is_dir { copy_dir(src, &target, st) } else { copy_file(src, &target, st) }
}

fn dest_error(dest: &Path) -> Option<OpOutcome> {
    if dest.is_dir() {
        return None;
    }
    Some(OpOutcome { errors: vec![OpError { path: dest.display().to_string(), message: "La carpeta de destino no existe.".into(), kind: ErrKind::NotFound }], ..Default::default() })
}

pub fn copy_items(sources: &[PathBuf], dest_dir: &Path, ctx: &mut Ctx<'_>) -> OpOutcome {
    if let Some(o) = dest_error(dest_dir) {
        return o;
    }
    let (files, bytes) = scan(sources, ctx.cancel);
    let mut st = St::new(ctx, files, bytes);
    let mut completed = 0;
    for src in sources {
        if st.check().is_err() {
            break;
        }
        let before = st.errors.len();
        if let Err(f) = copy_top(src, dest_dir, &mut st) {
            if st.record(src, f).is_err() {
                break;
            }
        }
        if st.errors.len() == before {
            completed += 1;
        }
    }
    let undo = (!st.created.is_empty()).then(|| UndoEntry::Copy { created: st.created.clone() });
    st.emit();
    st.finish(undo, completed)
}

// ───────────────────────── mover ─────────────────────────

fn delete_after_copy(src: &Path, st: &mut St) {
    let before = st.errors.len();
    if let Ok(m) = fs::symlink_metadata(src) {
        let r = if m.is_dir() { delete_tree(src, st) } else { fs::remove_file(src).map_err(Fail::from) };
        if let Err(Fail::Io(e)) = r {
            st.errors.push(op_error(src, &e));
        }
    }
    if st.errors.len() > before {
        st.errors.push(OpError { path: src.display().to_string(), message: "Se copió al destino pero no se pudo eliminar el original.".into(), kind: ErrKind::Other });
    }
}

fn do_rename(src: &Path, target: &Path, is_dir: bool, replace: bool, st: &mut St) -> R {
    match fs::rename(src, target) {
        Ok(()) => {
            st.moved.push((src.to_path_buf(), target.to_path_buf()));
            st.prog.files_done += 1;
            st.emit();
            Ok(())
        }
        Err(e) if is_cross_device(&e) => {
            // Otra unidad: copia + eliminación del original (solo si todo se copió bien).
            let before = st.errors.len();
            if is_dir {
                let (_, b) = scan(&[src.to_path_buf()], st.ctx.cancel);
                st.prog.bytes_total += b;
                copy_dir(src, target, st)?;
            } else {
                st.prog.bytes_total += fs::metadata(src).map(|m| m.len()).unwrap_or(0);
                copy_contents(src, target, replace, st)?;
            }
            if st.errors.len() == before {
                delete_after_copy(src, st);
                st.moved.push((src.to_path_buf(), target.to_path_buf()));
            } else {
                st.errors.push(OpError { path: src.display().to_string(), message: "El original se conservó porque hubo errores al copiar.".into(), kind: ErrKind::Other });
            }
            st.prog.files_done += 1;
            st.emit();
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

fn move_path(src: &Path, target: &Path, is_dir: bool, st: &mut St) -> R {
    st.prog.current = name_of(src)?;
    st.check()?;
    let tm = match fs::symlink_metadata(target) {
        Ok(tm) => tm,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return do_rename(src, target, is_dir, false, st),
        Err(e) => return Err(e.into()),
    };
    if is_dir && tm.is_dir() {
        return move_merge(src, target, st);
    }
    if is_dir != tm.is_dir() {
        return Err(Fail::Msg(ErrKind::Exists, "Ya existe un elemento con ese nombre pero de distinto tipo.".into()));
    }
    let sm = fs::metadata(src)?;
    let info = ConflictInfo { src: item_info(src, &sm), dst: item_info(target, &tm) };
    match (st.ctx.on_conflict)(&info) {
        Decision::Replace => {
            make_writable(target);
            do_rename(src, target, false, true, st)
        }
        Decision::Skip => {
            st.skipped += 1;
            st.prog.files_done += 1;
            st.emit();
            Ok(())
        }
        Decision::KeepBoth => {
            let dir = target.parent().unwrap_or(Path::new("."));
            let t2 = dir.join(unique_name(dir, &name_of(target)?, false));
            do_rename(src, &t2, false, false, st)
        }
        Decision::Cancel => Err(Fail::Cancelled),
    }
}

fn move_merge(src: &Path, target: &Path, st: &mut St) -> R {
    for ent in fs::read_dir(src)? {
        st.check()?;
        let ent = match ent {
            Ok(e) => e,
            Err(e) => {
                st.record(src, Fail::Io(e))?;
                continue;
            }
        };
        let p = ent.path();
        let is_dir = fs::symlink_metadata(&p).map(|m| m.is_dir()).unwrap_or(false);
        if let Err(f) = move_path(&p, &target.join(ent.file_name()), is_dir, st) {
            st.record(&p, f)?;
        }
    }
    let _ = fs::remove_dir(src); // solo se elimina si quedó vacía
    Ok(())
}

pub fn move_items(sources: &[PathBuf], dest_dir: &Path, ctx: &mut Ctx<'_>) -> OpOutcome {
    if let Some(o) = dest_error(dest_dir) {
        return o;
    }
    let mut st = St::new(ctx, sources.len() as u64, 0);
    let mut completed = 0;
    for src in sources {
        if st.check().is_err() {
            break;
        }
        let before = st.errors.len();
        let r = (|| -> R {
            safety::check_destructive(src).map_err(|m| Fail::Msg(ErrKind::Blocked, m))?;
            let name = name_of(src)?;
            let meta = fs::symlink_metadata(src)?;
            let is_dir = meta.is_dir();
            if is_dir {
                safety::check_not_inside(dest_dir, src).map_err(|m| Fail::Msg(ErrKind::Invalid, m))?;
            }
            let target = dest_dir.join(name);
            if safety::same_path(&target, src) {
                return Err(Fail::Msg(ErrKind::Invalid, "El elemento ya está en esa carpeta.".into()));
            }
            move_path(src, &target, is_dir, &mut st)
        })();
        if let Err(f) = r {
            if st.record(src, f).is_err() {
                break;
            }
        }
        if st.errors.len() == before {
            completed += 1;
        }
    }
    let undo = (!st.moved.is_empty()).then(|| UndoEntry::Move { pairs: st.moved.clone() });
    st.emit();
    st.finish(undo, completed)
}

/// Mueve un solo elemento a una ruta libre (lo usa «Deshacer»). Devuelve el motivo si falla.
pub fn move_single(src: &Path, dst: &Path) -> Result<(), String> {
    let cancel = AtomicBool::new(false);
    let mut p = |_: &Progress| {};
    let mut c = |_: &ConflictInfo| Decision::Skip;
    let mut ctx = Ctx { cancel: &cancel, paused: None, on_progress: &mut p, on_conflict: &mut c };
    let is_dir = fs::symlink_metadata(src).map(|m| m.is_dir()).map_err(|e| classify(&e).1)?;
    let mut st = St::new(&mut ctx, 1, 0);
    let r = move_path(src, dst, is_dir, &mut st);
    match (r, st.errors.first()) {
        (Ok(()), None) => Ok(()),
        (Err(Fail::Io(e)), _) => Err(classify(&e).1),
        (Err(Fail::Msg(_, m)), _) => Err(m),
        (_, Some(e)) => Err(e.message.clone()),
        (Err(Fail::Cancelled), None) => Err("Cancelado.".into()),
    }
}

// ───────────────────────── eliminar (permanente) ─────────────────────────

fn delete_tree(p: &Path, st: &mut St) -> R {
    st.check()?;
    let m = fs::symlink_metadata(p)?;
    if m.is_dir() {
        let before = st.errors.len();
        for ent in fs::read_dir(p)? {
            match ent {
                Ok(e) => {
                    if let Err(f) = delete_tree(&e.path(), st) {
                        st.record(&e.path(), f)?;
                    }
                }
                Err(e) => st.record(p, Fail::Io(e))?,
            }
        }
        if st.errors.len() > before {
            return Err(Fail::Msg(ErrKind::Other, "No se eliminó la carpeta porque contiene elementos que no se pudieron eliminar.".into()));
        }
        make_writable(p);
        fs::remove_dir(p)?;
    } else {
        st.prog.current = name_of(p)?;
        let r = if m.file_type().is_symlink() {
            fs::remove_file(p).or_else(|_| fs::remove_dir(p))
        } else {
            fs::remove_file(p).or_else(|e| {
                if e.kind() == io::ErrorKind::PermissionDenied {
                    make_writable(p);
                    fs::remove_file(p)
                } else {
                    Err(e)
                }
            })
        };
        r?;
        st.prog.files_done += 1;
        st.prog.bytes_done += m.len();
        st.emit();
    }
    Ok(())
}

pub fn delete_items(paths: &[PathBuf], ctx: &mut Ctx<'_>) -> OpOutcome {
    // Sin recorrido previo (`scan`): empezar a borrar ya. El total se desconoce (0 = barra indeterminada); contar antes
    // recorría todo el árbol dos veces (medido en Linux: ~+21-24 % de tiempo antes de borrar el primer archivo).
    let mut st = St::new(ctx, 0, 0);
    let mut completed = 0;
    for p in paths {
        if st.check().is_err() {
            break;
        }
        let before = st.errors.len();
        let r = match safety::check_destructive(p) {
            Err(m) => Err(Fail::Msg(ErrKind::Blocked, m)),
            Ok(()) => delete_tree(p, &mut st),
        };
        if let Err(f) = r {
            if st.record(p, f).is_err() {
                break;
            }
        }
        if st.errors.len() == before {
            completed += 1;
        }
    }
    st.emit();
    st.finish(None, completed)
}

/// Tamaño de cada lote enviado a la Papelera: una sola operación del sistema por lote (en vez de una por elemento), con
/// avance y cancelación entre lotes.
pub const TRASH_BATCH: usize = 64;

/// Envía elementos a la Papelera por lotes. `batch` recibe un lote entero; si falla, cada elemento del lote se reintenta
/// por separado con `single` para atribuir el error al elemento correcto (un fallo no arrastra a los demás).
/// El envío real (crate `trash`) lo inyecta el llamador: así esta lógica es pura y se prueba sin Windows.
pub fn trash_items(
    paths: &[PathBuf],
    ctx: &mut Ctx<'_>,
    batch: &mut dyn FnMut(&[PathBuf]) -> Result<(), String>,
    single: &mut dyn FnMut(&Path) -> Result<(), String>,
) -> OpOutcome {
    let mut out = OpOutcome::default();
    let mut prog = Progress { files_total: paths.len() as u64, ..Default::default() };
    let mut ok: Vec<PathBuf> = Vec::with_capacity(paths.len());
    for p in paths {
        match safety::check_destructive(p) {
            Err(m) => out.errors.push(OpError { path: p.display().to_string(), message: m, kind: ErrKind::Blocked }),
            Ok(()) if fs::symlink_metadata(p).is_err() => {
                out.errors.push(OpError { path: p.display().to_string(), message: "El elemento ya no existe.".into(), kind: ErrKind::NotFound })
            }
            Ok(()) => ok.push(p.clone()),
        }
    }
    prog.files_done = (paths.len() - ok.len()) as u64;
    for chunk in ok.chunks(TRASH_BATCH) {
        ctx.wait_if_paused();
        if ctx.cancel.load(Ordering::Relaxed) {
            out.cancelled = true;
            break;
        }
        prog.current = chunk[0].file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if batch(chunk).is_ok() {
            out.completed += chunk.len() as u64;
        } else {
            // Un lote que falla a medias ya ha enviado el resto: lo que no sigue en su sitio está en la Papelera, y
            // reintentarlo daría un error falso.
            for p in chunk {
                if fs::symlink_metadata(p).is_err() {
                    out.completed += 1;
                    continue;
                }
                match single(p) {
                    Ok(()) => out.completed += 1,
                    Err(message) => out.errors.push(OpError { path: p.display().to_string(), message, kind: ErrKind::Other }),
                }
            }
        }
        prog.files_done += chunk.len() as u64;
        (ctx.on_progress)(&prog);
    }
    out
}

// ───────────────────────── renombrar y crear ─────────────────────────

/// Renombrado sin validaciones (lo usa «Deshacer», que ya comprobó el estado).
pub fn rename_raw(from: &Path, to: &Path) -> io::Result<()> {
    fs::rename(from, to)
}

pub fn rename_item(path: &Path, new_name: &str) -> Result<(PathBuf, UndoEntry), OpError> {
    let bad = |kind, m: String| OpError { path: path.display().to_string(), message: m, kind };
    validate_name(new_name).map_err(|m| bad(ErrKind::Invalid, m))?;
    safety::check_destructive(path).map_err(|m| bad(ErrKind::Blocked, m))?;
    let parent = path.parent().ok_or_else(|| bad(ErrKind::Invalid, "No se puede renombrar la raíz.".into()))?;
    let target = parent.join(new_name);
    let cur = name_of(path).map_err(|_| bad(ErrKind::Invalid, "Ruta no válida.".into()))?;
    if cur == new_name {
        return Err(bad(ErrKind::Invalid, "El nombre no ha cambiado.".into()));
    }
    let case_only = cur.to_lowercase() == new_name.to_lowercase();
    if fs::symlink_metadata(&target).is_ok() && !(case_only && cfg!(windows)) {
        return Err(bad(ErrKind::Exists, "Ya existe un elemento con ese nombre.".into()));
    }
    let r = if case_only && cfg!(windows) {
        // En Windows A.txt → a.txt "existe": se pasa por un nombre temporal.
        let tmp = tmp_path(&target);
        fs::rename(path, &tmp).and_then(|_| fs::rename(&tmp, &target))
    } else {
        fs::rename(path, &target)
    };
    r.map_err(|e| op_error(path, &e))?;
    Ok((target.clone(), UndoEntry::Rename { from: path.to_path_buf(), to: target }))
}

fn create_unique(parent: &Path, base: &str, is_dir: bool) -> Result<PathBuf, OpError> {
    validate_name(base).map_err(|m| OpError { path: parent.display().to_string(), message: m, kind: ErrKind::Invalid })?;
    if !parent.is_dir() {
        return Err(OpError { path: parent.display().to_string(), message: "La carpeta de destino no existe.".into(), kind: ErrKind::NotFound });
    }
    let (stem, ext) = match (is_dir, base.rfind('.')) {
        (false, Some(i)) if i > 0 => (&base[..i], &base[i..]),
        _ => (base, ""),
    };
    for i in 1..10_000 {
        let name = if i == 1 { base.to_string() } else { format!("{stem} ({i}){ext}") };
        let p = parent.join(&name);
        let r = if is_dir { fs::create_dir(&p) } else { OpenOptions::new().write(true).create_new(true).open(&p).map(|_| ()) };
        match r {
            Ok(()) => return Ok(p),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(op_error(&p, &e)),
        }
    }
    Err(OpError { path: parent.display().to_string(), message: "No se pudo generar un nombre libre.".into(), kind: ErrKind::Exists })
}

pub fn create_folder(parent: &Path, base: &str) -> Result<(PathBuf, UndoEntry), OpError> {
    let p = create_unique(parent, base, true)?;
    Ok((p.clone(), UndoEntry::Create { path: p }))
}

pub fn create_file(parent: &Path, base: &str) -> Result<(PathBuf, UndoEntry), OpError> {
    let p = create_unique(parent, base, false)?;
    Ok((p.clone(), UndoEntry::Create { path: p }))
}

// ───────────────────────── pruebas ─────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::undo;
    use std::sync::atomic::AtomicUsize;

    static N: AtomicUsize = AtomicUsize::new(0);
    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nova-ops-{}-{}-{tag}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }
    fn w(p: &Path, s: &str) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, s).unwrap();
    }
    fn run<F>(f: F, decide: Decision) -> (OpOutcome, Vec<Progress>)
    where
        F: FnOnce(&mut Ctx) -> OpOutcome,
    {
        let cancel = AtomicBool::new(false);
        let mut log = vec![];
        let mut p = |x: &Progress| log.push(x.clone());
        let mut c = |_: &ConflictInfo| decide;
        let mut ctx = Ctx { cancel: &cancel, paused: None, on_progress: &mut p, on_conflict: &mut c };
        let o = f(&mut ctx);
        (o, log)
    }

    #[test]
    fn copy_tree_preserves_content_and_reports_progress() {
        let d = tmp("copy");
        w(&d.join("src/a.txt"), "hola");
        w(&d.join("src/sub/b.txt"), "mundo");
        fs::create_dir_all(d.join("dst")).unwrap();
        let (o, log) = run(|c| copy_items(&[d.join("src")], &d.join("dst"), c), Decision::Skip);
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        assert_eq!(fs::read_to_string(d.join("dst/src/sub/b.txt")).unwrap(), "mundo");
        assert_eq!(o.completed, 1);
        let last = log.last().unwrap();
        assert_eq!((last.files_done, last.files_total, last.bytes_done, last.bytes_total), (2, 2, 9, 9));
        assert!(matches!(o.undo, Some(UndoEntry::Copy { .. })));
        assert!(d.join("src/a.txt").exists(), "el origen no se toca");
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn conflict_decisions() {
        for (dec, expect_dst, expect_extra, skipped) in [
            (Decision::Replace, "NUEVO", false, 0),
            (Decision::Skip, "viejo", false, 1),
            (Decision::KeepBoth, "viejo", true, 0),
        ] {
            let d = tmp("conf");
            w(&d.join("o/f.txt"), "NUEVO");
            w(&d.join("d/f.txt"), "viejo");
            let (o, _) = run(|c| copy_items(&[d.join("o/f.txt")], &d.join("d"), c), dec);
            assert!(o.errors.is_empty(), "{dec:?} {:?}", o.errors);
            assert_eq!(fs::read_to_string(d.join("d/f.txt")).unwrap(), expect_dst, "{dec:?}");
            assert_eq!(d.join("d/f (2).txt").exists(), expect_extra, "{dec:?}");
            assert_eq!(o.skipped, skipped);
            let leftovers = fs::read_dir(d.join("d")).unwrap().filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().ends_with(".tmp")).count();
            assert_eq!(leftovers, 0, "no deben quedar temporales");
            let _ = fs::remove_dir_all(d);
        }
    }

    #[test]
    fn conflict_cancel_stops_operation() {
        let d = tmp("cc");
        w(&d.join("o/f.txt"), "N");
        w(&d.join("d/f.txt"), "V");
        let (o, _) = run(|c| copy_items(&[d.join("o/f.txt")], &d.join("d"), c), Decision::Cancel);
        assert_eq!(fs::read_to_string(d.join("d/f.txt")).unwrap(), "V");
        let _ = o;
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn copy_in_same_folder_makes_numbered_copy() {
        let d = tmp("same");
        w(&d.join("f.txt"), "x");
        let (o, _) = run(|c| copy_items(&[d.join("f.txt")], &d, c), Decision::Skip);
        assert!(o.errors.is_empty());
        assert!(d.join("f (2).txt").exists());
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn cannot_copy_folder_into_itself() {
        let d = tmp("self");
        w(&d.join("a/x.txt"), "1");
        let (o, _) = run(|c| copy_items(&[d.join("a")], &d.join("a"), c), Decision::Skip);
        assert_eq!(o.errors.len(), 1);
        assert_eq!(o.errors[0].kind, ErrKind::Invalid);
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn missing_destination_is_an_error() {
        let d = tmp("nodest");
        w(&d.join("f"), "1");
        let (o, _) = run(|c| copy_items(&[d.join("f")], &d.join("no-existe"), c), Decision::Skip);
        assert_eq!(o.errors[0].kind, ErrKind::NotFound);
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn large_file_uses_chunked_copy_with_progress_and_mtime() {
        let d = tmp("big");
        let data = vec![7u8; (FAST_COPY_MAX as usize) + 3 * BUF + 17];
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("big.bin"), &data).unwrap();
        let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_500_000_000);
        File::options().write(true).open(d.join("big.bin")).unwrap().set_modified(old).unwrap();
        fs::create_dir_all(d.join("out")).unwrap();
        let (o, log) = run(|c| copy_items(&[d.join("big.bin")], &d.join("out"), c), Decision::Skip);
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        assert_eq!(fs::read(d.join("out/big.bin")).unwrap(), data);
        assert!(log.len() >= 4, "progreso por bloques: {}", log.len());
        assert!(log.windows(2).all(|p| p[0].bytes_done <= p[1].bytes_done), "el progreso no retrocede");
        assert_eq!(fs::metadata(d.join("out/big.bin")).unwrap().modified().unwrap(), old, "conserva la fecha de modificación");
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn cancel_mid_copy_leaves_no_partial_or_temp_files() {
        let d = tmp("cancel");
        fs::create_dir_all(d.join("out")).unwrap();
        fs::write(d.join("big.bin"), vec![1u8; (FAST_COPY_MAX as usize) + 4 * BUF]).unwrap();
        let cancel = AtomicBool::new(false);
        let mut p = |x: &Progress| {
            if x.bytes_done > 0 {
                cancel.store(true, Ordering::Relaxed);
            }
        };
        let mut c = |_: &ConflictInfo| Decision::Skip;
        let mut ctx = Ctx { cancel: &cancel, paused: None, on_progress: &mut p, on_conflict: &mut c };
        let o = copy_items(&[d.join("big.bin")], &d.join("out"), &mut ctx);
        assert!(o.cancelled);
        assert_eq!(fs::read_dir(d.join("out")).unwrap().count(), 0, "sin archivo parcial ni temporal");
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn move_renames_and_undo_restores() {
        let d = tmp("mv");
        w(&d.join("a/f.txt"), "1");
        fs::create_dir_all(d.join("b")).unwrap();
        let (o, _) = run(|c| move_items(&[d.join("a/f.txt")], &d.join("b"), c), Decision::Skip);
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        assert!(!d.join("a/f.txt").exists() && d.join("b/f.txt").exists());
        let rep = undo::apply(o.undo.as_ref().unwrap(), &|_| Ok(()));
        assert!(rep.errors.is_empty(), "{:?}", rep.errors);
        assert!(d.join("a/f.txt").exists() && !d.join("b/f.txt").exists());
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn move_merges_folders_and_resolves_conflicts() {
        let d = tmp("merge");
        w(&d.join("a/same.txt"), "NUEVO");
        w(&d.join("a/only.txt"), "solo");
        w(&d.join("b/a/same.txt"), "viejo");
        let (o, _) = run(|c| move_items(&[d.join("a")], &d.join("b"), c), Decision::Replace);
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        assert_eq!(fs::read_to_string(d.join("b/a/same.txt")).unwrap(), "NUEVO");
        assert!(d.join("b/a/only.txt").exists());
        assert!(!d.join("a").exists(), "la carpeta de origen vacía se elimina");
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn move_refuses_into_itself_and_same_folder() {
        let d = tmp("mvself");
        w(&d.join("a/x"), "1");
        let (o, _) = run(|c| move_items(&[d.join("a")], &d.join("a"), c), Decision::Skip);
        assert_eq!(o.errors[0].kind, ErrKind::Invalid);
        let (o, _) = run(|c| move_items(&[d.join("a")], &d, c), Decision::Skip);
        assert_eq!(o.errors[0].kind, ErrKind::Invalid, "ya está en esa carpeta");
        let _ = fs::remove_dir_all(d);
    }

    #[cfg(unix)]
    #[test]
    fn destructive_ops_are_blocked_on_system_paths() {
        let (o, _) = run(|c| delete_items(&[PathBuf::from("/usr")], c), Decision::Skip);
        assert_eq!(o.errors[0].kind, ErrKind::Blocked);
        let (o, _) = run(|c| move_items(&[PathBuf::from("/etc")], &std::env::temp_dir(), c), Decision::Skip);
        assert_eq!(o.errors[0].kind, ErrKind::Blocked);
        assert_eq!(rename_item(Path::new("/usr"), "x").unwrap_err().kind, ErrKind::Blocked);
    }

    #[test]
    fn delete_removes_trees_and_counts_progress() {
        let d = tmp("del");
        w(&d.join("t/a.txt"), "1");
        w(&d.join("t/s/b.txt"), "22");
        let (o, log) = run(|c| delete_items(&[d.join("t")], c), Decision::Skip);
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        assert!(!d.join("t").exists());
        assert_eq!(log.last().unwrap().files_done, 2);
        assert!(o.undo.is_none(), "eliminar permanentemente no es reversible");
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn trash_batches_and_attributes_errors_to_the_right_item() {
        let d = tmp("trash");
        let names: Vec<PathBuf> = (0..150).map(|i| d.join(format!("f{i}.txt"))).collect();
        for p in &names {
            w(p, "x");
        }
        let bad = names[70].clone();
        let (mut batches, mut singles) = (vec![], 0);
        let (o, log) = run(
            |c| {
                trash_items(
                    &names,
                    c,
                    &mut |chunk| {
                        batches.push(chunk.len());
                        if chunk.contains(&bad) { Err("lote falló".into()) } else { chunk.iter().for_each(|p| fs::remove_file(p).unwrap()); Ok(()) }
                    },
                    &mut |p| {
                        singles += 1;
                        if p == bad { Err("Acceso denegado".into()) } else { fs::remove_file(p).map_err(|e| e.to_string()) }
                    },
                )
            },
            Decision::Skip,
        );
        assert_eq!(batches, vec![64, 64, 22]);
        assert_eq!(singles, 64, "solo el lote que falló se reintenta uno a uno");
        assert_eq!(o.completed, 149);
        assert_eq!(o.errors.len(), 1);
        assert_eq!(o.errors[0].path, bad.display().to_string());
        assert_eq!(log.last().unwrap().files_done, 150);
        assert!(bad.exists() && !names[0].exists());
        let _ = fs::remove_dir_all(d);
    }

    /// Como Windows: el lote envía todo lo que puede y luego informa del fallo. Lo enviado no puede contar como error.
    #[test]
    fn trash_batch_that_fails_halfway_counts_what_was_sent() {
        let d = tmp("trash3");
        let names: Vec<PathBuf> = (0..6).map(|i| d.join(format!("f{i}.txt"))).collect();
        names.iter().for_each(|p| w(p, "x"));
        let bad = names[3].clone();
        let mut singles = vec![];
        let (o, _) = run(
            |c| {
                trash_items(
                    &names,
                    c,
                    &mut |chunk| {
                        chunk.iter().filter(|p| **p != bad).for_each(|p| fs::remove_file(p).unwrap());
                        Err("Some operations were aborted".into())
                    },
                    &mut |p| {
                        singles.push(p.to_path_buf());
                        Err("en uso".into())
                    },
                )
            },
            Decision::Skip,
        );
        assert_eq!(o.completed, 5);
        assert_eq!(o.errors.len(), 1);
        assert_eq!(o.errors[0].path, bad.display().to_string());
        assert_eq!(singles, vec![bad], "solo se reintenta lo que sigue en su sitio");
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn trash_reports_missing_items_without_calling_the_system_and_honours_cancel() {
        let d = tmp("trash2");
        let (o, _) = run(|c| trash_items(&[d.join("no-existe")], c, &mut |_| panic!("no debe llamarse"), &mut |_| panic!("no debe llamarse")), Decision::Skip);
        assert_eq!(o.errors[0].kind, ErrKind::NotFound);
        let cancel = AtomicBool::new(true);
        let mut p = |_: &Progress| {};
        let mut c = |_: &ConflictInfo| Decision::Skip;
        let mut ctx = Ctx { cancel: &cancel, paused: None, on_progress: &mut p, on_conflict: &mut c };
        w(&d.join("a"), "1");
        let o = trash_items(&[d.join("a")], &mut ctx, &mut |_| panic!("cancelado"), &mut |_| panic!("cancelado"));
        assert!(o.cancelled && d.join("a").exists());
        let _ = fs::remove_dir_all(d);
    }

    fn copy_while(paused: bool, act: impl FnOnce(&AtomicBool, &AtomicBool, &Path) + Send) -> (OpOutcome, PathBuf) {
        let d = tmp(if paused { "pausa" } else { "pausa2" });
        w(&d.join("o/a.txt"), "1");
        w(&d.join("o/b.txt"), "2");
        fs::create_dir_all(d.join("dst")).unwrap();
        let (cancel, pause) = (AtomicBool::new(false), AtomicBool::new(paused));
        let out = std::thread::scope(|s| {
            let job = s.spawn(|| {
                let mut p = |_: &Progress| {};
                let mut c = |_: &ConflictInfo| Decision::Skip;
                let mut ctx = Ctx { cancel: &cancel, paused: Some(&pause), on_progress: &mut p, on_conflict: &mut c };
                copy_items(&[d.join("o/a.txt"), d.join("o/b.txt")], &d.join("dst"), &mut ctx)
            });
            act(&cancel, &pause, &d);
            job.join().unwrap()
        });
        (out, d)
    }

    #[test]
    fn a_paused_copy_writes_nothing_until_resumed() {
        let (out, d) = copy_while(true, |_, pause, dir| {
            std::thread::sleep(std::time::Duration::from_millis(300));
            assert_eq!(fs::read_dir(dir.join("dst")).unwrap().count(), 0, "en pausa no se escribe nada");
            pause.store(false, Ordering::Relaxed);
        });
        assert_eq!((out.completed, out.cancelled), (2, false));
        assert!(d.join("dst/a.txt").exists() && d.join("dst/b.txt").exists());
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn cancelling_a_paused_copy_ends_it_at_once() {
        let (out, d) = copy_while(true, |cancel, _, _| {
            std::thread::sleep(std::time::Duration::from_millis(150));
            cancel.store(true, Ordering::Relaxed);
        });
        assert!(out.cancelled);
        assert_eq!(fs::read_dir(d.join("dst")).unwrap().count(), 0, "no deja nada a medias");
        let _ = fs::remove_dir_all(d);
    }

    #[cfg(unix)]
    #[test]
    fn delete_never_follows_symlinks() {
        let d = tmp("dellink");
        w(&d.join("real/keep.txt"), "no tocar");
        fs::create_dir_all(d.join("t")).unwrap();
        std::os::unix::fs::symlink(d.join("real"), d.join("t/enlace")).unwrap();
        let (o, _) = run(|c| delete_items(&[d.join("t")], c), Decision::Skip);
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        assert!(d.join("real/keep.txt").exists(), "el contenido del destino del enlace sigue intacto");
        let _ = fs::remove_dir_all(d);
    }

    #[cfg(unix)]
    #[test]
    fn copy_skips_directory_symlinks() {
        let d = tmp("copylink");
        w(&d.join("real/x.txt"), "1");
        fs::create_dir_all(d.join("t")).unwrap();
        std::os::unix::fs::symlink(d.join("real"), d.join("t/enlace")).unwrap();
        fs::create_dir_all(d.join("out")).unwrap();
        let (o, _) = run(|c| copy_items(&[d.join("t")], &d.join("out"), c), Decision::Skip);
        assert_eq!(o.skipped, 1);
        assert!(!d.join("out/t/enlace").exists());
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn rename_validates_and_undoes() {
        let d = tmp("ren");
        w(&d.join("a.txt"), "1");
        w(&d.join("b.txt"), "2");
        assert_eq!(rename_item(&d.join("a.txt"), "b.txt").unwrap_err().kind, ErrKind::Exists);
        assert_eq!(rename_item(&d.join("a.txt"), "x:y").unwrap_err().kind, ErrKind::Invalid);
        assert_eq!(rename_item(&d.join("a.txt"), "a.txt").unwrap_err().kind, ErrKind::Invalid);
        let (new, entry) = rename_item(&d.join("a.txt"), "c.txt").unwrap();
        assert_eq!(new, d.join("c.txt"));
        assert!(undo::apply(&entry, &|_| Ok(())).errors.is_empty());
        assert!(d.join("a.txt").exists() && !d.join("c.txt").exists());
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn undo_refuses_when_original_is_occupied() {
        let d = tmp("undoocc");
        w(&d.join("a.txt"), "1");
        let (_, entry) = rename_item(&d.join("a.txt"), "c.txt").unwrap();
        w(&d.join("a.txt"), "otro");
        let rep = undo::apply(&entry, &|_| Ok(()));
        assert_eq!(rep.errors[0].kind, ErrKind::Exists);
        assert_eq!(fs::read_to_string(d.join("a.txt")).unwrap(), "otro", "no se pisa nada");
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn undo_copy_sends_only_created_items_to_trash() {
        let d = tmp("undocopy");
        w(&d.join("src/a.txt"), "1");
        w(&d.join("dst/src/viejo.txt"), "ya existía");
        fs::create_dir_all(d.join("dst")).unwrap();
        let (o, _) = run(|c| copy_items(&[d.join("src")], &d.join("dst"), c), Decision::Skip);
        let trashed = std::cell::RefCell::new(vec![]);
        let rep = undo::apply(o.undo.as_ref().unwrap(), &|p| {
            trashed.borrow_mut().push(p.to_path_buf());
            Ok(())
        });
        assert!(rep.errors.is_empty());
        assert_eq!(*trashed.borrow(), vec![d.join("dst/src/a.txt")], "la carpeta preexistente no se toca");
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn create_folder_and_file_get_unique_names() {
        let d = tmp("create");
        let (a, _) = create_folder(&d, "Nueva carpeta").unwrap();
        let (b, _) = create_folder(&d, "Nueva carpeta").unwrap();
        assert_eq!((a.file_name().unwrap().to_str().unwrap(), b.file_name().unwrap().to_str().unwrap()), ("Nueva carpeta", "Nueva carpeta (2)"));
        let (f1, _) = create_file(&d, "Nuevo documento.txt").unwrap();
        let (f2, _) = create_file(&d, "Nuevo documento.txt").unwrap();
        assert!(f1.is_file() && f2.ends_with("Nuevo documento (2).txt"));
        assert_eq!(create_folder(&d, "con*raro").unwrap_err().kind, ErrKind::Invalid);
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn scan_counts_files_and_bytes() {
        let d = tmp("scan");
        w(&d.join("a"), "123");
        w(&d.join("s/b"), "45");
        assert_eq!(scan(&[d.clone()], &AtomicBool::new(false)), (2, 5));
        let _ = fs::remove_dir_all(d);
    }
}
