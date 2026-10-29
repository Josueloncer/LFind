//! Ubicaciones conocidas del sistema: carpetas de usuario y unidades.

use serde::Serialize;
use std::path::PathBuf;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PlaceKind {
    Home,
    Desktop,
    Documents,
    Downloads,
    Pictures,
    Music,
    Videos,
    Drive,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    pub id: String,
    pub label: String,
    pub path: String,
    pub kind: PlaceKind,
}

fn place(id: &str, label: &str, kind: PlaceKind, p: Option<PathBuf>) -> Option<Place> {
    let p = p?;
    if !p.is_dir() {
        return None;
    }
    Some(Place { id: id.into(), label: label.into(), path: p.to_string_lossy().into_owned(), kind })
}

/// Carpetas de usuario que existen realmente en este equipo.
pub fn user_places() -> Vec<Place> {
    [
        place("home", "Inicio", PlaceKind::Home, dirs::home_dir()),
        place("desktop", "Escritorio", PlaceKind::Desktop, dirs::desktop_dir()),
        place("documents", "Documentos", PlaceKind::Documents, dirs::document_dir()),
        place("downloads", "Descargas", PlaceKind::Downloads, dirs::download_dir()),
        place("pictures", "Imágenes", PlaceKind::Pictures, dirs::picture_dir()),
        place("music", "Música", PlaceKind::Music, dirs::audio_dir()),
        place("videos", "Vídeos", PlaceKind::Videos, dirs::video_dir()),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Letras de unidad a partir de la máscara de `GetLogicalDrives` (bit 0 = A:).
pub fn drive_letters_from_mask(mask: u32) -> Vec<char> {
    (0..26u8)
        .filter(|i| mask & (1 << i) != 0)
        .map(|i| (b'A' + i) as char)
        .collect()
}

#[cfg(windows)]
pub fn drives() -> Vec<Place> {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetLogicalDrives() -> u32;
    }
    // Solo consulta la máscara del sistema: no toca los discos (no bloquea con unidades vacías).
    let mask = unsafe { GetLogicalDrives() };
    drive_letters_from_mask(mask)
        .into_iter()
        .map(|l| Place {
            id: format!("drive-{l}"),
            label: format!("Unidad {l}:"),
            path: format!("{l}:\\"),
            kind: PlaceKind::Drive,
        })
        .collect()
}

#[cfg(not(windows))]
pub fn drives() -> Vec<Place> {
    vec![Place { id: "drive-root".into(), label: "Sistema (/)".into(), path: "/".into(), kind: PlaceKind::Drive }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_to_letters() {
        assert_eq!(drive_letters_from_mask(0b101), vec!['A', 'C']);
        assert_eq!(drive_letters_from_mask(1 << 25), vec!['Z']);
        assert!(drive_letters_from_mask(0).is_empty());
    }

    #[test]
    fn only_existing_places_are_returned() {
        for p in user_places() {
            assert!(std::path::Path::new(&p.path).is_dir());
        }
    }
}
