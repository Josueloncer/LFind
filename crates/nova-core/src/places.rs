//! Ubicaciones conocidas del sistema: carpetas de usuario y unidades.

use serde::Serialize;
use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

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
    Cloud,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    pub id: String,
    pub label: String,
    pub path: String,
    pub kind: PlaceKind,
    /// Solo unidades: "fixed", "removable", "network"… (sale del sistema sin tocar el disco).
    pub drive_kind: Option<&'static str>,
}

fn place(id: &str, label: &str, kind: PlaceKind, p: Option<PathBuf>) -> Option<Place> {
    let p = p?;
    if !p.is_dir() {
        return None;
    }
    Some(Place { id: id.into(), label: label.into(), path: p.to_string_lossy().into_owned(), kind, drive_kind: None })
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
#[link(name = "kernel32")]
extern "system" {
    fn GetLogicalDrives() -> u32;
}

/// Unidades presentes, como máscara de bits. No toca los discos: se puede consultar a menudo para detectar cambios.
#[cfg(windows)]
pub fn drives_mask() -> u32 {
    unsafe { GetLogicalDrives() }
}

#[cfg(not(windows))]
pub fn drives_mask() -> u32 {
    1
}

#[cfg(windows)]
pub fn drives() -> Vec<Place> {
    // Solo consultas al sistema: no toca los discos (no bloquea con unidades vacías o de red caídas).
    drive_letters_from_mask(drives_mask())
        .into_iter()
        .map(|l| {
            let path = format!("{l}:\\");
            let code = unsafe { crate::sys::win::GetDriveTypeW(crate::sys::win::wide(std::ffi::OsStr::new(&path)).as_ptr()) };
            Place { id: format!("drive-{l}"), label: format!("Unidad {l}:"), path, kind: PlaceKind::Drive, drive_kind: Some(crate::volumes::drive_kind_name(code)) }
        })
        .collect()
}

#[cfg(not(windows))]
pub fn drives() -> Vec<Place> {
    vec![Place { id: "drive-root".into(), label: "Sistema (/)".into(), path: "/".into(), kind: PlaceKind::Drive, drive_kind: Some("fixed") }]
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PathState {
    Ok,
    /// La unidad responde y la carpeta no está: de verdad ya no existe.
    Missing,
    /// No se puede saber: unidad desconectada, red caída o sin respuesta a tiempo. Nunca se trata como borrada.
    Unavailable,
}

fn root_of(path: &Path) -> PathBuf {
    let mut r = PathBuf::new();
    for c in path.components().take_while(|c| matches!(c, Component::Prefix(_) | Component::RootDir)) {
        r.push(c.as_os_str());
    }
    r
}

/// Estado de una carpeta. Lee sus atributos, no su contenido: no descarga nada de OneDrive.
pub fn path_state(path: &Path) -> PathState {
    match fs::metadata(path) {
        Ok(m) if m.is_dir() => PathState::Ok,
        Ok(_) => PathState::Missing,
        Err(e) if e.kind() == ErrorKind::PermissionDenied => PathState::Ok,
        Err(_) if root_of(path).is_dir() => PathState::Missing,
        Err(_) => PathState::Unavailable,
    }
}

const CHECK_WORKERS: usize = 8;

/// Comprueba varias rutas a la vez y con tiempo límite: una unidad de red colgada solo deja sus rutas como `Unavailable`.
pub fn check_paths(paths: Vec<PathBuf>, timeout: Duration) -> Vec<PathState> {
    let total = paths.len();
    let queue = Arc::new(Mutex::new(paths.into_iter().enumerate().collect::<Vec<_>>()));
    let (tx, rx) = mpsc::channel();
    for _ in 0..CHECK_WORKERS.min(total) {
        let (queue, tx) = (queue.clone(), tx.clone());
        std::thread::spawn(move || loop {
            let next = queue.lock().unwrap_or_else(|e| e.into_inner()).pop();
            let Some((i, p)) = next else { break };
            if tx.send((i, path_state(&p))).is_err() {
                break;
            }
        });
    }
    drop(tx);
    let mut out = vec![PathState::Unavailable; total];
    let end = Instant::now() + timeout;
    for _ in 0..total {
        let Some(left) = end.checked_duration_since(Instant::now()) else { break };
        match rx.recv_timeout(left) {
            Ok((i, s)) => out[i] = s,
            Err(_) => break,
        }
    }
    out
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
    fn path_states_distinguish_deleted_from_unreachable() {
        let d = std::env::temp_dir().join(format!("nova-places-{}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        let f = d.join("archivo.txt");
        fs::write(&f, b"x").unwrap();
        assert_eq!(path_state(&d), PathState::Ok);
        assert_eq!(path_state(&d.join("borrada")), PathState::Missing, "la unidad responde: ya no existe");
        assert_eq!(path_state(&f), PathState::Missing, "un archivo no es una carpeta");
        if cfg!(windows) {
            // Ninguna letra libre responde: no se puede afirmar que la carpeta se borrara.
            let mask = drives_mask();
            if let Some(l) = (0..26u8).rev().find(|i| mask & (1 << i) == 0) {
                assert_eq!(path_state(Path::new(&format!("{}:\\x", (b'A' + l) as char))), PathState::Unavailable);
            }
        }
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn check_paths_keeps_order_and_handles_empty_input() {
        let t = std::env::temp_dir();
        let r = check_paths(vec![t.clone(), t.join("nova-no-existe-xyz"), t], Duration::from_secs(5));
        assert_eq!(r, vec![PathState::Ok, PathState::Missing, PathState::Ok]);
        assert!(check_paths(vec![], Duration::from_secs(1)).is_empty());
    }

    #[test]
    fn drives_report_their_type_without_touching_disks() {
        for d in drives() {
            assert!(d.drive_kind.is_some(), "{}", d.path);
        }
    }

    #[test]
    fn only_existing_places_are_returned() {
        for p in user_places() {
            assert!(std::path::Path::new(&p.path).is_dir());
        }
    }
}
