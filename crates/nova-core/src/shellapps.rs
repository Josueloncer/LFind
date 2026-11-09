//! «Abrir con» y arrastre hacia otras aplicaciones, con las interfaces nativas de Windows. No interviene ningún proceso intermedio.

use std::path::Path;

#[cfg(windows)]
mod imp {
    use super::*;
    use std::ffi::c_void;
    use std::ptr::{null, null_mut};

    type P = *mut c_void;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Guid(u32, u16, u16, [u8; 8]);

    #[repr(C)]
    struct EnumVtbl {
        _qi: usize,
        _add: usize,
        release: unsafe extern "system" fn(P) -> u32,
        next: unsafe extern "system" fn(P, u32, *mut P, *mut u32) -> i32,
    }
    #[repr(C)]
    struct HandlerVtbl {
        _qi: usize,
        _add: usize,
        release: unsafe extern "system" fn(P) -> u32,
        get_name: unsafe extern "system" fn(P, *mut *mut u16) -> i32,
        get_ui_name: unsafe extern "system" fn(P, *mut *mut u16) -> i32,
        _icon: usize,
        _recommended: usize,
        _default: usize,
        invoke: unsafe extern "system" fn(P, P) -> i32,
    }
    /// Sirve para `IShellItem` e `IShellItemArray`: ambos empiezan igual.
    #[repr(C)]
    struct BindVtbl {
        _qi: usize,
        _add: usize,
        release: unsafe extern "system" fn(P) -> u32,
        bind: unsafe extern "system" fn(P, P, *const Guid, *const Guid, *mut P) -> i32,
    }
    #[repr(C)]
    struct UnknownVtbl {
        _qi: usize,
        _add: usize,
        release: unsafe extern "system" fn(P) -> u32,
    }
    #[repr(C)]
    struct OpenAsInfo {
        file: *const u16,
        class: *const u16,
        flags: u32,
    }

    #[link(name = "shell32")]
    extern "system" {
        fn SHAssocEnumHandlers(ext: *const u16, filter: u32, out: *mut P) -> i32;
        fn SHCreateItemFromParsingName(path: *const u16, ctx: P, riid: *const Guid, out: *mut P) -> i32;
        fn SHOpenWithDialog(hwnd: isize, info: *const OpenAsInfo) -> i32;
        fn SHParseDisplayName(name: *const u16, ctx: P, pidl: *mut P, sfgao: u32, out: *mut u32) -> i32;
        fn SHCreateShellItemArrayFromIDLists(n: u32, list: *const P, out: *mut P) -> i32;
        fn SHDoDragDrop(hwnd: isize, data: P, source: P, ok: u32, effect: *mut u32) -> i32;
    }
    #[link(name = "ole32")]
    extern "system" {
        fn CoInitializeEx(r: P, c: u32) -> i32;
        fn CoUninitialize();
        fn CoTaskMemFree(p: P);
    }

    const IID_ISHELLITEM: Guid = Guid(0x43826D1E, 0xE718, 0x42EE, [0xBC, 0x55, 0xA1, 0xE2, 0x61, 0xC3, 0x7B, 0xFE]);
    const IID_IDATAOBJECT: Guid = Guid(0x0000010E, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
    const BHID_DATAOBJECT: Guid = Guid(0xB8C0BD9F, 0xED24, 0x455C, [0x83, 0xE6, 0xD5, 0x39, 0x0C, 0x4F, 0xE8, 0xC4]);
    const ASSOC_FILTER_RECOMMENDED: u32 = 1;
    const DROPEFFECT_ALL: u32 = 1 | 2 | 4;
    const ERROR_CANCELLED: i32 = 0x800704C7u32 as i32;
    const DRAGDROP_S_DROP: i32 = 0x40100;
    const DRAGDROP_S_CANCEL: i32 = 0x40101;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }
    unsafe fn take_string(p: *mut u16) -> String {
        if p.is_null() {
            return String::new();
        }
        let n = (0..).take_while(|i| *p.add(*i) != 0).count();
        let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, n));
        CoTaskMemFree(p as P);
        s
    }

    /// Las interfaces del shell quieren un hilo STA propio: así ninguna operación retiene a otra.
    fn in_sta<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> Result<R, String> {
        std::thread::Builder::new()
            .name("lfind-shell".into())
            .spawn(move || unsafe {
                let com = CoInitializeEx(null_mut(), 2 | 4) >= 0;
                let r = f();
                if com {
                    CoUninitialize();
                }
                r
            })
            .map_err(|e| e.to_string())?
            .join()
            .map_err(|_| "Error interno al consultar las aplicaciones.".to_string())
    }

    /// Recorre las aplicaciones recomendadas para `ext`. `visit` devuelve `true` para terminar.
    unsafe fn each_handler(ext: &str, mut visit: impl FnMut(P, &HandlerVtbl, String, String) -> bool) -> Result<(), String> {
        let name = wide(ext);
        let mut e: P = null_mut();
        let hr = SHAssocEnumHandlers(name.as_ptr(), ASSOC_FILTER_RECOMMENDED, &mut e);
        if hr < 0 || e.is_null() {
            return Err(format!("Windows no pudo listar las aplicaciones ({hr:#x})."));
        }
        let vt = *(e as *mut *const EnumVtbl);
        loop {
            let (mut h, mut got): (P, u32) = (null_mut(), 0);
            if ((*vt).next)(e, 1, &mut h, &mut got) != 0 || got == 0 || h.is_null() {
                break;
            }
            let hv = *(h as *mut *const HandlerVtbl);
            let (mut a, mut b): (*mut u16, *mut u16) = (null_mut(), null_mut());
            ((*hv).get_name)(h, &mut a);
            ((*hv).get_ui_name)(h, &mut b);
            let (exe, label) = (take_string(a), take_string(b));
            let stop = visit(h, &*hv, label, exe);
            ((*hv).release)(h);
            if stop {
                break;
            }
        }
        ((*vt).release)(e);
        Ok(())
    }

    /// El ejecutable existe, o Windows lo resuelve por su cuenta (aplicaciones de la Tienda, sin ruta absoluta).
    fn installed(exe: &str) -> bool {
        let p = Path::new(exe);
        !p.is_absolute() || p.exists()
    }

    pub fn handlers(ext: &str) -> Result<Vec<String>, String> {
        if ext.len() < 2 {
            return Ok(vec![]);
        }
        let ext = ext.to_string();
        in_sta(move || unsafe {
            let mut out: Vec<String> = vec![];
            each_handler(&ext, |_, _, label, exe| {
                if !label.is_empty() && installed(&exe) && !out.contains(&label) {
                    out.push(label);
                }
                false
            })?;
            Ok(out)
        })?
    }

    pub fn open_with_handler(path: &Path, app: &str) -> Result<(), String> {
        let (file, app) = (wide(&path.to_string_lossy()), app.to_string());
        let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
        in_sta(move || unsafe {
            let mut item: P = null_mut();
            let hr = SHCreateItemFromParsingName(file.as_ptr(), null_mut(), &IID_ISHELLITEM, &mut item);
            if hr < 0 {
                return Err(format!("Windows no encuentra el archivo ({hr:#x})."));
            }
            let vt = *(item as *mut *const BindVtbl);
            let mut data: P = null_mut();
            let hr = ((*vt).bind)(item, null_mut(), &BHID_DATAOBJECT, &IID_IDATAOBJECT, &mut data);
            ((*vt).release)(item);
            if hr < 0 {
                return Err(format!("Windows no pudo preparar el archivo ({hr:#x})."));
            }
            let mut result: Option<i32> = None;
            let walked = each_handler(&ext, |h, hv, label, _| {
                if label == app {
                    result = Some((hv.invoke)(h, data));
                    return true;
                }
                false
            });
            ((**(data as *mut *const UnknownVtbl)).release)(data);
            walked?;
            match result {
                Some(hr) if hr >= 0 => Ok(()),
                Some(hr) => Err(format!("«{app}» no pudo abrir el archivo ({hr:#x}).")),
                None => Err(format!("«{app}» ya no está disponible para este tipo de archivo.")),
            }
        })?
    }

    /// El cuadro «Abrir con» de Windows. Cancelarlo no es un error.
    pub fn choose_app(path: &Path) -> Result<(), String> {
        const OAIF_ALLOW_REGISTRATION: u32 = 1;
        const OAIF_EXEC: u32 = 4;
        let file = wide(&path.to_string_lossy());
        in_sta(move || unsafe {
            let info = OpenAsInfo { file: file.as_ptr(), class: null(), flags: OAIF_ALLOW_REGISTRATION | OAIF_EXEC };
            let hr = SHOpenWithDialog(0, &info);
            if hr >= 0 || hr == ERROR_CANCELLED { Ok(()) } else { Err(format!("Windows no pudo mostrar «Abrir con» ({hr:#x}).")) }
        })?
    }

    /// Arrastre OLE real. Debe llamarse en el hilo de la ventana. Devuelve el efecto: 0 cancelado, 1 copia, 2 movimiento, 4 enlace.
    pub unsafe fn drag_files(hwnd: isize, paths: &[String]) -> Result<u32, String> {
        let mut pidls: Vec<P> = Vec::with_capacity(paths.len());
        for p in paths {
            let w = wide(p);
            let mut pidl: P = null_mut();
            if SHParseDisplayName(w.as_ptr(), null_mut(), &mut pidl, 0, null_mut()) >= 0 && !pidl.is_null() {
                pidls.push(pidl);
            }
        }
        let free = |list: &[P]| list.iter().for_each(|p| CoTaskMemFree(*p));
        if pidls.is_empty() {
            return Err("Esos elementos ya no existen.".into());
        }
        let mut array: P = null_mut();
        let hr = SHCreateShellItemArrayFromIDLists(pidls.len() as u32, pidls.as_ptr(), &mut array);
        if hr < 0 || array.is_null() {
            free(&pidls);
            return Err(format!("Windows no pudo preparar el arrastre ({hr:#x})."));
        }
        let vt = *(array as *mut *const BindVtbl);
        let mut data: P = null_mut();
        let hr = ((*vt).bind)(array, null_mut(), &BHID_DATAOBJECT, &IID_IDATAOBJECT, &mut data);
        ((*vt).release)(array);
        free(&pidls);
        if hr < 0 || data.is_null() {
            return Err(format!("Windows no pudo preparar el arrastre ({hr:#x})."));
        }
        let mut effect = 0u32;
        let hr = SHDoDragDrop(hwnd, data, null_mut(), DROPEFFECT_ALL, &mut effect);
        ((**(data as *mut *const UnknownVtbl)).release)(data);
        match hr {
            DRAGDROP_S_DROP => Ok(effect),
            DRAGDROP_S_CANCEL => Ok(0),
            other if other >= 0 => Ok(effect),
            other => Err(format!("Windows no pudo completar el arrastre ({other:#x}).")),
        }
    }
}

#[cfg(windows)]
pub use imp::{choose_app, drag_files, handlers, open_with_handler};

#[cfg(not(windows))]
pub fn handlers(_ext: &str) -> Result<Vec<String>, String> {
    Ok(vec![])
}
#[cfg(not(windows))]
pub fn open_with_handler(_p: &Path, _app: &str) -> Result<(), String> {
    Err("Solo disponible en Windows.".into())
}
#[cfg(not(windows))]
pub fn choose_app(_p: &Path) -> Result<(), String> {
    Err("Solo disponible en Windows.".into())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn lists_applications_for_a_common_type() {
        let apps = handlers(".txt").unwrap();
        assert!(!apps.is_empty(), "{apps:?}");
        let mut unique = apps.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), apps.len(), "sin repetidos");
    }

    #[test]
    fn a_file_without_extension_has_no_list_but_is_not_an_error() {
        assert!(handlers("").unwrap().is_empty());
    }

    #[test]
    fn an_unknown_application_is_a_clear_error() {
        let d = std::env::temp_dir().join(format!("lfind-open-{}.txt", std::process::id()));
        std::fs::write(&d, b"x").unwrap();
        let e = open_with_handler(&d, "Aplicación inexistente").unwrap_err();
        assert!(e.contains("no está disponible"), "{e}");
        std::fs::remove_file(d).unwrap();
    }

    #[test]
    fn a_missing_file_is_a_clear_error() {
        assert!(open_with_handler(Path::new("C:\\no\\existe.txt"), "x").is_err());
    }
}
