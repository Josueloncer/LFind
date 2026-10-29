//! Validación de nombres y generación de nombres únicos.
//! Se aplican las reglas de Windows en todas las plataformas: la app es para Windows y así
//! un nombre aceptado aquí es válido en cualquier unidad.

use std::path::Path;

const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2",
    "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// `Err(mensaje)` con una explicación lista para mostrar al usuario.
pub fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("El nombre no puede estar vacío.".into());
    }
    if name == "." || name == ".." {
        return Err("Ese nombre no es válido.".into());
    }
    if name.encode_utf16().count() > 255 {
        return Err("El nombre es demasiado largo (máximo 255 caracteres).".into());
    }
    if let Some(c) = name.chars().find(|c| matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || (*c as u32) < 0x20) {
        return Err(format!("El nombre no puede contener el carácter «{}».", if (c as u32) < 0x20 { '␀' } else { c }));
    }
    if name.ends_with(' ') || name.ends_with('.') {
        return Err("El nombre no puede terminar en espacio ni en punto.".into());
    }
    let stem = name.split('.').next().unwrap_or(name).trim_end().to_ascii_uppercase();
    if RESERVED.contains(&stem.as_str()) {
        return Err(format!("«{stem}» es un nombre reservado de Windows."));
    }
    Ok(())
}

fn split_ext(name: &str, is_dir: bool) -> (&str, &str) {
    if is_dir {
        return (name, "");
    }
    match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    }
}

/// Devuelve `name` si no existe en `dir`; si no, `nombre (2).ext`, `nombre (3).ext`…
pub fn unique_name(dir: &Path, name: &str, is_dir: bool) -> String {
    let exists = |n: &str| std::fs::symlink_metadata(dir.join(n)).is_ok();
    if !exists(name) {
        return name.to_string();
    }
    let (stem, ext) = split_ext(name, is_dir);
    for i in 2..100_000 {
        let cand = format!("{stem} ({i}){ext}");
        if !exists(&cand) {
            return cand;
        }
    }
    format!("{stem} (copia){ext}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_normal_names() {
        for n in ["informe.pdf", "a", "Mi carpeta", "ñandú (1).txt", ".gitignore", "a.b.c"] {
            assert!(validate_name(n).is_ok(), "{n}");
        }
    }

    #[test]
    fn rejects_bad_names() {
        for n in ["", ".", "..", "a/b", "a\\b", "a:b", "a*b", "a?b", "a\"b", "a<b", "a|b", "fin.", "fin ", "con", "NUL.txt", "com1", "Lpt9.log", "a\u{1}b"] {
            assert!(validate_name(n).is_err(), "{n:?} debería rechazarse");
        }
        assert!(validate_name(&"x".repeat(256)).is_err());
        assert!(validate_name(&"x".repeat(255)).is_ok());
        assert!(validate_name("console").is_ok(), "«console» no es reservado");
    }

    #[test]
    fn unique_names() {
        let d = std::env::temp_dir().join(format!("nova-names-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        assert_eq!(unique_name(&d, "a.txt", false), "a.txt");
        std::fs::write(d.join("a.txt"), b"").unwrap();
        assert_eq!(unique_name(&d, "a.txt", false), "a (2).txt");
        std::fs::write(d.join("a (2).txt"), b"").unwrap();
        assert_eq!(unique_name(&d, "a.txt", false), "a (3).txt");
        std::fs::create_dir(d.join("v1.2")).unwrap();
        assert_eq!(unique_name(&d, "v1.2", true), "v1.2 (2)", "en carpetas el punto no es extensión");
        std::fs::write(d.join(".env"), b"").unwrap();
        assert_eq!(unique_name(&d, ".env", false), ".env (2)");
        let _ = std::fs::remove_dir_all(d);
    }
}
