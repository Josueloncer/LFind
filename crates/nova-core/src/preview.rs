//! Vista previa textual segura para el panel de vista previa.
//! Solo extensiones de texto/código conocidas, solo los primeros bytes y nunca contenido binario.

use serde::Serialize;
use std::fs::File;
use std::io::Read;
use std::path::Path;

pub const TEXT_EXTS: &[&str] = &[
    "txt", "md", "markdown", "log", "json", "jsonc", "xml", "yml", "yaml", "toml", "ini", "cfg", "conf", "csv", "tsv", "js", "mjs", "ts", "tsx", "jsx", "css",
    "scss", "html", "htm", "rs", "py", "java", "c", "h", "cpp", "hpp", "cs", "go", "sh", "ps1", "bat", "cmd", "sql", "lua", "rb", "php", "kt", "swift", "vue",
    "svelte", "gitignore", "editorconfig", "lock",
];

pub const MAX_PREVIEW_BYTES: usize = 8 * 1024;

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TextPreview {
    pub text: String,
    pub truncated: bool,
    pub total_bytes: u64,
}

pub fn is_text_ext(ext: &str) -> bool {
    TEXT_EXTS.contains(&ext)
}

pub fn text_preview(path: &Path, max_bytes: usize) -> Result<TextPreview, String> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    // «.gitignore» no tiene extensión para Rust: se toma el nombre completo.
    let name = path.file_name().map(|n| n.to_string_lossy().trim_start_matches('.').to_lowercase()).unwrap_or_default();
    if !is_text_ext(&ext) && !is_text_ext(&name) {
        return Err("Este tipo de archivo no tiene vista previa de texto.".into());
    }
    let meta = std::fs::metadata(path).map_err(|e| crate::ops::classify(&e).1)?;
    if !meta.is_file() {
        return Err("No es un archivo.".into());
    }
    if crate::dir::is_cloud_only(&meta) {
        return Err("Este archivo está solo en la nube: la vista previa lo descargaría.".into());
    }
    let mut buf = vec![0u8; max_bytes.min(MAX_PREVIEW_BYTES * 4)];
    let n = File::open(path).and_then(|mut f| f.read(&mut buf)).map_err(|e| crate::ops::classify(&e).1)?;
    buf.truncate(n);
    if buf.contains(&0) {
        return Err("El archivo parece binario: no se muestra como texto.".into());
    }
    // Si se cortó en mitad de un carácter UTF-8, se descarta el fragmento final incompleto.
    let text = match std::str::from_utf8(&buf) {
        Ok(s) => s.to_string(),
        Err(e) if e.error_len().is_none() => String::from_utf8_lossy(&buf[..e.valid_up_to()]).into_owned(),
        Err(_) => String::from_utf8_lossy(&buf).into_owned(),
    };
    Ok(TextPreview { text, truncated: meta.len() > n as u64, total_bytes: meta.len() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("nova-prev-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn reads_only_the_beginning_and_flags_truncation() {
        let p = tmp("a.txt", "x".repeat(20_000).as_bytes());
        let r = text_preview(&p, MAX_PREVIEW_BYTES).unwrap();
        assert_eq!(r.text.len(), MAX_PREVIEW_BYTES);
        assert!(r.truncated);
        assert_eq!(r.total_bytes, 20_000);
        assert!(!text_preview(&tmp("b.txt", b"hola"), 100).unwrap().truncated);
    }

    #[test]
    fn rejects_binary_and_unknown_types() {
        assert!(text_preview(&tmp("c.txt", b"ab\0cd"), 100).is_err());
        assert!(text_preview(&tmp("d.exe", b"MZ"), 100).is_err());
        assert!(text_preview(&tmp("e.pdf", b"%PDF"), 100).is_err());
    }

    #[test]
    fn cutting_in_the_middle_of_a_utf8_char_is_handled() {
        let p = tmp("f.txt", "ñ".repeat(10).as_bytes()); // 2 bytes por carácter
        let r = text_preview(&p, 5).unwrap();
        assert_eq!(r.text, "ññ");
        assert!(r.truncated);
    }

    #[test]
    fn dotfiles_by_name() {
        assert!(text_preview(&tmp(".gitignore", b"target/"), 100).is_ok());
    }
}
