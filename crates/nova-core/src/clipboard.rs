//! Portapapeles de archivos del sistema (el mismo que usa el Explorador): formato `CF_HDROP` + «Preferred DropEffect».
//!
//! Aquí solo vive el formato (construir/leer los bytes), que es independiente de la plataforma y se puede probar en cualquier
//! sistema. El acceso real al portapapeles está en `clipboard_win` (solo Windows).
//!
//! Layout de `DROPFILES` (20 bytes): `pFiles:u32` (desplazamiento a la lista, 20), `pt:POINT` (2×i32, a 0), `fNC:i32` (0),
//! `fWide:i32` (1 = UTF-16). A continuación, cada ruta terminada en `\0` y un `\0` final extra.

/// Ruta de Windows en forma canónica para el Shell: barras `\`, sin prefijo `\\?\` y sin barra final (salvo en una raíz `C:\`).
pub fn normalize_path(p: &str) -> String {
    let mut s = p.replace('/', "\\");
    if let Some(r) = s.strip_prefix("\\\\?\\") {
        // `\\?\UNC\srv\share` → `\\srv\share`
        s = match r.strip_prefix("UNC\\") {
            Some(unc) => format!("\\\\{unc}"),
            None => r.to_string(),
        };
    }
    let is_root = |t: &str| t.len() == 3 && t.as_bytes()[1] == b':' && t.ends_with('\\');
    if is_root(&s) {
        return s;
    }
    while s.len() > 3 && s.ends_with('\\') {
        s.pop();
    }
    if s.len() == 2 && s.as_bytes()[1] == b':' {
        s.push('\\'); // `C:` → `C:\`
    }
    s
}

pub const DROPEFFECT_COPY: u32 = 1;
pub const DROPEFFECT_MOVE: u32 = 2;
pub const DROPEFFECT_LINK: u32 = 4;
pub const DROPFILES_SIZE: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Copy,
    Cut,
}

impl Mode {
    pub fn effect(self) -> u32 {
        match self {
            Mode::Copy => DROPEFFECT_COPY,
            Mode::Cut => DROPEFFECT_MOVE,
        }
    }
    /// `None` si el efecto no es ni copiar ni mover (p. ej. «crear acceso directo»): LFind lo trata como copia.
    pub fn from_effect(e: u32) -> Mode {
        if e & DROPEFFECT_MOVE != 0 && e & DROPEFFECT_COPY == 0 {
            Mode::Cut
        } else {
            Mode::Copy
        }
    }
}

/// Bytes de un `DROPFILES` Unicode con las rutas dadas.
pub fn build_dropfiles(paths: &[String]) -> Vec<u8> {
    let mut out = Vec::with_capacity(DROPFILES_SIZE + paths.iter().map(|p| p.len() * 2 + 2).sum::<usize>() + 2);
    out.extend_from_slice(&(DROPFILES_SIZE as u32).to_le_bytes());
    out.extend_from_slice(&[0u8; 12]); // pt.x, pt.y, fNC
    out.extend_from_slice(&1u32.to_le_bytes()); // fWide
    for p in paths {
        let n = normalize_path(p);
        for u in n.encode_utf16().filter(|&u| u != 0) {
            out.extend_from_slice(&u.to_le_bytes());
        }
        out.extend_from_slice(&[0, 0]);
    }
    out.extend_from_slice(&[0, 0]);
    out
}

/// Lee las rutas de un `DROPFILES` (ANSI o Unicode). Ignora datos mal formados en lugar de entrar en pánico.
pub fn parse_dropfiles(b: &[u8]) -> Vec<String> {
    if b.len() < DROPFILES_SIZE {
        return vec![];
    }
    let off = u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize;
    let wide = u32::from_le_bytes([b[16], b[17], b[18], b[19]]) != 0;
    if off < DROPFILES_SIZE || off > b.len() {
        return vec![];
    }
    let data = &b[off..];
    let mut out = vec![];
    if wide {
        let units: Vec<u16> = data.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        for part in units.split(|&u| u == 0) {
            if part.is_empty() {
                break; // doble terminador = fin de la lista
            }
            out.push(String::from_utf16_lossy(part));
        }
    } else {
        for part in data.split(|&c| c == 0) {
            if part.is_empty() {
                break;
            }
            out.push(String::from_utf8_lossy(part).into_owned());
        }
    }
    out
}

#[cfg(any(windows, nova_winck))]
pub use crate::clipboard_win::{has_files, read_files, sequence, write_files};

#[cfg(not(any(windows, nova_winck)))]
pub fn write_files(_paths: &[String], _mode: Mode) -> Result<(), String> {
    Err("El portapapeles del sistema solo está disponible en Windows.".into())
}
#[cfg(not(any(windows, nova_winck)))]
pub fn read_files() -> Result<Option<(Vec<String>, Mode)>, String> {
    Ok(None)
}
#[cfg(not(any(windows, nova_winck)))]
pub fn has_files() -> bool {
    false
}
#[cfg(not(any(windows, nova_winck)))]
pub fn sequence() -> u32 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_normalised_for_the_shell() {
        assert_eq!(normalize_path("C:/Users/Ana/Docs/"), "C:\\Users\\Ana\\Docs");
        assert_eq!(normalize_path("C:\\"), "C:\\");
        assert_eq!(normalize_path("C:"), "C:\\");
        assert_eq!(normalize_path("\\\\?\\C:\\a\\b"), "C:\\a\\b");
        assert_eq!(normalize_path("\\\\?\\UNC\\srv\\share\\x"), "\\\\srv\\share\\x");
        assert_eq!(normalize_path("\\\\srv\\share\\"), "\\\\srv\\share");
    }

    #[test]
    fn roundtrip_unicode_paths() {
        let p = vec!["C:/Users/Ana/Documentos/ñandú ✓.txt".to_string(), "D:\\x y\\z".to_string()];
        let b = build_dropfiles(&p);
        assert_eq!(&b[..4], &20u32.to_le_bytes());
        assert_eq!(&b[16..20], &1u32.to_le_bytes());
        assert_eq!(&b[b.len() - 4..], &[0, 0, 0, 0], "doble terminador");
        assert_eq!(parse_dropfiles(&b), vec!["C:\\Users\\Ana\\Documentos\\ñandú ✓.txt", "D:\\x y\\z"]);
    }

    #[test]
    fn empty_list_is_valid() {
        let b = build_dropfiles(&[]);
        assert_eq!(b.len(), DROPFILES_SIZE + 2);
        assert!(parse_dropfiles(&b).is_empty());
    }

    #[test]
    fn ansi_lists_are_understood() {
        let mut b = vec![0u8; 20];
        b[0] = 20;
        b.extend_from_slice(b"C:\\a.txt\0C:\\b.txt\0\0");
        assert_eq!(parse_dropfiles(&b), vec!["C:\\a.txt", "C:\\b.txt"]);
    }

    #[test]
    fn malformed_data_never_panics() {
        assert!(parse_dropfiles(&[]).is_empty());
        assert!(parse_dropfiles(&[1, 2, 3]).is_empty());
        let mut b = vec![0u8; 20];
        b[0] = 200; // desplazamiento fuera de rango
        assert!(parse_dropfiles(&b).is_empty());
        b[0] = 4; // desplazamiento dentro de la cabecera
        assert!(parse_dropfiles(&b).is_empty());
        let mut w = build_dropfiles(&["C:\\a".into()]);
        w.truncate(w.len() - 3); // UTF-16 cortado a mitad
        let _ = parse_dropfiles(&w);
    }

    #[test]
    fn embedded_nuls_are_dropped_not_truncating_the_list() {
        let b = build_dropfiles(&["C:\\a\0b".into(), "C:\\c".into()]);
        assert_eq!(parse_dropfiles(&b), vec!["C:\\ab", "C:\\c"]);
    }

    #[test]
    fn drop_effects() {
        assert_eq!(Mode::Copy.effect(), 1);
        assert_eq!(Mode::Cut.effect(), 2);
        assert_eq!(Mode::from_effect(2), Mode::Cut);
        assert_eq!(Mode::from_effect(1), Mode::Copy);
        assert_eq!(Mode::from_effect(5), Mode::Copy);
        assert_eq!(Mode::from_effect(4), Mode::Copy);
        assert_eq!(Mode::from_effect(0), Mode::Copy);
    }
}
