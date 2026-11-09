//! Familias tipográficas instaladas en el equipo. Solo se leen sus nombres: ninguna fuente se carga ni se copia.

use std::collections::BTreeMap;

const MAX_FAMILIES: usize = 2000;

/// Deja una lista limpia: sin variantes verticales («@Nombre»), sin nombres que no se puedan usar en CSS, sin repetir y en orden alfabético.
pub fn clean_families(names: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = BTreeMap::new();
    for name in names {
        let name = name.trim().to_string();
        let usable = !name.is_empty() && name.chars().count() <= 100 && !name.starts_with('@') && !name.chars().any(|c| c == '"' || c == '\\' || c.is_control());
        if usable {
            seen.entry(name.to_lowercase()).or_insert(name);
        }
    }
    seen.into_values().take(MAX_FAMILIES).collect()
}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;

    #[repr(C)]
    struct LogFont {
        height: i32,
        width: i32,
        escapement: i32,
        orientation: i32,
        weight: i32,
        italic: u8,
        underline: u8,
        strikeout: u8,
        charset: u8,
        out_precision: u8,
        clip_precision: u8,
        quality: u8,
        pitch_family: u8,
        face: [u16; 32],
    }

    type EnumProc = unsafe extern "system" fn(*const LogFont, *const c_void, u32, isize) -> i32;

    #[link(name = "gdi32")]
    extern "system" {
        fn CreateCompatibleDC(hdc: isize) -> isize;
        fn DeleteDC(hdc: isize) -> i32;
        fn EnumFontFamiliesExW(hdc: isize, font: *const LogFont, proc_: EnumProc, param: isize, flags: u32) -> i32;
    }

    const DEFAULT_CHARSET: u8 = 1;
    /// Las fuentes de mapa de bits no se pueden usar en la interfaz web.
    const RASTER_FONTTYPE: u32 = 1;

    unsafe extern "system" fn collect(font: *const LogFont, _metrics: *const c_void, kind: u32, param: isize) -> i32 {
        if kind & RASTER_FONTTYPE == 0 {
            let face = &(*font).face;
            let end = face.iter().position(|c| *c == 0).unwrap_or(face.len());
            (*(param as *mut Vec<String>)).push(String::from_utf16_lossy(&face[..end]));
        }
        1
    }

    pub fn raw_families() -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        unsafe {
            let hdc = CreateCompatibleDC(0);
            if hdc == 0 {
                return out;
            }
            let query = LogFont { height: 0, width: 0, escapement: 0, orientation: 0, weight: 0, italic: 0, underline: 0, strikeout: 0, charset: DEFAULT_CHARSET, out_precision: 0, clip_precision: 0, quality: 0, pitch_family: 0, face: [0; 32] };
            EnumFontFamiliesExW(hdc, &query, collect, &mut out as *mut Vec<String> as isize, 0);
            DeleteDC(hdc);
        }
        out
    }
}

/// Fuentes instaladas. Fuera de Windows, lista vacía: la interfaz ofrece entonces las opciones predefinidas.
pub fn installed() -> Vec<String> {
    #[cfg(windows)]
    {
        clean_families(imp::raw_families())
    }
    #[cfg(not(windows))]
    {
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_duplicates_vertical_variants_and_unsafe_names() {
        let raw = ["Arial", "arial", "@MS Gothic", "", "  Verdana ", "Mala\"Fuente", "Otra\\Mala", "Bahnschrift"].map(String::from);
        assert_eq!(clean_families(raw), vec!["Arial", "Bahnschrift", "Verdana"]);
    }

    #[test]
    fn the_list_is_bounded() {
        assert_eq!(clean_families((0..MAX_FAMILIES + 50).map(|i| format!("Fuente {i}"))).len(), MAX_FAMILIES);
    }

    #[cfg(windows)]
    #[test]
    fn finds_the_fonts_of_this_system() {
        let all = installed();
        assert!(all.len() > 10, "{}", all.len());
        assert!(all.iter().any(|f| f == "Segoe UI") && all.iter().any(|f| f == "Arial"));
        assert!(all.iter().all(|f| !f.starts_with('@')));
    }
}
