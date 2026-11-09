//! Iconos del sistema de Windows, los mismos que muestra el Explorador.
//!
//! * `type_icon`: icono asociado a un TIPO de archivo (o a «carpeta»). `SHGetFileInfoW` con
//!   `SHGFI_USEFILEATTRIBUTES` resuelve el icono solo con la extensión: NO abre ni toca ningún archivo.
//! * `file_icon`: icono PROPIO de un archivo o carpeta existente (recursos de un EXE, `IconLocation` de un
//!   LNK, `IconFile` de un URL, `desktop.ini` de una carpeta, icono de una unidad). Es lo que hace el Explorador:
//!   el shell lee el recurso de icono; no ejecuta el programa.
//! * Ambos obtienen un índice de la lista de imágenes del sistema y piden el icono al tamaño necesario
//!   (`SHGetImageList`: 32, 48 o 256 px) con `ImageList_GetIcon`, sin usar la interfaz `IImageList`.
//!
//! No usa la miniatura del archivo (eso es `thumbs_win`, con `SIIGBF_THUMBNAILONLY`): son dos servicios distintos.
//! Declarado a mano (sin la crate `windows`), como el resto de llamadas nativas. Cada llamada corre en un hilo
//! del servicio con su propio COM STA (ver `icons::IconService`), con tiempo límite y sin bloquear la interfaz.

use crate::icons::{without_corner_padding, IconProvider, RawIcon};
use std::ffi::c_void;
use std::path::Path;
use std::ptr::null_mut;

#[repr(C)]
#[derive(Clone, Copy)]
struct Guid {
    d1: u32,
    d2: u16,
    d3: u16,
    d4: [u8; 8],
}

/// {46EB5926-582E-4017-9FDF-E8998DAA0950}
const IID_IIMAGELIST: Guid = Guid { d1: 0x46EB5926, d2: 0x582E, d3: 0x4017, d4: [0x9F, 0xDF, 0xE8, 0x99, 0x8D, 0xAA, 0x09, 0x50] };

#[repr(C)]
struct UnknownVtbl {
    query_interface: unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
}

#[repr(C)]
struct Unknown {
    vtbl: *const UnknownVtbl,
}

#[repr(C)]
struct ShFileInfoW {
    hicon: isize,
    icon: i32,
    attributes: u32,
    display_name: [u16; 260],
    type_name: [u16; 80],
}

#[repr(C)]
struct IconInfo {
    is_icon: i32,
    x_hotspot: u32,
    y_hotspot: u32,
    mask: isize,
    color: isize,
}

#[repr(C)]
struct BitmapInfoHeader {
    size: u32,
    width: i32,
    height: i32,
    planes: u16,
    bit_count: u16,
    compression: u32,
    size_image: u32,
    x_ppm: i32,
    y_ppm: i32,
    clr_used: u32,
    clr_important: u32,
}

#[repr(C)]
struct BitmapInfo {
    header: BitmapInfoHeader,
    colors: [u32; 1],
}

#[repr(C)]
struct Bitmap {
    bm_type: i32,
    width: i32,
    height: i32,
    width_bytes: i32,
    planes: u16,
    bits_pixel: u16,
    bits: *mut c_void,
}

// Si el diseño no coincide con el de Windows, no compila (en lugar de fallar en ejecución).
const _: () = assert!(std::mem::size_of::<BitmapInfoHeader>() == 40);
#[cfg(target_pointer_width = "64")]
const _: () = assert!(std::mem::size_of::<Bitmap>() == 32);
#[cfg(target_pointer_width = "64")]
const _: () = assert!(std::mem::size_of::<ShFileInfoW>() == 696);
#[cfg(target_pointer_width = "64")]
const _: () = assert!(std::mem::size_of::<IconInfo>() == 32);

#[link(name = "shell32")]
extern "system" {
    fn SHGetFileInfoW(path: *const u16, attrs: u32, info: *mut ShFileInfoW, cb: u32, flags: u32) -> usize;
    fn SHGetImageList(list: i32, riid: *const Guid, out: *mut *mut c_void) -> i32;
}

#[link(name = "comctl32")]
extern "system" {
    /// Una `IImageList*` del shell es también un `HIMAGELIST`.
    fn ImageList_GetIcon(himl: *mut c_void, index: i32, flags: u32) -> isize;
}

#[link(name = "user32")]
extern "system" {
    fn GetIconInfo(hicon: isize, info: *mut IconInfo) -> i32;
    fn DestroyIcon(hicon: isize) -> i32;
}

#[link(name = "ole32")]
extern "system" {
    fn CoInitializeEx(reserved: *mut c_void, coinit: u32) -> i32;
    fn CoUninitialize();
}

#[link(name = "gdi32")]
extern "system" {
    fn GetObjectW(h: isize, c: i32, pv: *mut c_void) -> i32;
    fn CreateCompatibleDC(hdc: isize) -> isize;
    fn GetDIBits(hdc: isize, hbm: isize, start: u32, lines: u32, bits: *mut c_void, bmi: *mut BitmapInfo, usage: u32) -> i32;
    fn DeleteDC(hdc: isize) -> i32;
    fn DeleteObject(h: isize) -> i32;
}

const COINIT_APARTMENTTHREADED: u32 = 0x2;
const COINIT_DISABLE_OLE1DDE: u32 = 0x4;
const SHGFI_SYSICONINDEX: u32 = 0x4000;
const SHGFI_USEFILEATTRIBUTES: u32 = 0x10;
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;
const SHIL_LARGE: i32 = 0; // 32 px
const SHIL_EXTRALARGE: i32 = 2; // 48 px
const SHIL_JUMBO: i32 = 4; // 256 px
const ILD_TRANSPARENT: u32 = 1;
/// Dimensión máxima que se acepta de un icono (protege contra manejadores que devuelvan algo absurdo).
const MAX_DIM: i32 = 1024;

pub struct WinIcons;

impl IconProvider for WinIcons {
    fn type_icon(&self, ext: &str, dir: bool, px: u32) -> Result<RawIcon, String> {
        let name = if dir { "carpeta".to_string() } else if ext.is_empty() { "archivo".to_string() } else { format!("archivo.{ext}") };
        let (attrs, flags) = (if dir { FILE_ATTRIBUTE_DIRECTORY } else { FILE_ATTRIBUTE_NORMAL }, SHGFI_SYSICONINDEX | SHGFI_USEFILEATTRIBUTES);
        unsafe { with_com(|| icon_for(&name, attrs, flags, px)) }
    }

    fn file_icon(&self, path: &Path, px: u32) -> Result<RawIcon, String> {
        let name = path.to_string_lossy().into_owned();
        unsafe { with_com(|| icon_for(&name, 0, SHGFI_SYSICONINDEX, px)) }
    }
}

unsafe fn with_com<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    let com = CoInitializeEx(null_mut(), COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) >= 0;
    let r = f();
    if com {
        CoUninitialize();
    }
    r
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Lista de imágenes del sistema para el tamaño pedido; si el sistema no tiene la de 256 px, usa la de 48.
fn list_ids(px: u32) -> &'static [i32] {
    match px {
        0..=32 => &[SHIL_LARGE],
        33..=48 => &[SHIL_EXTRALARGE, SHIL_LARGE],
        _ => &[SHIL_JUMBO, SHIL_EXTRALARGE, SHIL_LARGE],
    }
}

unsafe fn icon_for(name: &str, attrs: u32, flags: u32, px: u32) -> Result<RawIcon, String> {
    let w = wide(name);
    let mut info: ShFileInfoW = std::mem::zeroed();
    let r = SHGetFileInfoW(w.as_ptr(), attrs, &mut info, std::mem::size_of::<ShFileInfoW>() as u32, flags);
    if r == 0 {
        return Err("Windows no devolvió icono para este elemento.".into());
    }
    let mut last = "No se pudo obtener la lista de iconos del sistema.".to_string();
    for &id in list_ids(px) {
        let mut raw: *mut c_void = null_mut();
        if SHGetImageList(id, &IID_IIMAGELIST, &mut raw) < 0 || raw.is_null() {
            continue;
        }
        let hicon = ImageList_GetIcon(raw, info.icon, ILD_TRANSPARENT);
        let unk = raw as *mut Unknown;
        ((*(*unk).vtbl).release)(raw);
        if hicon == 0 {
            last = "La lista de iconos del sistema no contiene este icono.".into();
            continue;
        }
        let out = hicon_to_rgba(hicon);
        DestroyIcon(hicon);
        return out.map(without_corner_padding);
    }
    Err(last)
}

/// HICON → RGBA con alfa recto. Los iconos con canal alfa (casi todos hoy) lo traen en el bitmap de color;
/// los antiguos de 24 bits se enmascaran con el bitmap de máscara.
unsafe fn hicon_to_rgba(hicon: isize) -> Result<RawIcon, String> {
    let mut ii: IconInfo = std::mem::zeroed();
    if GetIconInfo(hicon, &mut ii) == 0 {
        return Err("No se pudo leer el icono.".into());
    }
    let out = read_bitmaps(&ii);
    if ii.color != 0 {
        DeleteObject(ii.color);
    }
    if ii.mask != 0 {
        DeleteObject(ii.mask);
    }
    out
}

unsafe fn read_bitmaps(ii: &IconInfo) -> Result<RawIcon, String> {
    if ii.color == 0 {
        return Err("Icono monocromo no admitido.".into());
    }
    let mut bm: Bitmap = std::mem::zeroed();
    if GetObjectW(ii.color, std::mem::size_of::<Bitmap>() as i32, &mut bm as *mut Bitmap as *mut c_void) == 0 {
        return Err("No se pudo medir el icono.".into());
    }
    let (w, h) = (bm.width, bm.height);
    if w <= 0 || h <= 0 || w > MAX_DIM || h > MAX_DIM {
        return Err("Tamaño de icono no válido.".into());
    }
    let dc = CreateCompatibleDC(0);
    if dc == 0 {
        return Err("No se pudo crear el contexto gráfico.".into());
    }
    let bytes = (w as usize) * (h as usize) * 4;
    let mut color = vec![0u8; bytes];
    let ok = dib(dc, ii.color, w, h, &mut color);
    let mut rgba = Vec::new();
    if ok {
        let has_alpha = color.chunks_exact(4).any(|p| p[3] != 0);
        let mut mask = vec![0u8; bytes];
        let have_mask = !has_alpha && ii.mask != 0 && dib(dc, ii.mask, w, h, &mut mask);
        rgba.reserve_exact(bytes);
        for (i, p) in color.chunks_exact(4).enumerate() {
            let a = if has_alpha {
                p[3]
            } else if have_mask {
                if mask[i * 4] == 0 { 255 } else { 0 } // máscara: negro = opaco, blanco = transparente
            } else {
                255
            };
            rgba.extend_from_slice(&[p[2], p[1], p[0], a]);
        }
    }
    DeleteDC(dc);
    if !ok {
        return Err("No se pudieron leer los píxeles del icono.".into());
    }
    Ok(RawIcon { w: w as u32, h: h as u32, rgba })
}

/// Lee un bitmap como BGRA de 32 bits, de arriba abajo.
unsafe fn dib(dc: isize, bmp: isize, w: i32, h: i32, out: &mut [u8]) -> bool {
    let mut bi = BitmapInfo {
        header: BitmapInfoHeader { size: 40, width: w, height: -h, planes: 1, bit_count: 32, compression: 0, size_image: 0, x_ppm: 0, y_ppm: 0, clr_used: 0, clr_important: 0 },
        colors: [0],
    };
    GetDIBits(dc, bmp, 0, h as u32, out.as_mut_ptr() as *mut c_void, &mut bi, 0) != 0
}
