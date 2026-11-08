//! Claves de ruta, extensiones y categorías (coinciden con `src/lib/fileKinds.ts`).

/// Clave de comparación: minúsculas, separador `/`, sin `/` final. Windows no distingue mayúsculas.
pub fn path_key(path: &str) -> String {
    let mut k = path.replace('\\', "/").to_lowercase();
    while k.len() > 1 && k.ends_with('/') {
        k.pop();
    }
    k
}

/// Profundidad = número de separadores de la clave.
pub fn depth_of(key: &str) -> u32 {
    key.bytes().filter(|&b| b == b'/').count() as u32
}

/// Extensión en minúsculas sin punto. `.gitignore` y `archivo.` no tienen extensión.
pub fn ext_of(name: &str) -> String {
    match name.rfind('.') {
        Some(i) if i > 0 && i + 1 < name.len() => name[i + 1..].to_lowercase(),
        _ => String::new(),
    }
}

/// Intervalo de claves `[desde, hasta)` que contiene exactamente a los descendientes de `folder`.
pub fn subtree_range(folder: &str) -> (String, String) {
    let k = path_key(folder);
    let base = if k.ends_with('/') { k[..k.len() - 1].to_string() } else { k };
    (format!("{base}/"), format!("{base}0")) // '0' es el carácter que sigue a '/'
}

pub fn category_exts(cat: &str) -> &'static [&'static str] {
    match cat {
        "image" => &["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "ico", "heic", "avif", "tif", "tiff", "raw"],
        "video" => &["mp4", "mkv", "mov", "avi", "webm", "wmv", "flv", "m4v"],
        "audio" => &["mp3", "wav", "flac", "ogg", "m4a", "aac", "wma", "opus"],
        "archive" => &["zip", "rar", "7z", "tar", "gz", "bz2", "xz", "iso"],
        "code" => &["js", "ts", "tsx", "jsx", "json", "rs", "py", "html", "css", "md", "toml", "yml", "yaml", "c", "cpp", "h", "cs", "java", "go", "sh", "ps1", "bat", "sql", "xml"],
        "doc" => &["doc", "docx", "txt", "rtf", "odt", "log"],
        "sheet" => &["xls", "xlsx", "csv", "ods", "tsv"],
        "slides" => &["ppt", "pptx", "odp", "key"],
        "pdf" => &["pdf"],
        "design" => &["psd", "psb", "ai", "eps", "indd", "xcf", "sketch", "fig", "cdr"],
        "exec" => &["exe", "msi", "dll", "lnk"],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keys() {
        assert_eq!(path_key("C:\\Users\\Ana\\"), "c:/users/ana");
        assert_eq!(path_key("C:\\"), "c:");
        assert_eq!(path_key("/"), "/");
        assert_eq!(depth_of("c:/users/ana"), 2);
    }
    #[test]
    fn exts() {
        assert_eq!(ext_of("Foto.PNG"), "png");
        assert_eq!(ext_of(".gitignore"), "");
        assert_eq!(ext_of("a.tar.gz"), "gz");
        assert_eq!(ext_of("fin."), "");
        assert_eq!(ext_of("sin"), "");
    }
    #[test]
    fn subtree() {
        let (a, b) = subtree_range("C:\\Users\\Ana");
        assert!("c:/users/ana/x.txt" >= a.as_str() && "c:/users/ana/x.txt" < b.as_str());
        assert!(!("c:/users/anabel/x" >= a.as_str() && "c:/users/anabel/x" < b.as_str()));
        assert!(!("c:/users/ana" >= a.as_str()));
    }
}
