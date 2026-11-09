//! Carpetas de OneDrive configuradas en este equipo.
//!
//! La fuente fiable es el registro: `HKCU\Software\Microsoft\OneDrive\Accounts\<Personal|Business1…>`, con una carpeta por cuenta.
//! Las variables de entorno solo se usan si el registro no da nada. Una cuenta sin carpeta (o cuya carpeta ya no existe) no se muestra.
//! Comprobar que la carpeta raíz existe no descarga nada: solo los archivos «solo en la nube» se descargan, y al abrirlos.

use crate::index::classify::path_key;
use crate::places::{Place, PlaceKind};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Account {
    /// Nombre de la subclave: `Personal`, `Business1`…
    pub key: String,
    pub folder: Option<String>,
    pub display: Option<String>,
    pub email: Option<String>,
}

fn clean(v: &Option<String>) -> Option<&str> {
    v.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

/// Convierte las cuentas en ubicaciones: sin duplicados, y distinguibles si dos cuentas de empresa se llaman igual.
pub fn resolve(accounts: &[Account], env: &dyn Fn(&str) -> Option<String>, exists: &dyn Fn(&str) -> bool) -> Vec<Place> {
    let mut found: Vec<(String, String, Option<String>)> = vec![];
    for a in accounts {
        let Some(folder) = clean(&a.folder) else { continue };
        let label = if a.key.eq_ignore_ascii_case("personal") { "OneDrive - Personal".to_string() } else { format!("OneDrive - {}", clean(&a.display).or(clean(&a.email)).unwrap_or("Empresa")) };
        found.push((label, folder.to_string(), clean(&a.email).map(String::from)));
    }
    if found.is_empty() {
        for (var, label) in [("OneDriveConsumer", "OneDrive - Personal"), ("OneDriveCommercial", "OneDrive - Empresa"), ("OneDrive", "OneDrive")] {
            if let Some(v) = env(var).filter(|v| !v.trim().is_empty()) {
                found.push((label.to_string(), v, None));
            }
        }
    }
    let mut seen: Vec<String> = vec![];
    found.retain(|(_, f, _)| {
        let k = path_key(f);
        let new = !seen.contains(&k) && exists(f);
        if new {
            seen.push(k);
        }
        new
    });
    let labels: Vec<String> = found.iter().map(|f| f.0.clone()).collect();
    found
        .into_iter()
        .enumerate()
        .map(|(i, (label, path, email))| {
            let repeated = labels.iter().filter(|l| **l == label).count() > 1;
            let label = match email.filter(|_| repeated) {
                Some(e) => format!("{label} ({e})"),
                None => label,
            };
            Place { id: format!("onedrive-{i}"), label, path, kind: PlaceKind::Cloud, drive_kind: None }
        })
        .collect()
}

#[cfg(windows)]
fn read_accounts() -> Vec<Account> {
    use crate::sys::win::wide;
    use std::ffi::{c_void, OsStr};
    use std::ptr::null_mut;

    #[link(name = "advapi32")]
    extern "system" {
        fn RegOpenKeyExW(key: isize, sub: *const u16, opts: u32, sam: u32, out: *mut isize) -> i32;
        fn RegEnumKeyExW(key: isize, i: u32, name: *mut u16, len: *mut u32, res: *mut u32, class: *mut u16, class_len: *mut u32, time: *mut c_void) -> i32;
        fn RegQueryValueExW(key: isize, name: *const u16, res: *mut u32, ty: *mut u32, data: *mut u8, len: *mut u32) -> i32;
        fn RegCloseKey(key: isize) -> i32;
    }
    const HKCU: isize = 0x8000_0001u32 as i32 as isize;
    const KEY_READ: u32 = 0x2_0019;
    const REG_SZ: u32 = 1;
    const REG_EXPAND_SZ: u32 = 2;

    /// Clave abierta que se cierra sola.
    struct Key(isize);
    impl Drop for Key {
        fn drop(&mut self) {
            unsafe { RegCloseKey(self.0) };
        }
    }
    fn open(parent: isize, sub: &str) -> Option<Key> {
        let mut h = 0;
        (unsafe { RegOpenKeyExW(parent, wide(OsStr::new(sub)).as_ptr(), 0, KEY_READ, &mut h) } == 0).then_some(Key(h))
    }
    fn text(key: &Key, name: &str) -> Option<String> {
        let mut buf = [0u16; 1024];
        let (mut ty, mut len) = (0u32, std::mem::size_of_val(&buf) as u32);
        let r = unsafe { RegQueryValueExW(key.0, wide(OsStr::new(name)).as_ptr(), null_mut(), &mut ty, buf.as_mut_ptr() as *mut u8, &mut len) };
        if r != 0 || (ty != REG_SZ && ty != REG_EXPAND_SZ) {
            return None;
        }
        let units = &buf[..(len as usize / 2).min(buf.len())];
        let end = units.iter().position(|c| *c == 0).unwrap_or(units.len());
        Some(String::from_utf16_lossy(&units[..end]))
    }

    let Some(root) = open(HKCU, r"Software\Microsoft\OneDrive\Accounts") else { return vec![] };
    let mut out = vec![];
    for i in 0..64 {
        let mut name = [0u16; 256];
        let mut len = name.len() as u32;
        if unsafe { RegEnumKeyExW(root.0, i, name.as_mut_ptr(), &mut len, null_mut(), null_mut(), null_mut(), null_mut()) } != 0 {
            break;
        }
        let key = String::from_utf16_lossy(&name[..len as usize]);
        if let Some(k) = open(root.0, &key) {
            out.push(Account { folder: text(&k, "UserFolder"), display: text(&k, "DisplayName"), email: text(&k, "UserEmail"), key });
        }
    }
    out
}

#[cfg(not(windows))]
fn read_accounts() -> Vec<Account> {
    vec![]
}

/// Ubicaciones de OneDrive de este equipo. Lista vacía si no está instalado o configurado.
pub fn onedrive_places() -> Vec<Place> {
    resolve(&read_accounts(), &|k| std::env::var(k).ok(), &|p| std::path::Path::new(p).is_dir())
}

/// ¿El contenido de esta ruta está solo en la nube? Pregunta solo por los atributos, sin abrir el archivo: no provoca descargas.
/// Una ruta que no existe o no se puede consultar responde `false`.
#[cfg(windows)]
pub fn path_is_cloud_only(path: &std::path::Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetFileAttributesW(name: *const u16) -> u32;
    }
    const INVALID: u32 = u32::MAX;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let attrs = unsafe { GetFileAttributesW(wide.as_ptr()) };
    attrs != INVALID && crate::dir::attrs_cloud_only(attrs)
}

#[cfg(not(windows))]
pub fn path_is_cloud_only(_path: &std::path::Path) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_attributes_exclude_folders() {
        use crate::dir::attrs_cloud_only as f;
        assert!(f(0x40_0000) && f(0x4_0000) && f(0x1000));
        assert!(!f(0x20), "archivo local");
        assert!(!f(0x10 | 0x40_0000), "una carpeta no se considera solo-nube");
    }

    #[cfg(windows)]
    #[test]
    fn reads_the_attribute_of_a_real_file() {
        let p = std::env::temp_dir().join(format!("lfind-cloud-{}.txt", std::process::id()));
        std::fs::write(&p, "x").unwrap();
        assert!(!path_is_cloud_only(&p));
        let attrib = |flag: &str| assert!(std::process::Command::new("attrib").arg(flag).arg(&p).status().unwrap().success());
        attrib("+O");
        assert!(path_is_cloud_only(&p));
        attrib("-O");
        assert!(!path_is_cloud_only(&p));
        std::fs::remove_file(&p).unwrap();
        assert!(!path_is_cloud_only(&p), "lo que ya no existe no es un error");
    }

    fn acc(key: &str, folder: Option<&str>, display: Option<&str>, email: Option<&str>) -> Account {
        Account { key: key.into(), folder: folder.map(String::from), display: display.map(String::from), email: email.map(String::from) }
    }
    const NO_ENV: fn(&str) -> Option<String> = |_| None;
    const ALL: fn(&str) -> bool = |_| true;
    fn labels(p: &[Place]) -> Vec<&str> {
        p.iter().map(|x| x.label.as_str()).collect()
    }

    #[test]
    fn personal_and_business_keep_their_real_folders() {
        let a = [acc("Personal", Some(r"E:\Nube"), None, Some("a@b")), acc("Business1", Some(r"D:\OneDrive - Contoso"), Some("Contoso"), Some("x@c"))];
        let p = resolve(&a, &NO_ENV, &ALL);
        assert_eq!(labels(&p), ["OneDrive - Personal", "OneDrive - Contoso"]);
        assert_eq!(p[0].path, r"E:\Nube", "no se asume C:\\Users\\…\\OneDrive");
        assert!(p.iter().all(|x| x.kind == PlaceKind::Cloud));
    }

    #[test]
    fn accounts_without_a_folder_or_with_a_missing_one_are_not_shown() {
        let a = [acc("Business1", None, None, None), acc("FileCoAuth", Some("  "), None, None), acc("Personal", Some(r"C:\Borrada"), None, None)];
        assert!(resolve(&a, &NO_ENV, &|_| false).is_empty());
        assert!(resolve(&a[..2], &NO_ENV, &ALL).is_empty());
    }

    #[test]
    fn two_accounts_are_never_mixed() {
        let a = [acc("Business1", Some(r"D:\A"), Some("Contoso"), Some("uno@c")), acc("Business2", Some(r"D:\B"), Some("Contoso"), Some("dos@c"))];
        assert_eq!(labels(&resolve(&a, &NO_ENV, &ALL)), ["OneDrive - Contoso (uno@c)", "OneDrive - Contoso (dos@c)"]);
    }

    #[test]
    fn the_same_folder_is_listed_once() {
        let a = [acc("Personal", Some(r"C:\X\"), None, None), acc("Business1", Some("c:/x"), Some("Y"), None)];
        assert_eq!(resolve(&a, &NO_ENV, &ALL).len(), 1);
    }

    #[test]
    fn environment_is_only_a_fallback() {
        let env = |k: &str| match k {
            "OneDrive" | "OneDriveConsumer" => Some(r"C:\Env".to_string()),
            _ => None,
        };
        assert_eq!(labels(&resolve(&[], &env, &ALL)), ["OneDrive - Personal"]);
        assert_eq!(resolve(&[acc("Personal", Some(r"C:\Reg"), None, None)], &env, &ALL)[0].path, r"C:\Reg");
    }

    #[test]
    fn real_accounts_on_this_machine_exist() {
        for p in onedrive_places() {
            assert!(std::path::Path::new(&p.path).is_dir(), "{}", p.label);
        }
    }
}
