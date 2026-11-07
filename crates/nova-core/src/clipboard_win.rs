//! Portapapeles de archivos de Windows (`CF_HDROP` + «Preferred DropEffect»). El formato está en `clipboard`.
//!
//! El propietario del portapapeles es una ventana *message-only* mínima (clase predefinida `STATIC`, sin `WndProc` propio)
//! que se crea en el hilo que llama y se destruye al terminar: con `OpenClipboard(NULL)` `SetClipboardData` falla, y el
//! propietario debe pertenecer al hilo que abre el portapapeles. Los datos se publican completos (sin renderizado
//! diferido), así que siguen disponibles aunque la ventana ya no exista.

use crate::clipboard::{build_dropfiles, parse_dropfiles, Mode};
use std::ffi::c_void;
use std::ptr::null_mut;

#[link(name = "user32")]
extern "system" {
    fn OpenClipboard(h: isize) -> i32;
    fn CloseClipboard() -> i32;
    fn EmptyClipboard() -> i32;
    fn SetClipboardData(fmt: u32, h: isize) -> isize;
    fn GetClipboardData(fmt: u32) -> isize;
    fn IsClipboardFormatAvailable(fmt: u32) -> i32;
    fn RegisterClipboardFormatW(name: *const u16) -> u32;
    fn GetClipboardSequenceNumber() -> u32;
    fn CreateWindowExW(ex: u32, class: *const u16, title: *const u16, style: u32, x: i32, y: i32, w: i32, h: i32, parent: isize, menu: isize, inst: isize, param: *const c_void) -> isize;
    fn DestroyWindow(h: isize) -> i32;
}
#[link(name = "kernel32")]
extern "system" {
    fn GlobalAlloc(flags: u32, bytes: usize) -> isize;
    fn GlobalFree(h: isize) -> isize;
    fn GlobalLock(h: isize) -> *mut c_void;
    fn GlobalUnlock(h: isize) -> i32;
    fn GlobalSize(h: isize) -> usize;
    fn Sleep(ms: u32);
}

const CF_HDROP: u32 = 15;
const GMEM_MOVEABLE: u32 = 0x0002;
const HWND_MESSAGE: isize = -3;

/// Ventana propietaria temporal (se destruye sola).
struct Owner(isize);
impl Owner {
    fn new() -> Result<Owner, String> {
        let class: Vec<u16> = "STATIC".encode_utf16().chain(std::iter::once(0)).collect();
        let h = unsafe { CreateWindowExW(0, class.as_ptr(), class.as_ptr(), 0, 0, 0, 0, 0, HWND_MESSAGE, 0, 0, std::ptr::null()) };
        if h == 0 {
            Err("No se pudo crear la ventana propietaria del portapapeles.".into())
        } else {
            Ok(Owner(h))
        }
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        unsafe { DestroyWindow(self.0) };
    }
}

fn drop_effect_format() -> u32 {
    let name: Vec<u16> = "Preferred DropEffect".encode_utf16().chain(std::iter::once(0)).collect();
    unsafe { RegisterClipboardFormatW(name.as_ptr()) }
}

/// El portapapeles lo puede tener abierto otra aplicación unos milisegundos: se reintenta un poco.
unsafe fn open(owner: isize) -> bool {
    for _ in 0..20 {
        if OpenClipboard(owner) != 0 {
            return true;
        }
        Sleep(10);
    }
    false
}

struct Open;
impl Drop for Open {
    fn drop(&mut self) {
        unsafe { CloseClipboard() };
    }
}

unsafe fn global_from(bytes: &[u8]) -> Result<isize, String> {
    let h = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1));
    if h == 0 {
        return Err("Sin memoria para el portapapeles.".into());
    }
    let p = GlobalLock(h);
    if p.is_null() {
        GlobalFree(h);
        return Err("No se pudo bloquear la memoria del portapapeles.".into());
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), p as *mut u8, bytes.len());
    GlobalUnlock(h);
    Ok(h)
}

/// Publica archivos en el portapapeles del sistema como copiar/cortar del Explorador.
pub fn write_files(paths: &[String], mode: Mode) -> Result<(), String> {
    if paths.is_empty() {
        return Err("No hay archivos que copiar.".into());
    }
    let owner = Owner::new()?;
    unsafe {
        let drop_h = global_from(&build_dropfiles(paths))?;
        let eff_h = match global_from(&mode.effect().to_le_bytes()) {
            Ok(h) => h,
            Err(e) => {
                GlobalFree(drop_h);
                return Err(e);
            }
        };
        if !open(owner.0) {
            GlobalFree(drop_h);
            GlobalFree(eff_h);
            return Err("El portapapeles está ocupado.".into());
        }
        let _open = Open;
        EmptyClipboard();
        // Si SetClipboardData acepta el bloque, el sistema es su dueño; si falla, hay que liberarlo.
        if SetClipboardData(CF_HDROP, drop_h) == 0 {
            GlobalFree(drop_h);
            GlobalFree(eff_h);
            return Err("SetClipboardData(CF_HDROP) falló.".into());
        }
        if SetClipboardData(drop_effect_format(), eff_h) == 0 {
            GlobalFree(eff_h);
        }
    }
    Ok(())
}

/// Archivos que hay ahora en el portapapeles del sistema (`None` si no hay ningún `CF_HDROP`).
pub fn read_files() -> Result<Option<(Vec<String>, Mode)>, String> {
    unsafe {
        if IsClipboardFormatAvailable(CF_HDROP) == 0 {
            return Ok(None);
        }
        if !open(0) {
            return Err("El portapapeles está ocupado.".into());
        }
        let _open = Open;
        let h = GetClipboardData(CF_HDROP);
        if h == 0 {
            return Ok(None);
        }
        let size = GlobalSize(h);
        let p = GlobalLock(h);
        if p.is_null() || size == 0 {
            return Ok(None);
        }
        let bytes = std::slice::from_raw_parts(p as *const u8, size).to_vec();
        GlobalUnlock(h);
        let paths = parse_dropfiles(&bytes);
        if paths.is_empty() {
            return Ok(None);
        }
        let mut mode = Mode::Copy;
        let fmt = drop_effect_format();
        if IsClipboardFormatAvailable(fmt) != 0 {
            let eh = GetClipboardData(fmt);
            if eh != 0 && GlobalSize(eh) >= 4 {
                let ep = GlobalLock(eh);
                if !ep.is_null() {
                    let v = std::ptr::read_unaligned(ep as *const u32);
                    GlobalUnlock(eh);
                    mode = Mode::from_effect(v);
                }
            }
        }
        Ok(Some((paths, mode)))
    }
}

/// ¿Hay archivos (`CF_HDROP`) en el portapapeles? Consulta barata: no abre el portapapeles ni copia datos.
pub fn has_files() -> bool {
    unsafe { IsClipboardFormatAvailable(CF_HDROP) != 0 }
}

/// Número de secuencia del portapapeles: cambia cada vez que alguien lo modifica (permite saber si lo cambió otra app).
pub fn sequence() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}

#[allow(dead_code)]
fn _unused(_: *mut c_void) -> *mut c_void {
    null_mut()
}
