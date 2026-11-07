//! Plantillas de «Nuevo» (las que Windows registra en `HKCR\.ext\...\ShellNew`), leídas con `reg.exe` (sin FFI ni COM).
//! Solo se consultan unas extensiones conocidas, una vez y en segundo plano; crear un archivo vuelve a leer SOLO esa clave.
//! El identificador es la extensión (`.docx`) y se valida; nunca llega una ruta desde la interfaz.

use std::io;
use std::path::{Path, PathBuf};

/// Extensiones que se ofrecen si tienen `ShellNew` registrado. («Documento de texto» y «Carpeta» ya son entradas propias.)
pub const CANDIDATES: &[&str] = &["docx", "xlsx", "pptx", "rtf", "bmp", "odt", "ods", "odp", "vsdx", "accdb", "pub", "zip"];

#[derive(Clone, Debug, PartialEq)]
pub enum Template {
    /// Archivo vacío.
    Null,
    /// Copiar este archivo (ya con variables de entorno expandidas; puede ser relativo: se busca en las carpetas de plantillas).
    File(String),
    /// Bytes literales.
    Data(Vec<u8>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub id: String,    // ".docx"
    pub label: String, // «Documento de Microsoft Word»
}

pub fn valid_ext(id: &str) -> bool {
    id.len() >= 2 && id.len() <= 11 && id.starts_with('.') && id[1..].bytes().all(|b| b.is_ascii_alphanumeric())
}

/// Divide una línea de valor de `reg query`: `    nombre    REG_SZ    datos` → (nombre, tipo, datos).
fn value_line(l: &str) -> Option<(String, String, String)> {
    if !l.starts_with("    ") {
        return None;
    }
    let l = l.trim_end_matches('\r');
    let at = l.find("    REG_")?;
    let name = l[..at].trim().to_string();
    let rest = &l[at + 4..];
    let (ty, data) = match rest.find("    ") {
        Some(i) => (&rest[..i], rest[i..].trim()),
        None => (rest, ""),
    };
    Some((name, ty.to_string(), data.to_string()))
}

/// Valor predeterminado de la clave raíz (`reg query HKCR\.ext /s`: la primera línea de valor sin nombre propio).
pub fn parse_default(out: &str) -> Option<String> {
    for l in out.lines() {
        if let Some((n, _, d)) = value_line(l) {
            if n.starts_with('(') && !d.is_empty() {
                return Some(d);
            }
        }
        if l.starts_with("HKEY") && l.contains("\\ShellNew") {
            break;
        }
    }
    None
}

/// Busca valores `NullFile` / `FileName` / `Data` en cualquier subclave `ShellNew` de la salida de `reg query … /s`.
pub fn parse_shellnew(out: &str) -> Option<Template> {
    let mut inside = false;
    let mut null = false;
    let mut file = None;
    let mut data = None;
    for l in out.lines() {
        if l.starts_with("HKEY") {
            inside = l.trim_end().to_ascii_lowercase().ends_with("\\shellnew");
            continue;
        }
        if !inside {
            continue;
        }
        if let Some((n, _ty, d)) = value_line(l) {
            match n.to_ascii_lowercase().as_str() {
                "nullfile" => null = true,
                "filename" if !d.is_empty() => file = Some(expand_env(&d, &|k| std::env::var(k).ok())),
                "data" => data = decode_hex(&d),
                _ => {}
            }
        }
    }
    file.map(Template::File).or(data.map(Template::Data)).or(null.then_some(Template::Null))
}

pub fn decode_hex(s: &str) -> Option<Vec<u8>> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

/// Expande `%VAR%` (sin distinguir mayúsculas en el nombre al pedirlo).
pub fn expand_env(s: &str, get: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('%') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        match after.find('%') {
            Some(j) if j > 0 => {
                match get(&after[..j]) {
                    Some(v) => out.push_str(&v),
                    None => out.push_str(&rest[i..i + j + 2]),
                }
                rest = &after[j + 1..];
            }
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Nombre de archivo seguro para el texto de la plantilla.
pub fn file_label(label: &str) -> String {
    let s: String = label.chars().map(|c| if "<>:\"/\\|?*".contains(c) || c.is_control() { ' ' } else { c }).collect();
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(windows)]
fn reg(args: &[&str]) -> Option<String> {
    use std::os::windows::process::CommandExt;
    let o = std::process::Command::new("reg.exe").arg("query").args(args).creation_flags(0x0800_0000).output().ok()?;
    o.status.success().then(|| String::from_utf8_lossy(&o.stdout).into_owned())
}

#[cfg(not(windows))]
fn reg(_args: &[&str]) -> Option<String> {
    None
}

fn query_ext(ext: &str) -> Option<(Template, String)> {
    let key = format!("HKCR\\.{ext}");
    let out = reg(&[&key, "/s"])?;
    let tpl = parse_shellnew(&out)?;
    let label = parse_default(&out)
        .and_then(|prog| reg(&[&format!("HKCR\\{prog}"), "/ve"]))
        .and_then(|o| o.lines().find_map(|l| value_line(l).filter(|v| v.0.starts_with('(')).map(|v| v.2)))
        .map(|l| file_label(&l))
        .filter(|l| !l.is_empty())
        .unwrap_or_else(|| format!("Archivo {}", ext.to_uppercase()));
    Some((tpl, label))
}

/// Plantillas disponibles (una consulta por extensión candidata; hacer UNA vez y en segundo plano).
pub fn list() -> Vec<Item> {
    CANDIDATES.iter().filter_map(|e| query_ext(e).map(|(_, label)| Item { id: format!(".{e}"), label })).collect()
}

fn template_dirs() -> Vec<PathBuf> {
    let mut v = vec![];
    if let Some(r) = std::env::var_os("SystemRoot") {
        v.push(Path::new(&r).join("ShellNew"));
    }
    if let Some(u) = std::env::var_os("USERPROFILE") {
        v.push(Path::new(&u).join("Templates"));
    }
    if let Some(a) = std::env::var_os("APPDATA") {
        v.push(Path::new(&a).join("Microsoft").join("Windows").join("Templates"));
    }
    v
}

/// Crea el archivo de la plantilla `id` en `dir`. Devuelve su ruta.
pub fn create(dir: &Path, id: &str) -> io::Result<PathBuf> {
    if !valid_ext(id) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Plantilla no válida."));
    }
    let (tpl, label) = query_ext(&id[1..]).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Esa plantilla ya no está disponible."))?;
    create_with(dir, id, &tpl, &label, &template_dirs())
}

pub fn create_with(dir: &Path, ext: &str, tpl: &Template, label: &str, search: &[PathBuf]) -> io::Result<PathBuf> {
    let name = crate::names::unique_name(dir, &format!("Nuevo {}{}", label, ext), false);
    let path = dir.join(name);
    match tpl {
        Template::Null => drop(std::fs::OpenOptions::new().write(true).create_new(true).open(&path)?),
        Template::Data(b) => {
            use std::io::Write;
            std::fs::OpenOptions::new().write(true).create_new(true).open(&path)?.write_all(b)?;
        }
        Template::File(f) => {
            let p = Path::new(f);
            let src = if p.is_absolute() { Some(p.to_path_buf()) } else { search.iter().map(|d| d.join(f)).find(|c| c.is_file()) };
            let src = src.filter(|s| s.is_file()).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "No se encontró el archivo de plantilla."))?;
            if path.exists() {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, "El archivo ya existe."));
            }
            std::fs::copy(src, &path)?;
        }
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORD: &str = "\r\nHKEY_CLASSES_ROOT\\.docx\r\n    (Predeterminado)    REG_SZ    Word.Document.12\r\n    Content Type    REG_SZ    application/x\r\n\r\nHKEY_CLASSES_ROOT\\.docx\\OpenWithProgids\r\n    Word.Document.12    REG_NONE    \r\n\r\nHKEY_CLASSES_ROOT\\.docx\\Word.Document.12\\ShellNew\r\n    FileName    REG_SZ    %SystemRoot%\\Plantilla.docx\r\n";

    #[test]
    fn lee_plantillas() {
        assert_eq!(parse_default(WORD).as_deref(), Some("Word.Document.12"));
        let t = parse_shellnew(WORD).unwrap();
        assert!(matches!(t, Template::File(ref f) if f.ends_with("Plantilla.docx")));
        assert_eq!(parse_shellnew("HKEY_CLASSES_ROOT\\.bmp\\ShellNew\r\n    NullFile    REG_SZ    \r\n"), Some(Template::Null));
        assert_eq!(parse_shellnew("HKEY_CLASSES_ROOT\\.rtf\\ShellNew\r\n    Data    REG_BINARY    7B5C7274667D\r\n"), Some(Template::Data(b"{\\rtf}".to_vec())));
        assert_eq!(parse_shellnew("HKEY_CLASSES_ROOT\\.x\r\n    NullFile    REG_SZ    \r\n"), None); // fuera de ShellNew
        assert_eq!(parse_shellnew("HKEY_CLASSES_ROOT\\.x\\ShellNew\r\n    Otro    REG_SZ    1\r\n"), None);
    }

    #[test]
    fn utilidades() {
        assert!(valid_ext(".docx") && !valid_ext("docx") && !valid_ext(".a/b") && !valid_ext("."));
        assert_eq!(decode_hex("7B 5c"), Some(vec![0x7b, 0x5c]));
        assert_eq!(decode_hex("7"), None);
        let get = |k: &str| (k == "A").then(|| "x".to_string());
        assert_eq!(expand_env("%A%\\%B%\\100%", &get), "x\\%B%\\100%");
        assert_eq!(file_label("Documento: de  Word/1"), "Documento de Word 1");
    }

    #[test]
    fn crea_archivos() {
        let d = std::env::temp_dir().join(format!("nova_new_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let a = create_with(&d, ".bmp", &Template::Null, "Imagen", &[]).unwrap();
        assert_eq!(a.file_name().unwrap(), "Nuevo Imagen.bmp");
        assert_eq!(std::fs::metadata(&a).unwrap().len(), 0);
        let b = create_with(&d, ".bmp", &Template::Data(vec![1, 2, 3]), "Imagen", &[]).unwrap();
        assert_eq!(b.file_name().unwrap(), "Nuevo Imagen (2).bmp");
        std::fs::write(d.join("T.docx"), "plantilla").unwrap();
        let c = create_with(&d, ".docx", &Template::File("T.docx".into()), "Word", &[d.clone()]).unwrap();
        assert_eq!(std::fs::read_to_string(c).unwrap(), "plantilla");
        assert!(create_with(&d, ".docx", &Template::File("no.docx".into()), "Word", &[d.clone()]).is_err());
        assert!(create(&d, "../x").is_err());
        let _ = std::fs::remove_dir_all(&d);
    }
}
