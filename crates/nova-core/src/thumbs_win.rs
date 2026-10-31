//! Miniaturas proporcionadas por Windows (la misma API que usa el Explorador: `IShellItemImageFactory`).
//!
//! Qué cubre: cualquier formato para el que el sistema tenga un proveedor de miniaturas instalado
//! (PDF con Edge/Acrobat, PSD/AI con Photoshop/Illustrator o un códec, vídeo, Office, carátulas de audio…).
//! Qué NO hace: no genera nada por su cuenta. Si Windows no tiene proveedor, devuelve `Unsupported`.
//!
//! Se declara a mano (sin la crate `windows`) igual que el resto de llamadas nativas del proyecto.
//! Cada extracción corre en su propio hilo con tiempo límite: un manejador de terceros que se
//! cuelgue no puede bloquear la aplicación (el hilo se abandona y el archivo se marca como fallido).

use crate::thumbs::{bgra_to_rgba_image, ThumbError};
use image::RgbaImage;
use std::ffi::c_void;
use std::path::Path;
use std::ptr::null_mut;
use std::time::Duration;

#[repr(C)]
#[derive(Clone, Copy)]
struct Guid {
    d1: u32,
    d2: u16,
    d3: u16,
    d4: [u8; 8],
}

/// {bcc18b79-ba16-442f-80c4-8a59c30c463b}
const IID_ISHELLITEMIMAGEFACTORY: Guid = Guid { d1: 0xbcc18b79, d2: 0xba16, d3: 0x442f, d4: [0x80, 0xc4, 0x8a, 0x59, 0xc3, 0x0c, 0x46, 0x3b] };

#[repr(C)]
#[derive(Clone, Copy)]
struct Size {
    cx: i32,
    cy: i32,
}

#[repr(C)]
struct ImageFactoryVtbl {
    query_interface: unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    get_image: unsafe extern "system" fn(*mut c_void, Size, u32, *mut isize) -> i32,
}

#[repr(C)]
struct ImageFactory {
    vtbl: *const ImageFactoryVtbl,
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

// Si el diseño de estas estructuras no coincide con el de Windows, no compila (en lugar de fallar en ejecución).
const _: () = assert!(std::mem::size_of::<BitmapInfoHeader>() == 40);
#[cfg(target_pointer_width = "64")]
const _: () = assert!(std::mem::size_of::<Bitmap>() == 32);

#[link(name = "shell32")]
extern "system" {
    fn SHCreateItemFromParsingName(path: *const u16, bind_ctx: *mut c_void, riid: *const Guid, out: *mut *mut c_void) -> i32;
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
const SIIGBF_BIGGERSIZEOK: u32 = 0x1;
/// Solo miniaturas reales: sin esto, Windows devolvería el icono genérico del tipo de archivo.
const SIIGBF_THUMBNAILONLY: u32 = 0x8;

const E_FILE_NOT_FOUND: u32 = 0x8007_0002;
const E_PATH_NOT_FOUND: u32 = 0x8007_0003;
const E_ACCESS_DENIED: u32 = 0x8007_0005;
const WTS_E_EXTRACTION_TIMEDOUT: u32 = 0x8004_B201;

fn hr_error(hr: i32) -> ThumbError {
    match hr as u32 {
        E_FILE_NOT_FOUND | E_PATH_NOT_FOUND => ThumbError::Failed("Windows no encuentra el archivo.".into()),
        E_ACCESS_DENIED => ThumbError::Failed("Acceso denegado al archivo.".into()),
        WTS_E_EXTRACTION_TIMEDOUT => ThumbError::Failed("El proveedor de miniaturas de Windows tardó demasiado.".into()),
        // WTS_E_FAILEDEXTRACTION (0x8004B200), E_FAIL…: el sistema no tiene proveedor para este formato.
        _ => ThumbError::Unsupported,
    }
}

unsafe fn extract(path: &Path, px: u32) -> Result<RgbaImage, ThumbError> {
    let com = CoInitializeEx(null_mut(), COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) >= 0;
    let r = extract_inner(path, px);
    if com {
        CoUninitialize();
    }
    r
}

unsafe fn extract_inner(path: &Path, px: u32) -> Result<RgbaImage, ThumbError> {
    let wide: Vec<u16> = path.to_string_lossy().encode_utf16().chain(Some(0)).collect();
    let mut factory: *mut c_void = null_mut();
    let hr = SHCreateItemFromParsingName(wide.as_ptr(), null_mut(), &IID_ISHELLITEMIMAGEFACTORY, &mut factory);
    if hr < 0 || factory.is_null() {
        return Err(hr_error(hr));
    }
    let obj = factory as *mut ImageFactory;
    let mut hbmp: isize = 0;
    let side = px.min(1024) as i32;
    let hr = ((*(*obj).vtbl).get_image)(factory, Size { cx: side, cy: side }, SIIGBF_THUMBNAILONLY | SIIGBF_BIGGERSIZEOK, &mut hbmp);
    ((*(*obj).vtbl).release)(factory);
    if hr < 0 || hbmp == 0 {
        return Err(hr_error(hr));
    }

    let mut bm = Bitmap { bm_type: 0, width: 0, height: 0, width_bytes: 0, planes: 0, bits_pixel: 0, bits: null_mut() };
    if GetObjectW(hbmp, std::mem::size_of::<Bitmap>() as i32, &mut bm as *mut Bitmap as *mut c_void) == 0 {
        DeleteObject(hbmp);
        return Err(ThumbError::Failed("No se pudo leer el mapa de bits de Windows.".into()));
    }
    let (w, h) = (bm.width, bm.height.abs());
    if w <= 0 || h <= 0 || w > 4096 || h > 4096 {
        DeleteObject(hbmp);
        return Err(ThumbError::Failed("Tamaño de miniatura inesperado.".into()));
    }
    let hdc = CreateCompatibleDC(0);
    let mut bmi = BitmapInfo {
        // Altura negativa = filas de arriba abajo. 32 bits = BGRA.
        header: BitmapInfoHeader { size: 40, width: w, height: -h, planes: 1, bit_count: 32, compression: 0, size_image: 0, x_ppm: 0, y_ppm: 0, clr_used: 0, clr_important: 0 },
        colors: [0],
    };
    let mut buf = vec![0u8; (w as usize) * (h as usize) * 4];
    let lines = GetDIBits(hdc, hbmp, 0, h as u32, buf.as_mut_ptr() as *mut c_void, &mut bmi, 0);
    DeleteDC(hdc);
    DeleteObject(hbmp);
    if lines == 0 {
        return Err(ThumbError::Failed("No se pudieron leer los píxeles de la miniatura.".into()));
    }
    bgra_to_rgba_image(w as u32, h as u32, &buf).ok_or_else(|| ThumbError::Failed("Miniatura de Windows incompleta.".into()))
}

/// Pide a Windows la miniatura de `path`, con tiempo límite.
pub fn shell_thumbnail(path: &Path, px: u32, timeout: Duration) -> Result<RgbaImage, ThumbError> {
    let (tx, rx) = std::sync::mpsc::channel();
    let p = path.to_path_buf();
    std::thread::Builder::new()
        .name("nova-shell-thumb".into())
        .spawn(move || {
            let _ = tx.send(unsafe { extract(&p, px) });
        })
        .map_err(|e| ThumbError::Failed(format!("No se pudo crear el hilo de miniaturas: {e}")))?;
    match rx.recv_timeout(timeout) {
        Ok(r) => r,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(ThumbError::Failed("Windows tardó demasiado en generar la miniatura.".into())),
        Err(_) => Err(ThumbError::Failed("Falló la extracción de la miniatura.".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hresults_map_to_the_right_errors() {
        assert_eq!(hr_error(0x8004B200u32 as i32), ThumbError::Unsupported);
        assert_eq!(hr_error(0x80004005u32 as i32), ThumbError::Unsupported);
        assert!(matches!(hr_error(0x80070002u32 as i32), ThumbError::Failed(_)));
        assert!(matches!(hr_error(0x80070005u32 as i32), ThumbError::Failed(_)));
        assert!(matches!(hr_error(0x8004B201u32 as i32), ThumbError::Failed(_)));
    }

    #[test]
    fn iid_matches_the_documented_guid() {
        let g = IID_ISHELLITEMIMAGEFACTORY;
        assert_eq!(format!("{:08x}-{:04x}-{:04x}", g.d1, g.d2, g.d3), "bcc18b79-ba16-442f-80c4-8a59c30c463b");
        assert_eq!(g.d4, [0x80, 0xc4, 0x8a, 0x59, 0xc3, 0x0c, 0x46, 0x3b]);
    }
}
