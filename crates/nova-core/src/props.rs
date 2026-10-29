//! Propiedades de archivos y carpetas, y estadísticas recursivas (tamaño, nº de elementos).

use serde::Serialize;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

fn ms(t: io::Result<SystemTime>) -> Option<i64> {
    t.ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64)
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FileProps {
    pub name: String,
    pub path: String,
    pub parent: Option<String>,
    pub is_dir: bool,
    pub size: u64,
    pub created: Option<i64>,
    pub modified: Option<i64>,
    pub accessed: Option<i64>,
    pub readonly: bool,
    pub hidden: bool,
    pub system: bool,
    pub link: bool,
    pub link_target: Option<String>,
    pub ext: Option<String>,
}

pub fn file_props(path: &Path) -> io::Result<FileProps> {
    let lm = fs::symlink_metadata(path)?;
    let link = lm.file_type().is_symlink();
    let m = if link { fs::metadata(path).unwrap_or_else(|_| lm.clone()) } else { lm.clone() };
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string());
    #[cfg(windows)]
    let (hidden, system) = {
        use std::os::windows::fs::MetadataExt;
        let a = lm.file_attributes();
        (a & 0x2 != 0, a & 0x4 != 0)
    };
    #[cfg(not(windows))]
    let (hidden, system) = (name.starts_with('.'), false);
    Ok(FileProps {
        ext: if m.is_dir() { None } else { path.extension().map(|e| e.to_string_lossy().to_lowercase()) },
        name,
        path: path.display().to_string(),
        parent: path.parent().map(|p| p.display().to_string()),
        is_dir: m.is_dir(),
        size: if m.is_dir() { 0 } else { m.len() },
        created: ms(m.created()),
        modified: ms(m.modified()),
        accessed: ms(m.accessed()),
        readonly: m.permissions().readonly(),
        hidden,
        system,
        link,
        link_target: if link { fs::read_link(path).ok().map(|t| t.display().to_string()) } else { None },
    })
}

#[derive(Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub files: u64,
    pub dirs: u64,
    pub bytes: u64,
    /// Elementos que no se pudieron leer (permisos, etc.).
    pub errors: u64,
}

/// Recorre `paths` sin seguir enlaces. `on_progress` se llama como mucho cada ~100 ms.
/// Devuelve (estadísticas, cancelado).
pub fn dir_stats(paths: &[PathBuf], cancel: &AtomicBool, on_progress: &mut dyn FnMut(&Stats)) -> (Stats, bool) {
    let mut s = Stats::default();
    let mut stack: Vec<PathBuf> = paths.to_vec();
    let mut last = Instant::now();
    while let Some(p) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            return (s, true);
        }
        match fs::symlink_metadata(&p) {
            Err(_) => s.errors += 1,
            Ok(m) if m.is_dir() => {
                s.dirs += 1;
                match fs::read_dir(&p) {
                    Ok(rd) => stack.extend(rd.flatten().map(|e| e.path())),
                    Err(_) => s.errors += 1,
                }
            }
            Ok(m) => {
                s.files += 1;
                s.bytes += m.len();
            }
        }
        if last.elapsed().as_millis() >= 100 {
            on_progress(&s);
            last = Instant::now();
        }
    }
    on_progress(&s);
    (s, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn props_of_file_and_folder() {
        let d = std::env::temp_dir().join(format!("nova-props-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(d.join("sub")).unwrap();
        fs::write(d.join("a.TXT"), b"hola").unwrap();
        let f = file_props(&d.join("a.TXT")).unwrap();
        assert_eq!((f.size, f.is_dir, f.ext.as_deref()), (4, false, Some("txt")));
        assert!(f.modified.is_some());
        assert_eq!(f.parent.as_deref(), Some(d.to_str().unwrap()));
        assert!(file_props(&d.join("sub")).unwrap().is_dir);
        assert!(file_props(&d.join("nada")).is_err());
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn stats_count_recursively_and_cancel() {
        let d = std::env::temp_dir().join(format!("nova-stats-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(d.join("s/t")).unwrap();
        fs::write(d.join("a"), b"123").unwrap();
        fs::write(d.join("s/b"), b"45").unwrap();
        fs::write(d.join("s/t/c"), b"6").unwrap();
        let (s, c) = dir_stats(&[d.clone()], &AtomicBool::new(false), &mut |_| {});
        assert!(!c);
        assert_eq!((s.files, s.dirs, s.bytes), (3, 3, 6));
        let (_, c) = dir_stats(&[d.clone()], &AtomicBool::new(true), &mut |_| {});
        assert!(c);
        let _ = fs::remove_dir_all(d);
    }
}
