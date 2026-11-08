//! Protección del sistema y validación de rutas recibidas desde la interfaz.
//! Todo lo que llega por IPC se trata como no confiable.

use std::path::{Component, Path, PathBuf};

/// Convierte el texto recibido en una ruta absoluta y limpia. Rechaza `..`, NUL y rutas relativas.
pub fn parse_path(s: &str) -> Result<PathBuf, String> {
    if s.is_empty() || s.contains('\0') {
        return Err("Ruta no válida.".into());
    }
    let p = PathBuf::from(s);
    if !p.is_absolute() {
        return Err(format!("La ruta debe ser absoluta: {s}"));
    }
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir => return Err("Las rutas con «..» no están permitidas.".into()),
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    Ok(out)
}

fn strip_verbatim(s: String) -> String {
    s.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(s)
}

/// Componentes comparables: en Windows sin distinguir mayúsculas y sin prefijo `\\?\`.
fn parts(p: &Path) -> Vec<String> {
    let s = strip_verbatim(p.to_string_lossy().into_owned());
    let norm = |c: &str| if cfg!(windows) { c.to_lowercase() } else { c.to_string() };
    s.split(['\\', '/']).filter(|c| !c.is_empty()).map(norm).collect()
}

/// ¿`child` es `parent` o está dentro de `parent`?
pub fn contains(parent: &Path, child: &Path) -> bool {
    let (a, b) = (parts(parent), parts(child));
    b.len() >= a.len() && a.iter().zip(b.iter()).all(|(x, y)| x == y)
}

pub fn same_path(a: &Path, b: &Path) -> bool {
    parts(a) == parts(b)
}

pub fn is_root(p: &Path) -> bool {
    let n = parts(p).len();
    if cfg!(windows) { n <= 1 } else { n == 0 }
}

fn env_path(k: &str) -> Option<PathBuf> {
    std::env::var_os(k).map(PathBuf::from).filter(|p| p.is_absolute())
}

/// (carpetas protegidas solo ellas mismas, árboles protegidos con todo su contenido)
fn protected() -> (Vec<PathBuf>, Vec<PathBuf>) {
    if cfg!(windows) {
        let mut exact: Vec<PathBuf> = ["ProgramData", "USERPROFILE", "PUBLIC", "ALLUSERSPROFILE"].iter().filter_map(|k| env_path(k)).collect();
        if let Some(d) = env_path("SystemDrive").or_else(|| env_path("SystemRoot").and_then(|r| r.ancestors().last().map(Path::to_path_buf))) {
            let root = PathBuf::from(format!("{}\\", d.to_string_lossy().trim_end_matches(['\\', '/'])));
            exact.push(root.join("Users"));
        }
        let mut trees: Vec<PathBuf> = ["SystemRoot", "windir", "ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"].iter().filter_map(|k| env_path(k)).collect();
        trees.sort();
        trees.dedup();
        (exact, trees)
    } else {
        let exact = ["/home", "/root", "/var", "/opt", "/srv", "/mnt", "/media", "/tmp"].iter().map(PathBuf::from).collect();
        let trees = ["/bin", "/boot", "/dev", "/etc", "/lib", "/lib64", "/proc", "/sbin", "/sys", "/usr"].iter().map(PathBuf::from).collect();
        (exact, trees)
    }
}

/// Operaciones que modifican o destruyen (eliminar, mover, renombrar) no se permiten sobre
/// raíces de unidad ni carpetas del sistema, ni sobre su contenido en `Windows`/`Program Files`.
pub fn check_destructive(p: &Path) -> Result<(), String> {
    let blocked = |why: &str| Err(format!("Operación bloqueada por seguridad: {why}."));
    if is_root(p) {
        return blocked("es la raíz de una unidad");
    }
    let (exact, trees) = protected();
    if exact.iter().any(|e| same_path(e, p)) {
        return blocked("es una carpeta del sistema o de perfil de usuario");
    }
    if trees.iter().any(|t| contains(t, p)) {
        return blocked("pertenece a una carpeta del sistema");
    }
    let low = parts(p);
    if cfg!(windows) && low.iter().any(|c| c == "$recycle.bin" || c == "system volume information") {
        return blocked("es una carpeta interna del sistema");
    }
    Ok(())
}

/// Impide copiar/mover una carpeta dentro de sí misma o de sus subcarpetas.
pub fn check_not_inside(dest_dir: &Path, src: &Path) -> Result<(), String> {
    // Se resuelven enlaces cuando es posible para que un enlace no pueda saltarse la comprobación.
    let d = std::fs::canonicalize(dest_dir).unwrap_or_else(|_| dest_dir.to_path_buf());
    let s = std::fs::canonicalize(src).unwrap_or_else(|_| src.to_path_buf());
    if contains(&s, &d) {
        Err("No se puede copiar ni mover una carpeta dentro de sí misma.".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rejects_bad_input() {
        // Las rutas de ejemplo deben ser absolutas EN ESTE SISTEMA: en Windows `/a/b` no lo es (no tiene unidad).
        // Si no, el caso de «..» pasaría por el motivo equivocado (rechazada por relativa).
        let (dotdot, dot, clean) = if cfg!(windows) { (r"C:\a\..\etc", r"C:\a\.\b", r"C:\a\b") } else { ("/a/../etc", "/a/./b", "/a/b") };
        assert!(parse_path("").is_err());
        assert!(parse_path("relativa/ruta").is_err());
        assert!(parse_path(dotdot).unwrap_err().contains("«..»"), "debe rechazarse por contener «..»");
        assert!(parse_path("/a\0b").is_err());
        assert_eq!(parse_path(dot).unwrap(), PathBuf::from(clean));
    }

    #[cfg(windows)]
    #[test]
    fn parse_handles_windows_path_forms() {
        // Relativas a la unidad o a la raíz actual: dependen del directorio de trabajo, no son seguras.
        for p in [r"/a/./b", r"\a\b", r"C:foo"] {
            assert!(parse_path(p).unwrap_err().contains("absoluta"), "{p}");
        }
        // «..» no puede colarse por ninguna forma de ruta absoluta.
        for p in [r"C:/a/../b", r"C:\a\b\..", r"\\srv\share\a\..\b", r"\\?\C:\a\..\b", r"\\?\UNC\srv\share\..\x"] {
            assert!(parse_path(p).unwrap_err().contains("«..»"), "{p}");
        }
        assert_eq!(parse_path(r"C:/a/./b").unwrap(), PathBuf::from(r"C:\a\b"));
        assert_eq!(parse_path(r"\\srv\share\a\.\b").unwrap(), PathBuf::from(r"\\srv\share\a\b"));
    }

    #[cfg(unix)]
    #[test]
    fn protects_system_paths_unix() {
        for p in ["/", "/usr", "/usr/lib/x", "/etc/passwd", "/home", "/bin/sh"] {
            assert!(check_destructive(Path::new(p)).is_err(), "{p}");
        }
        for p in ["/home/ana/docs", "/tmp/nova-x", "/mnt/datos/a"] {
            assert!(check_destructive(Path::new(p)).is_ok(), "{p}");
        }
    }

    #[test]
    fn containment() {
        assert!(contains(Path::new("/a/b"), Path::new("/a/b/c")));
        assert!(contains(Path::new("/a/b"), Path::new("/a/b")));
        assert!(!contains(Path::new("/a/b"), Path::new("/a/bc")));
        assert!(!contains(Path::new("/a/b/c"), Path::new("/a/b")));
    }

    #[test]
    fn cannot_move_folder_into_itself() {
        let d = std::env::temp_dir().join(format!("nova-safety-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("a/b")).unwrap();
        assert!(check_not_inside(&d.join("a/b"), &d.join("a")).is_err());
        assert!(check_not_inside(&d.join("a"), &d.join("a")).is_err());
        assert!(check_not_inside(&d, &d.join("a")).is_ok());
        let _ = std::fs::remove_dir_all(d);
    }
}
