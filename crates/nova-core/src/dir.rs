//! Lectura incremental y cancelable de directorios.
//!
//! Diseño:
//! * Un único recorrido (`read_dir`), sin escaneos recursivos.
//! * Los resultados salen por lotes: uno pequeño primero (primer pintado
//!   inmediato) y lotes grandes después (menos coste de IPC).
//! * La bandera `cancel` se consulta en cada entrada; una navegación nueva
//!   detiene la lectura obsoleta sin esperar a que termine.
//! * Solo se envía el nombre; la UI compone la ruta completa.

use serde::Serialize;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::UNIX_EPOCH;

/// Tamaño del primer lote: llega rápido a pantalla.
pub const FIRST_BATCH: usize = 64;
/// Tamaño del resto de lotes.
pub const BATCH: usize = 1500;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    File,
    Dir,
    Other,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    pub kind: EntryKind,
    /// Bytes. 0 para carpetas.
    pub size: u64,
    /// Milisegundos desde epoch Unix. `None` si el sistema no lo informa.
    pub modified: Option<i64>,
    /// Fecha de creación, si el sistema la informa.
    pub created: Option<i64>,
    pub hidden: bool,
    /// Extensión en minúsculas, sin punto. Solo en archivos.
    pub ext: Option<String>,
    /// `true` si es un enlace simbólico o unión (kind refleja el destino).
    pub link: bool,
    /// `true` si el contenido solo está en la nube: leerlo provocaría una descarga.
    pub cloud: bool,
}

/// Atributos de Windows de un archivo cuyo contenido no está en el disco.
pub(crate) const fn attrs_cloud_only(attrs: u32) -> bool {
    const DIRECTORY: u32 = 0x10;
    const OFFLINE: u32 = 0x1000;
    const RECALL_ON_OPEN: u32 = 0x4_0000;
    const RECALL_ON_DATA_ACCESS: u32 = 0x40_0000;
    attrs & DIRECTORY == 0 && attrs & (OFFLINE | RECALL_ON_OPEN | RECALL_ON_DATA_ACCESS) != 0
}

/// ¿Leer este archivo obligaría a descargarlo? (OneDrive «disponible solo en línea» y similares). Consultar los atributos no descarga nada.
#[cfg(windows)]
pub fn is_cloud_only(meta: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    attrs_cloud_only(meta.file_attributes())
}

#[cfg(not(windows))]
pub fn is_cloud_only(_meta: &fs::Metadata) -> bool {
    false
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadStats {
    pub total: usize,
    /// Entradas que no se pudieron leer (se omiten).
    pub skipped: usize,
    pub cancelled: bool,
}

#[cfg(windows)]
fn is_hidden(name: &str, meta: Option<&fs::Metadata>) -> bool {
    use std::os::windows::fs::MetadataExt;
    const HIDDEN: u32 = 0x2;
    const SYSTEM: u32 = 0x4;
    let _ = name;
    meta.map(|m| m.file_attributes() & (HIDDEN | SYSTEM) != 0)
        .unwrap_or(false)
}

#[cfg(not(windows))]
fn is_hidden(name: &str, _meta: Option<&fs::Metadata>) -> bool {
    name.starts_with('.')
}

fn build_entry(de: fs::DirEntry) -> Option<Entry> {
    let name = de.file_name().to_string_lossy().into_owned();
    let ft = de.file_type().ok()?;
    // En Windows, `DirEntry::metadata` no hace una llamada extra al disco.
    let meta = de.metadata().ok();

    let link = ft.is_symlink();
    let (kind, size_meta) = if link {
        match fs::metadata(de.path()) {
            Ok(m) if m.is_dir() => (EntryKind::Dir, Some(m)),
            Ok(m) => (EntryKind::File, Some(m)),
            Err(_) => (EntryKind::Other, None),
        }
    } else if ft.is_dir() {
        (EntryKind::Dir, meta.clone())
    } else if ft.is_file() {
        (EntryKind::File, meta.clone())
    } else {
        (EntryKind::Other, meta.clone())
    };

    let hidden = is_hidden(&name, meta.as_ref());
    let size = if kind == EntryKind::File {
        size_meta.as_ref().map(|m| m.len()).unwrap_or(0)
    } else {
        0
    };
    let modified = size_meta
        .as_ref()
        .or(meta.as_ref())
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64);
    let ext = if kind == EntryKind::File {
        Path::new(&name)
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
    } else {
        None
    };

    let created = size_meta
        .as_ref()
        .or(meta.as_ref())
        .and_then(|m| m.created().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64);

    let cloud = meta.as_ref().is_some_and(is_cloud_only);
    Some(Entry { name, kind, size, modified, created, hidden, ext, link, cloud })
}

/// Lee `path` y entrega las entradas por lotes mediante `emit`.
///
/// Devuelve `Err` solo si no se puede abrir el directorio. Las entradas
/// individuales ilegibles se cuentan en `skipped`.
pub fn read_dir_batched<F>(
    path: &Path,
    first_batch: usize,
    batch: usize,
    cancel: &AtomicBool,
    mut emit: F,
) -> io::Result<ReadStats>
where
    F: FnMut(Vec<Entry>),
{
    let iter = fs::read_dir(path)?;
    let first_batch = first_batch.max(1);
    let batch = batch.max(1);
    let mut cur: Vec<Entry> = Vec::with_capacity(first_batch);
    let mut limit = first_batch;
    let mut total = 0usize;
    let mut skipped = 0usize;

    for item in iter {
        if cancel.load(Ordering::Relaxed) {
            return Ok(ReadStats { total, skipped, cancelled: true });
        }
        match item.ok().and_then(build_entry) {
            Some(e) => {
                cur.push(e);
                total += 1;
                if cur.len() >= limit {
                    emit(std::mem::replace(&mut cur, Vec::with_capacity(batch)));
                    limit = batch;
                }
            }
            None => skipped += 1,
        }
    }
    if !cur.is_empty() {
        emit(cur);
    }
    Ok(ReadStats { total, skipped, cancelled: false })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("nova-core-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn reads_files_and_dirs_with_metadata() {
        let d = tmp("basic");
        fs::write(d.join("a.TXT"), b"hola").unwrap();
        fs::create_dir(d.join("sub")).unwrap();
        let mut all = vec![];
        let st = read_dir_batched(&d, 8, 8, &AtomicBool::new(false), |b| all.extend(b)).unwrap();
        assert_eq!(st.total, 2);
        assert!(!st.cancelled);
        let f = all.iter().find(|e| e.name == "a.TXT").unwrap();
        assert_eq!(f.kind, EntryKind::File);
        assert_eq!(f.size, 4);
        assert_eq!(f.ext.as_deref(), Some("txt"));
        assert!(f.modified.is_some());
        let s = all.iter().find(|e| e.name == "sub").unwrap();
        assert_eq!(s.kind, EntryKind::Dir);
        assert_eq!(s.ext, None);
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn batches_first_small_then_large() {
        let d = tmp("batches");
        for i in 0..150 {
            fs::write(d.join(format!("f{i}.dat")), b"x").unwrap();
        }
        let mut sizes = vec![];
        let st = read_dir_batched(&d, 10, 50, &AtomicBool::new(false), |b| sizes.push(b.len())).unwrap();
        assert_eq!(st.total, 150);
        assert_eq!(sizes, vec![10, 50, 50, 40]);
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn cancel_stops_reading() {
        let d = tmp("cancel");
        for i in 0..50 {
            fs::write(d.join(format!("f{i}")), b"x").unwrap();
        }
        let mut n = 0;
        let st = read_dir_batched(&d, 5, 5, &AtomicBool::new(true), |b| n += b.len()).unwrap();
        assert!(st.cancelled);
        assert_eq!(n, 0);
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn missing_dir_is_error() {
        let r = read_dir_batched(Path::new("/definitivamente/no/existe"), 4, 4, &AtomicBool::new(false), |_| {});
        assert!(r.is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_to_dir_reports_dir_and_link() {
        let d = tmp("link");
        fs::create_dir(d.join("real")).unwrap();
        std::os::unix::fs::symlink(d.join("real"), d.join("enlace")).unwrap();
        let mut all = vec![];
        read_dir_batched(&d, 8, 8, &AtomicBool::new(false), |b| all.extend(b)).unwrap();
        let l = all.iter().find(|e| e.name == "enlace").unwrap();
        assert!(l.link);
        assert_eq!(l.kind, EntryKind::Dir);
        let _ = fs::remove_dir_all(d);
    }

    #[cfg(unix)]
    #[test]
    fn dotfiles_are_hidden_on_unix() {
        let d = tmp("hidden");
        fs::write(d.join(".oculto"), b"").unwrap();
        fs::write(d.join("visible"), b"").unwrap();
        let mut all = vec![];
        read_dir_batched(&d, 8, 8, &AtomicBool::new(false), |b| all.extend(b)).unwrap();
        assert!(all.iter().find(|e| e.name == ".oculto").unwrap().hidden);
        assert!(!all.iter().find(|e| e.name == "visible").unwrap().hidden);
        let _ = fs::remove_dir_all(d);
    }
}
