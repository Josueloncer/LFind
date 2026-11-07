//! Comprimir y extraer con el `tar.exe` (bsdtar/libarchive) que incluye Windows 10 1803+. Sin extensiones COM.
//! Se usa la ruta absoluta `%SystemRoot%\System32\tar.exe`: el `tar` del PATH puede ser el GNU tar de Git, que no sabe crear zip.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

const ARCHIVE_EXT: &[&str] = &["zip", "7z", "rar", "tar", "gz", "tgz", "bz2", "xz", "iso", "cab"];

pub fn is_archive(name: &str) -> bool {
    name.rfind('.').filter(|&i| i > 0).map_or(false, |i| ARCHIVE_EXT.contains(&name[i + 1..].to_ascii_lowercase().as_str()))
}

fn tar_exe() -> PathBuf {
    #[cfg(windows)]
    {
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        Path::new(&root).join("System32").join("tar.exe")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("tar")
    }
}

/// Nombre del `.zip` a crear: el del elemento (si es uno) o el de la carpeta que los contiene.
pub fn zip_base_name(sources: &[PathBuf]) -> String {
    let name = |p: &Path| p.file_stem().or_else(|| p.file_name()).map(|s| s.to_string_lossy().into_owned());
    let n = if sources.len() == 1 {
        let s = &sources[0];
        // una carpeta conserva su nombre completo (p. ej. «datos.v2»); un archivo pierde la extensión
        if s.is_dir() { s.file_name().map(|x| x.to_string_lossy().into_owned()) } else { name(s) }
    } else {
        sources.first().and_then(|s| s.parent()).and_then(|p| p.file_name()).map(|x| x.to_string_lossy().into_owned())
    };
    format!("{}.zip", n.filter(|x| !x.is_empty()).unwrap_or_else(|| "Archivo".into()))
}

/// Carpeta de destino al extraer: el nombre del archivo sin extensión (`a.tar.gz` → `a`).
pub fn extract_dir_name(archive: &str) -> String {
    let mut n = archive.to_string();
    for _ in 0..2 {
        if let Some(i) = n.rfind('.').filter(|&i| i > 0) {
            if ARCHIVE_EXT.contains(&n[i + 1..].to_ascii_lowercase().as_str()) || n[i + 1..].eq_ignore_ascii_case("tar") {
                n.truncate(i);
                continue;
            }
        }
        break;
    }
    if n.is_empty() { "Extraído".into() } else { n }
}

/// Argumentos de `tar` para crear un zip. Los nombres van como `./nombre` para que uno que empiece por `-` no se tome por opción.
pub fn compress_args(zip: &Path, parent: &Path, names: &[OsString]) -> Vec<OsString> {
    let mut a: Vec<OsString> = vec!["-a".into(), "-c".into(), "-f".into(), zip.into(), "-C".into(), parent.into()];
    for n in names {
        let mut s = OsString::from("./");
        s.push(n);
        a.push(s);
    }
    a
}

pub fn extract_args(archive: &Path, dest: &Path) -> Vec<OsString> {
    vec!["-x".into(), "-f".into(), archive.into(), "-C".into(), dest.into()]
}

fn run_tar(args: &[OsString]) -> io::Result<()> {
    let mut c = std::process::Command::new(tar_exe());
    c.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = c.output().map_err(|e| io::Error::new(e.kind(), format!("No se pudo ejecutar tar.exe de Windows: {e}")))?;
    if out.status.success() {
        Ok(())
    } else {
        let msg = String::from_utf8_lossy(&out.stderr);
        Err(io::Error::new(io::ErrorKind::Other, format!("tar.exe falló: {}", msg.trim())))
    }
}

/// Comprime elementos de UNA misma carpeta `dir` en un `.zip` nuevo dentro de ella. Devuelve la ruta del zip.
pub fn compress(sources: &[PathBuf], dir: &Path) -> io::Result<PathBuf> {
    if sources.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "No hay nada que comprimir."));
    }
    let mut names = Vec::new();
    for s in sources {
        if s.parent() != Some(dir) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Los elementos deben estar en la carpeta abierta."));
        }
        names.push(s.file_name().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Ruta no válida."))?.to_os_string());
    }
    let zip = dir.join(crate::names::unique_name(dir, &zip_base_name(sources), false));
    if let Err(e) = run_tar(&compress_args(&zip, dir, &names)) {
        let _ = std::fs::remove_file(&zip); // no dejar un zip a medias
        return Err(e);
    }
    Ok(zip)
}

/// Extrae `archive` en una carpeta nueva junto a él. bsdtar rechaza por defecto rutas absolutas y `..` (zip-slip).
pub fn extract(archive: &Path) -> io::Result<PathBuf> {
    let parent = archive.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Ruta no válida."))?;
    let file = archive.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let dest = parent.join(crate::names::unique_name(parent, &extract_dir_name(&file), true));
    std::fs::create_dir(&dest)?;
    if let Err(e) = run_tar(&extract_args(archive, &dest)) {
        let _ = std::fs::remove_dir(&dest); // solo si quedó vacía
        return Err(e);
    }
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nombres() {
        assert!(is_archive("a.ZIP") && is_archive("a.tar.gz") && !is_archive("a.txt") && !is_archive(".zip"));
        assert_eq!(zip_base_name(&[PathBuf::from("/x/foto.png")]), "foto.zip");
        assert_eq!(zip_base_name(&[PathBuf::from("/x/a"), PathBuf::from("/x/b")]), "x.zip");
        assert_eq!(zip_base_name(&[PathBuf::from("/")]), "Archivo.zip");
        assert_eq!(extract_dir_name("a.tar.gz"), "a");
        assert_eq!(extract_dir_name("datos.v2.zip"), "datos.v2");
        assert_eq!(extract_dir_name("x.zip"), "x");
    }

    #[test]
    fn argumentos() {
        let a = compress_args(Path::new("C:\\d\\o.zip"), Path::new("C:\\d"), &["-x".into(), "b c".into()]);
        let s: Vec<String> = a.iter().map(|x| x.to_string_lossy().into_owned()).collect();
        assert_eq!(s, ["-a", "-c", "-f", "C:\\d\\o.zip", "-C", "C:\\d", "./-x", "./b c"]);
        assert_eq!(extract_args(Path::new("a.zip"), Path::new("d")).len(), 5);
    }

    #[test]
    fn rechaza_elementos_de_otra_carpeta() {
        assert!(compress(&[PathBuf::from("/a/x")], Path::new("/b")).is_err());
        assert!(compress(&[], Path::new("/b")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn extrae_un_tar_gz_en_carpeta_nueva() {
        let d = std::env::temp_dir().join(format!("nova_arch_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("src")).unwrap();
        std::fs::write(d.join("src/h.txt"), "hola").unwrap();
        let st = std::process::Command::new("tar").args(["-czf"]).arg(d.join("p.tar.gz")).arg("-C").arg(d.join("src")).arg(".").status();
        if st.map_or(true, |s| !s.success()) {
            return; // sin tar en este entorno
        }
        let out = extract(&d.join("p.tar.gz")).unwrap();
        assert_eq!(out, d.join("p"));
        assert_eq!(std::fs::read_to_string(out.join("h.txt")).unwrap(), "hola");
        assert_eq!(extract(&d.join("p.tar.gz")).unwrap(), d.join("p (2)"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
