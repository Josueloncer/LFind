//! Capacidad y estado de las unidades.

use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DriveUsage {
    pub root: String,
    pub total: u64,
    pub free: u64,
    pub used: u64,
    /// "fixed" | "removable" | "network" | "cdrom" | "ramdisk" | "unknown"
    pub kind: &'static str,
    pub label: Option<String>,
    pub fs: Option<String>,
}

pub fn drive_kind_name(code: u32) -> &'static str {
    match code {
        2 => "removable",
        3 => "fixed",
        4 => "network",
        5 => "cdrom",
        6 => "ramdisk",
        _ => "unknown",
    }
}

#[cfg(windows)]
pub fn drive_usage(root: &str) -> Result<DriveUsage, String> {
    use crate::sys::win;
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    let r = win::wide(std::ffi::OsStr::new(root));
    let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
    let ok = unsafe { win::GetDiskFreeSpaceExW(r.as_ptr(), &mut avail, &mut total, &mut free) };
    if ok == 0 {
        return Err("No se pudo leer la capacidad de la unidad (¿sin medio o desconectada?).".into());
    }
    let kind = drive_kind_name(unsafe { win::GetDriveTypeW(r.as_ptr()) });
    let (mut label, mut fsn) = ([0u16; 261], [0u16; 64]);
    let got = unsafe { win::GetVolumeInformationW(r.as_ptr(), label.as_mut_ptr(), label.len() as u32, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), fsn.as_mut_ptr(), fsn.len() as u32) };
    let s = |b: &[u16]| {
        let n = b.iter().position(|&c| c == 0).unwrap_or(b.len());
        Some(OsString::from_wide(&b[..n]).to_string_lossy().into_owned()).filter(|x| !x.is_empty())
    };
    Ok(DriveUsage {
        root: root.to_string(),
        total,
        free: avail,
        used: total.saturating_sub(free),
        kind,
        label: if got != 0 { s(&label) } else { None },
        fs: if got != 0 { s(&fsn) } else { None },
    })
}

#[cfg(not(windows))]
pub fn drive_usage(_root: &str) -> Result<DriveUsage, String> {
    Err("La capacidad de unidades solo está disponible en Windows.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn kinds() {
        assert_eq!(drive_kind_name(3), "fixed");
        assert_eq!(drive_kind_name(2), "removable");
        assert_eq!(drive_kind_name(99), "unknown");
    }
}
