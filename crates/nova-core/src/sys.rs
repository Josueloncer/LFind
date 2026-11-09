//! Llamadas nativas mínimas. En Windows se declaran a mano (sin crates) igual que
//! `GetLogicalDrives` en `places.rs`; fuera de Windows existen alternativas simples.

#[cfg(windows)]
pub mod win {
    use std::ffi::{c_void, OsStr};
    use std::os::windows::ffi::OsStrExt;

    pub fn wide(s: &OsStr) -> Vec<u16> {
        s.encode_wide().chain(Some(0)).collect()
    }

    #[link(name = "kernel32")]
    extern "system" {
        pub fn GetDiskFreeSpaceExW(dir: *const u16, avail: *mut u64, total: *mut u64, free: *mut u64) -> i32;
        pub fn GetDriveTypeW(root: *const u16) -> u32;
        pub fn GetVolumeInformationW(
            root: *const u16, name: *mut u16, name_len: u32, serial: *mut u32, max_comp: *mut u32,
            flags: *mut u32, fs_name: *mut u16, fs_len: u32,
        ) -> i32;
        pub fn SetFileAttributesW(path: *const u16, attrs: u32) -> i32;
        pub fn GetCurrentThread() -> *mut c_void;
        pub fn SetThreadPriority(thread: *mut c_void, priority: i32) -> i32;
    }

    #[link(name = "shell32")]
    extern "system" {
        pub fn ShellExecuteW(hwnd: *mut c_void, op: *const u16, file: *const u16, params: *const u16, dir: *const u16, show: i32) -> isize;
    }

    #[link(name = "ole32")]
    extern "system" {
        pub fn CoInitializeEx(reserved: *mut c_void, coinit: u32) -> i32;
        pub fn CoUninitialize();
    }
}

/// Por compatibilidad, Windows disfraza los archivos de la nube al enumerarlos: uno ya descargado parece un archivo normal.
/// Mientras dure este guardia, el hilo ve sus atributos reales. Solo cambia lo que se lee, no el archivo ni su sincronización.
pub struct ExposePlaceholders {
    #[cfg(windows)]
    restore: Option<(unsafe extern "system" fn(i8) -> i8, i8)>,
}

impl ExposePlaceholders {
    #[cfg(windows)]
    pub fn new() -> Self {
        use std::ffi::c_void;
        #[link(name = "kernel32")]
        extern "system" {
            fn GetModuleHandleW(name: *const u16) -> *mut c_void;
            fn GetProcAddress(m: *mut c_void, name: *const u8) -> *mut c_void;
        }
        const PHCM_EXPOSE_PLACEHOLDERS: i8 = 2;
        let module: Vec<u16> = "ntdll.dll".encode_utf16().chain(Some(0)).collect();
        // Se busca en ejecución: no existe en versiones antiguas de Windows 10.
        let f = unsafe { GetProcAddress(GetModuleHandleW(module.as_ptr()), b"RtlSetThreadPlaceholderCompatibilityMode\0".as_ptr()) };
        if f.is_null() {
            return ExposePlaceholders { restore: None };
        }
        let set: unsafe extern "system" fn(i8) -> i8 = unsafe { std::mem::transmute(f) };
        let old = unsafe { set(PHCM_EXPOSE_PLACEHOLDERS) };
        ExposePlaceholders { restore: Some((set, old)) }
    }

    #[cfg(not(windows))]
    pub fn new() -> Self {
        ExposePlaceholders {}
    }
}

impl Drop for ExposePlaceholders {
    fn drop(&mut self) {
        #[cfg(windows)]
        if let Some((set, old)) = self.restore {
            unsafe { set(old) };
        }
    }
}

/// Pasa el hilo actual a segundo plano: prioridad baja de CPU y de disco. Dura lo que el hilo.
pub fn background_thread() {
    #[cfg(windows)]
    unsafe {
        const THREAD_MODE_BACKGROUND_BEGIN: i32 = 0x0001_0000;
        win::SetThreadPriority(win::GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN);
    }
}

/// Copia los atributos de solo lectura/oculto/sistema/archivo al archivo recién creado.
#[cfg(windows)]
pub fn copy_basic_attrs(src_attrs: u32, dst: &std::path::Path) {
    let a = src_attrs & 0x27; // READONLY | HIDDEN | SYSTEM | ARCHIVE
    if a != 0 {
        let w = win::wide(dst.as_os_str());
        unsafe { win::SetFileAttributesW(w.as_ptr(), a) };
    }
}
