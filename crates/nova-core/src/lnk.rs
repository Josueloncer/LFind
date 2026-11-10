//! Destino de un acceso directo de Windows (`.lnk`), para «Abrir ubicación del archivo».
//!
//! Se lee con `IShellLinkW` + `IPersistFile`, la API del propio shell, sin procesos externos. No se llama a `Resolve`: puede
//! buscar el destino por el disco y mostrar cuadros de diálogo, y aquí solo se quiere saber adónde apunta.
//! Los accesos directos «anunciados» de Windows Installer (Office y otros) no guardan una ruta: se resuelven con `msi.dll`.

use serde::Serialize;
use std::path::Path;

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutTarget {
    /// Ruta completa del destino.
    pub path: String,
    pub is_dir: bool,
    /// Argumentos del acceso directo (solo informativos: no cambian el destino).
    pub arguments: String,
}

/// Es un acceso directo de Windows por su extensión.
pub fn is_shortcut(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("lnk"))
}

/// Comprueba el destino leído del acceso directo. Separado de la lectura para poder probarlo sin COM.
pub fn check_target(raw: &str, arguments: String) -> Result<ShortcutTarget, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("Este acceso directo no apunta a ningún archivo ni carpeta.".into());
    }
    let p = Path::new(raw);
    match std::fs::metadata(p) {
        Ok(m) => Ok(ShortcutTarget { path: raw.to_string(), is_dir: m.is_dir(), arguments }),
        Err(_) => Err(format!("El destino de este acceso directo ya no existe: {raw}")),
    }
}

/// Destino real del acceso directo, o el motivo en español de por qué no se puede abrir su ubicación.
pub fn resolve(link: &Path) -> Result<ShortcutTarget, String> {
    if !is_shortcut(link) {
        return Err("Este elemento no es un acceso directo.".into());
    }
    if std::fs::metadata(link).is_err() {
        return Err("El acceso directo ya no existe.".into());
    }
    let link = link.to_path_buf();
    // Hilo propio: COM se inicializa a su manera y no choca con lo que ya haya en los hilos compartidos.
    std::thread::Builder::new()
        .name("lfind-lnk".into())
        .spawn(move || imp::read(&link))
        .map_err(|e| e.to_string())?
        .join()
        .map_err(|_| "Error interno al leer el acceso directo.".to_string())?
}

#[cfg(windows)]
mod imp {
    use super::{check_target, ShortcutTarget};
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::ptr::null_mut;

    type P = *mut c_void;
    #[repr(C)]
    struct Guid(u32, u16, u16, [u8; 8]);

    const CLSID_SHELL_LINK: Guid = Guid(0x0002_1401, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
    const IID_SHELL_LINK_W: Guid = Guid(0x0002_14F9, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
    const IID_PERSIST_FILE: Guid = Guid(0x0000_010B, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
    const IID_DATA_LIST: Guid = Guid(0x45E2_B4AE, 0xB1C3, 0x11D0, [0xB9, 0x2F, 0x00, 0xA0, 0xC9, 0x03, 0x12, 0xE1]);
    const CLSCTX_INPROC_SERVER: u32 = 0x1;
    const COINIT_STA_NO_DDE: u32 = 0x2 | 0x4;
    const STGM_READ: u32 = 0;
    const SLDF_HAS_DARWINID: u32 = 0x1000;
    const SIGDN_NORMALDISPLAY: u32 = 0;
    const MAX: usize = 32_768;

    #[link(name = "ole32")]
    extern "system" {
        fn CoInitializeEx(reserved: P, flags: u32) -> i32;
        fn CoUninitialize();
        fn CoCreateInstance(clsid: *const Guid, outer: P, ctx: u32, iid: *const Guid, out: *mut P) -> i32;
        fn CoTaskMemFree(p: P);
    }
    #[link(name = "shell32")]
    extern "system" {
        fn SHGetNameFromIDList(pidl: P, sigdn: u32, name: *mut *mut u16) -> i32;
    }
    #[link(name = "msi")]
    extern "system" {
        fn MsiGetShortcutTargetW(link: *const u16, product: *mut u16, feature: *mut u16, component: *mut u16) -> u32;
        fn MsiGetComponentPathW(product: *const u16, component: *const u16, buf: *mut u16, len: *mut u32) -> i32;
    }

    #[repr(C)]
    struct Unknown {
        query: unsafe extern "system" fn(P, *const Guid, *mut P) -> i32,
        add_ref: P,
        release: unsafe extern "system" fn(P) -> u32,
    }
    #[repr(C)]
    struct ShellLinkW {
        unknown: Unknown,
        get_path: unsafe extern "system" fn(P, *mut u16, i32, P, u32) -> i32,
        get_id_list: unsafe extern "system" fn(P, *mut P) -> i32,
        _set_id_list: P,
        _get_description: P,
        _set_description: P,
        _get_working_directory: P,
        _set_working_directory: P,
        get_arguments: unsafe extern "system" fn(P, *mut u16, i32) -> i32,
    }
    #[repr(C)]
    struct PersistFile {
        unknown: Unknown,
        _get_class_id: P,
        _is_dirty: P,
        load: unsafe extern "system" fn(P, *const u16, u32) -> i32,
    }
    #[repr(C)]
    struct DataList {
        unknown: Unknown,
        _add: P,
        _copy: P,
        _remove: P,
        get_flags: unsafe extern "system" fn(P, *mut u32) -> i32,
    }

    /// Libera la interfaz al salir, pase lo que pase.
    struct Com(P);
    impl Drop for Com {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { ((**(self.0 as *mut *const Unknown)).release)(self.0) };
            }
        }
    }
    impl Com {
        unsafe fn vt<T>(&self) -> &T {
            &**(self.0 as *mut *const T)
        }
        unsafe fn query(&self, iid: &Guid) -> Option<Com> {
            let mut out: P = null_mut();
            ((*self.vt::<Unknown>()).query)(self.0, iid, &mut out);
            (!out.is_null()).then_some(Com(out))
        }
    }

    fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
        s.encode_wide().chain(Some(0)).collect()
    }
    fn string(buf: &[u16]) -> String {
        String::from_utf16_lossy(&buf[..buf.iter().position(|c| *c == 0).unwrap_or(buf.len())])
    }

    pub fn read(link: &Path) -> Result<ShortcutTarget, String> {
        let hr = unsafe { CoInitializeEx(null_mut(), COINIT_STA_NO_DDE) };
        let r = unsafe { read_with_com(link) };
        if hr >= 0 {
            unsafe { CoUninitialize() };
        }
        r
    }

    unsafe fn read_with_com(link: &Path) -> Result<ShortcutTarget, String> {
        let mut obj: P = null_mut();
        if CoCreateInstance(&CLSID_SHELL_LINK, null_mut(), CLSCTX_INPROC_SERVER, &IID_SHELL_LINK_W, &mut obj) < 0 || obj.is_null() {
            return Err("Windows no ha podido abrir el acceso directo.".into());
        }
        let sl = Com(obj);
        let pf = sl.query(&IID_PERSIST_FILE).ok_or("Windows no ha podido abrir el acceso directo.")?;
        let wlink = wide(link.as_os_str());
        if ((*pf.vt::<PersistFile>()).load)(pf.0, wlink.as_ptr(), STGM_READ) < 0 {
            return Err("El acceso directo está dañado o no se puede leer.".into());
        }
        let mut args = vec![0u16; MAX];
        ((*sl.vt::<ShellLinkW>()).get_arguments)(sl.0, args.as_mut_ptr(), MAX as i32);
        let arguments = string(&args);

        // Instalado por Windows Installer: no guarda una ruta, la sabe el instalador.
        if let Some(dl) = sl.query(&IID_DATA_LIST) {
            let mut flags = 0u32;
            if ((*dl.vt::<DataList>()).get_flags)(dl.0, &mut flags) >= 0 && flags & SLDF_HAS_DARWINID != 0 {
                return darwin(&wlink).map_or_else(
                    || Err("Este acceso directo lo gestiona Windows Installer y no indica dónde está la aplicación instalada.".into()),
                    |p| check_target(&p, arguments),
                );
            }
        }

        let mut path = vec![0u16; MAX];
        ((*sl.vt::<ShellLinkW>()).get_path)(sl.0, path.as_mut_ptr(), MAX as i32, null_mut(), 0);
        let path = string(&path);
        if !path.trim().is_empty() {
            return check_target(&path, arguments);
        }
        // Sin ruta: apunta a algo del shell que no es un archivo («Este equipo», el Panel de control…).
        let mut pidl: P = null_mut();
        let name = if ((*sl.vt::<ShellLinkW>()).get_id_list)(sl.0, &mut pidl) >= 0 && !pidl.is_null() {
            let mut out: *mut u16 = null_mut();
            let got = SHGetNameFromIDList(pidl, SIGDN_NORMALDISPLAY, &mut out) >= 0 && !out.is_null();
            let n = if got {
                let len = (0..).take_while(|&i| *out.add(i) != 0).count();
                let s = String::from_utf16_lossy(std::slice::from_raw_parts(out, len));
                CoTaskMemFree(out as P);
                Some(s)
            } else {
                None
            };
            CoTaskMemFree(pidl);
            n
        } else {
            None
        };
        Err(match name.filter(|n| !n.trim().is_empty()) {
            Some(n) => format!("Este acceso directo apunta a «{n}», un elemento especial de Windows que no es un archivo ni una carpeta."),
            None => "Este acceso directo no apunta a ningún archivo ni carpeta.".into(),
        })
    }

    unsafe fn darwin(wlink: &[u16]) -> Option<String> {
        let (mut product, mut feature, mut component) = ([0u16; 40], [0u16; 40], [0u16; 40]);
        if MsiGetShortcutTargetW(wlink.as_ptr(), product.as_mut_ptr(), feature.as_mut_ptr(), component.as_mut_ptr()) != 0 {
            return None;
        }
        let mut buf = vec![0u16; MAX];
        let mut len = MAX as u32;
        // INSTALLSTATE_LOCAL (3) o SOURCE (4): hay una ruta utilizable.
        let state = MsiGetComponentPathW(product.as_ptr(), component.as_ptr(), buf.as_mut_ptr(), &mut len);
        (state == 3 || state == 4).then(|| string(&buf)).filter(|s| !s.is_empty())
    }
}

#[cfg(not(windows))]
mod imp {
    use super::ShortcutTarget;
    use std::path::Path;
    pub fn read(_link: &Path) -> Result<ShortcutTarget, String> {
        Err("Los accesos directos de Windows solo se pueden leer en Windows.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("lfind-lnk-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn only_lnk_files_are_shortcuts() {
        assert!(is_shortcut(Path::new(r"C:\a\Bloc.LNK")));
        assert!(!is_shortcut(Path::new(r"C:\a\nota.txt")));
        assert!(!is_shortcut(Path::new(r"C:\a\web.url")));
        assert!(resolve(Path::new("nota.txt")).unwrap_err().contains("no es un acceso directo"));
    }

    #[test]
    fn checks_that_the_target_exists() {
        let d = tmp("check");
        fs::write(d.join("doc.txt"), "x").unwrap();
        let t = check_target(&d.join("doc.txt").display().to_string(), "/a".into()).unwrap();
        assert!(!t.is_dir && t.arguments == "/a");
        assert!(check_target(&d.display().to_string(), String::new()).unwrap().is_dir);
        assert!(check_target(&d.join("no.txt").display().to_string(), String::new()).unwrap_err().contains("ya no existe"));
        assert!(check_target("  ", String::new()).unwrap_err().contains("no apunta"));
        let _ = fs::remove_dir_all(d);
    }

    /// Accesos directos reales, creados con el objeto del shell como haría cualquier programa.
    #[cfg(windows)]
    #[test]
    fn reads_real_shortcuts() {
        use std::os::windows::process::CommandExt;
        let d = tmp("real");
        fs::create_dir_all(d.join("carpeta destino")).unwrap();
        fs::write(d.join("informe ñ (1).txt"), "x").unwrap();
        fs::write(d.join("borrado.txt"), "x").unwrap();
        let make = |name: &str, target: &Path, args: &str| {
            let s = "$s=(New-Object -ComObject WScript.Shell).CreateShortcut($env:L);$s.TargetPath=$env:T;$s.Arguments=$env:A;$s.Save()";
            let ok = std::process::Command::new("powershell.exe")
                .args(["-NoProfile", "-NonInteractive", "-Command", s])
                .env("L", d.join(name))
                .env("T", target)
                .env("A", args)
                .creation_flags(0x0800_0000)
                .status()
                .unwrap()
                .success();
            assert!(ok, "{name}");
            d.join(name)
        };
        let doc = make("doc.lnk", &d.join("informe ñ (1).txt"), "");
        let dir = make("dir.lnk", &d.join("carpeta destino"), "");
        let exe = make("exe.lnk", Path::new(r"C:\Windows\System32\notepad.exe"), "/A \"C:\\x y.txt\"");
        let gone = make("gone.lnk", &d.join("borrado.txt"), "");
        fs::remove_file(d.join("borrado.txt")).unwrap();

        let t = resolve(&doc).unwrap();
        assert_eq!(Path::new(&t.path), d.join("informe ñ (1).txt"));
        assert!(!t.is_dir);
        let t = resolve(&dir).unwrap();
        assert_eq!(Path::new(&t.path), d.join("carpeta destino"));
        assert!(t.is_dir);
        let t = resolve(&exe).unwrap();
        assert!(t.path.eq_ignore_ascii_case(r"C:\Windows\System32\notepad.exe"), "{}", t.path);
        assert_eq!(t.arguments, "/A \"C:\\x y.txt\"");
        let e = resolve(&gone).unwrap_err();
        assert!(e.contains("ya no existe") && e.contains("borrado.txt"), "{e}");
        fs::write(d.join("roto.lnk"), "no soy un acceso directo").unwrap();
        assert!(resolve(&d.join("roto.lnk")).unwrap_err().contains("dañado"));
        let _ = fs::remove_dir_all(d);
    }
}
