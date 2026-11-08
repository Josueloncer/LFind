//! Miniaturas bajo demanda.
//!
//! Arquitectura (todo aquí es independiente de Tauri y de la interfaz):
//!  * `ThumbService`: punto único. Decide proveedor, consulta la caché, limita la concurrencia,
//!    evita trabajo duplicado y descarta solicitudes obsoletas (por «época»: cambia al cambiar de carpeta).
//!  * `ThumbProvider`: un proveedor por familia de formatos. Añadir soporte = implementar el trait.
//!      - `RasterProvider`: JPG/PNG/GIF(1.er fotograma)/BMP/TIFF/WebP/ICO con la crate `image`
//!        (Rust puro), con límites de tamaño y memoria y orientación EXIF.
//!      - `SvgProvider`: devuelve el SVG tal cual; se pinta con `<img>`, donde no se ejecutan scripts
//!        ni se cargan recursos externos.
//!      - `ShellProvider` (solo Windows, ver `thumbs_win`): miniaturas del propio Windows. Cubre PDF,
//!        PSD, AI, vídeo, Office… siempre que el sistema tenga un proveedor instalado (Edge/Acrobat,
//!        Photoshop/Illustrator, códecs…). Si no lo hay, devuelve `Unsupported` y la interfaz mantiene el icono.
//!  * `ThumbCache`: caché persistente con clave (ruta, tamaño, fecha de modificación, px, versión): si el
//!    archivo cambia, la clave cambia y nunca se sirve una miniatura obsoleta. Con límite de tamaño.
//!  * También se recuerdan los fallos (marcadores), para no repetir cada vez un manejador lento o roto.

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{DynamicImage, ExtendedColorType, ImageDecoder, ImageEncoder, ImageReader, Limits, RgbaImage};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Súbela si cambia el aspecto de las miniaturas (invalida toda la caché anterior).
pub const CACHE_VERSION: u32 = 1;
pub const MIN_PX: u32 = 32;
pub const MAX_PX: u32 = 1024;

pub const RASTER_EXTS: &[&str] = &["jpg", "jpeg", "jpe", "jfif", "png", "gif", "bmp", "tif", "tiff", "webp", "ico"];
pub const SVG_EXTS: &[&str] = &["svg"];
/// Formatos que solo puede resolver el sistema operativo (si tiene un proveedor instalado).
pub const SHELL_EXTS: &[&str] = &[
    "pdf", "psd", "psb", "ai", "eps", "heic", "heif", "avif", "cr2", "cr3", "nef", "arw", "dng", "orf", "rw2", "raf", "mp4", "m4v", "mov", "mkv", "avi", "wmv",
    "webm", "mpg", "mpeg", "flv", "3gp", "mp3", "flac", "m4a", "wma", "aac", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp", "vsdx", "xps",
];

pub fn is_thumb_ext(ext: &str) -> bool {
    RASTER_EXTS.contains(&ext) || SVG_EXTS.contains(&ext) || SHELL_EXTS.contains(&ext)
}

// ───────────────────────── tipos ─────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Thumb {
    pub mime: &'static str,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThumbError {
    /// Ningún proveedor sabe (o puede) generar miniatura de este archivo.
    Unsupported,
    TooLarge,
    /// La solicitud quedó obsoleta (se cambió de carpeta). No es un fallo del archivo.
    Cancelled,
    Failed(String),
}

impl ThumbError {
    pub fn kind(&self) -> &'static str {
        match self {
            ThumbError::Unsupported => "unsupported",
            ThumbError::TooLarge => "toolarge",
            ThumbError::Cancelled => "cancelled",
            ThumbError::Failed(_) => "failed",
        }
    }
    pub fn message(&self) -> String {
        match self {
            ThumbError::Unsupported => "Sin proveedor de miniaturas para este archivo.".into(),
            ThumbError::TooLarge => "El archivo es demasiado grande para generar miniatura.".into(),
            ThumbError::Cancelled => "Solicitud cancelada.".into(),
            ThumbError::Failed(m) => m.clone(),
        }
    }
}

pub trait ThumbProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn supports(&self, ext: &str) -> bool;
    /// `cancelled()` se consulta entre pasos costosos para abandonar pronto.
    fn render(&self, path: &Path, px: u32, cancelled: &dyn Fn() -> bool) -> Result<Thumb, ThumbError>;
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// ───────────────────────── codificación ─────────────────────────

/// Reduce (nunca amplía) a `px` y codifica: PNG si hay transparencia real, JPEG en otro caso.
pub fn encode_thumb(img: DynamicImage, px: u32) -> Result<Thumb, ThumbError> {
    let fail = |e: image::ImageError| ThumbError::Failed(format!("No se pudo codificar la miniatura: {e}"));
    let img = if img.width() > px || img.height() > px { img.thumbnail(px, px) } else { img };
    let (w, h) = (img.width(), img.height());
    let rgba = img.to_rgba8();
    let mut out = Vec::with_capacity(32 * 1024);
    if rgba.pixels().any(|p| p[3] < 255) {
        PngEncoder::new_with_quality(&mut out, CompressionType::Fast, FilterType::Adaptive)
            .write_image(rgba.as_raw(), w, h, ExtendedColorType::Rgba8)
            .map_err(fail)?;
        Ok(Thumb { mime: "image/png", bytes: out })
    } else {
        JpegEncoder::new_with_quality(&mut out, 82).encode(img.to_rgb8().as_raw(), w, h, ExtendedColorType::Rgb8).map_err(fail)?;
        Ok(Thumb { mime: "image/jpeg", bytes: out })
    }
}

/// BGRA (como lo entrega GDI) → RGBA con alfa recto.
/// * Si algún píxel es semitransparente se asume alfa premultiplicado (lo habitual en miniaturas del shell).
/// * Si TODOS los alfa son 0, el manejador no usa el canal alfa: se trata como opaco (si no, saldría invisible).
pub fn bgra_to_rgba_image(w: u32, h: u32, bgra: &[u8]) -> Option<RgbaImage> {
    if bgra.len() < (w as usize) * (h as usize) * 4 {
        return None;
    }
    let all_zero_alpha = bgra.chunks_exact(4).all(|p| p[3] == 0);
    let mut out = Vec::with_capacity(bgra.len());
    for p in bgra.chunks_exact(4) {
        let a = if all_zero_alpha { 255 } else { p[3] };
        let un = |c: u8| if a == 0 || a == 255 { c } else { ((c as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8 };
        out.extend_from_slice(&[un(p[2]), un(p[1]), un(p[0]), a]);
    }
    RgbaImage::from_raw(w, h, out)
}

// ───────────────────────── proveedores ─────────────────────────

pub struct RasterProvider {
    pub max_file_bytes: u64,
    pub max_pixels: u64,
}

impl Default for RasterProvider {
    fn default() -> Self {
        RasterProvider { max_file_bytes: 200 << 20, max_pixels: 100_000_000 }
    }
}

fn map_image_err(e: image::ImageError) -> ThumbError {
    use image::ImageError::*;
    match e {
        Limits(_) => ThumbError::TooLarge,
        Unsupported(_) => ThumbError::Unsupported,
        other => ThumbError::Failed(format!("Imagen dañada o ilegible: {other}")),
    }
}

impl ThumbProvider for RasterProvider {
    fn name(&self) -> &'static str {
        "raster"
    }
    fn supports(&self, ext: &str) -> bool {
        RASTER_EXTS.contains(&ext)
    }
    fn render(&self, path: &Path, px: u32, cancelled: &dyn Fn() -> bool) -> Result<Thumb, ThumbError> {
        let io_fail = |e: io::Error| ThumbError::Failed(crate::ops::classify(&e).1);
        if fs::metadata(path).map_err(io_fail)?.len() > self.max_file_bytes {
            return Err(ThumbError::TooLarge);
        }
        let mut reader = ImageReader::open(path).map_err(io_fail)?.with_guessed_format().map_err(io_fail)?;
        let mut limits = Limits::default();
        limits.max_image_width = Some(30_000);
        limits.max_image_height = Some(30_000);
        limits.max_alloc = Some(768 << 20);
        reader.limits(limits);
        let mut decoder = reader.into_decoder().map_err(map_image_err)?;
        let (w, h) = decoder.dimensions();
        if (w as u64) * (h as u64) > self.max_pixels {
            return Err(ThumbError::TooLarge);
        }
        let orientation = decoder.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
        if cancelled() {
            return Err(ThumbError::Cancelled);
        }
        let mut img = DynamicImage::from_decoder(decoder).map_err(map_image_err)?;
        img.apply_orientation(orientation);
        if cancelled() {
            return Err(ThumbError::Cancelled);
        }
        encode_thumb(img, px)
    }
}

/// El SVG se entrega sin tocar y lo pinta el navegador con `<img>`: en ese contexto no se ejecutan
/// scripts, no se cargan recursos externos y no puede acceder a nada de la aplicación.
pub struct SvgProvider {
    pub max_bytes: u64,
}

impl Default for SvgProvider {
    fn default() -> Self {
        SvgProvider { max_bytes: 2 << 20 }
    }
}

impl ThumbProvider for SvgProvider {
    fn name(&self) -> &'static str {
        "svg"
    }
    fn supports(&self, ext: &str) -> bool {
        SVG_EXTS.contains(&ext)
    }
    fn render(&self, path: &Path, _px: u32, _c: &dyn Fn() -> bool) -> Result<Thumb, ThumbError> {
        let io_fail = |e: io::Error| ThumbError::Failed(crate::ops::classify(&e).1);
        if fs::metadata(path).map_err(io_fail)?.len() > self.max_bytes {
            return Err(ThumbError::TooLarge);
        }
        let bytes = fs::read(path).map_err(io_fail)?;
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).to_lowercase();
        if !head.contains("<svg") {
            return Err(ThumbError::Failed("El archivo no es un SVG válido.".into()));
        }
        Ok(Thumb { mime: "image/svg+xml", bytes })
    }
}

#[cfg(any(windows, nova_winck))]
#[cfg_attr(not(windows), allow(dead_code))]
pub struct ShellProvider {
    pub timeout: Duration,
}

#[cfg(any(windows, nova_winck))]
impl ThumbProvider for ShellProvider {
    fn name(&self) -> &'static str {
        "windows-shell"
    }
    fn supports(&self, ext: &str) -> bool {
        SHELL_EXTS.contains(&ext) || RASTER_EXTS.contains(&ext) // los raster solo llegan aquí si el proveedor propio falló
    }
    fn render(&self, path: &Path, px: u32, cancelled: &dyn Fn() -> bool) -> Result<Thumb, ThumbError> {
        if cancelled() {
            return Err(ThumbError::Cancelled);
        }
        let img = crate::thumbs_win::shell_thumbnail(path, px, self.timeout)?;
        encode_thumb(DynamicImage::ImageRgba8(img), px)
    }
}

/// Proveedores por defecto, por orden de prioridad.
pub fn default_providers() -> Vec<Box<dyn ThumbProvider>> {
    #[allow(unused_mut)]
    let mut v: Vec<Box<dyn ThumbProvider>> = vec![Box::new(RasterProvider::default()), Box::new(SvgProvider::default())];
    #[cfg(windows)]
    v.push(Box::new(ShellProvider { timeout: Duration::from_secs(8) }));
    v
}

// ───────────────────────── caché en disco ─────────────────────────

pub enum CacheHit {
    Hit(Thumb),
    /// Se sabe que no se puede (o falló hace poco): no se vuelve a intentar todavía.
    Negative(ThumbError),
    Miss,
}

pub const UNSUPPORTED_TTL: Duration = Duration::from_secs(7 * 24 * 3600);
pub const FAILED_TTL: Duration = Duration::from_secs(5 * 60);

pub struct ThumbCache {
    dir: PathBuf,
    max_bytes: u64,
    puts: AtomicUsize,
}

fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn mime_ext(m: &str) -> &'static str {
    match m {
        "image/png" => "png",
        "image/svg+xml" => "svg",
        _ => "jpg",
    }
}

fn ext_mime(e: &str) -> &'static str {
    match e {
        "png" => "image/png",
        "svg" => "image/svg+xml",
        _ => "image/jpeg",
    }
}

impl ThumbCache {
    pub fn new(dir: PathBuf, max_bytes: u64) -> Self {
        ThumbCache { dir, max_bytes, puts: AtomicUsize::new(0) }
    }

    /// Clave estable (no depende de la versión de Rust). Cambia si cambia la ruta, el tamaño,
    /// la fecha de modificación, el tamaño pedido o `CACHE_VERSION`.
    pub fn key(path: &Path, size: u64, mtime_ms: i64, px: u32) -> String {
        let p = path.to_string_lossy().to_lowercase().replace('/', "\\");
        format!("{:016x}-{}-{}-{}-v{}", fnv1a(&p), size, mtime_ms, px, CACHE_VERSION)
    }

    fn path_of(&self, key: &str, ext: &str) -> PathBuf {
        self.dir.join(&key[..2]).join(format!("{key}.{ext}"))
    }

    pub fn get(&self, key: &str) -> CacheHit {
        for ext in ["jpg", "png", "svg"] {
            if let Ok(bytes) = fs::read(self.path_of(key, ext)) {
                if !bytes.is_empty() {
                    return CacheHit::Hit(Thumb { mime: ext_mime(ext), bytes });
                }
            }
        }
        for (ext, ttl, err) in [("none", UNSUPPORTED_TTL, ThumbError::Unsupported), ("fail", FAILED_TTL, ThumbError::Failed("Falló hace poco; se reintentará más tarde.".into()))] {
            let p = self.path_of(key, ext);
            if let Ok(m) = fs::metadata(&p) {
                let fresh = m.modified().ok().and_then(|t| t.elapsed().ok()).map_or(false, |age| age < ttl);
                if fresh {
                    return CacheHit::Negative(err);
                }
                let _ = fs::remove_file(&p);
            }
        }
        CacheHit::Miss
    }

    fn write(&self, key: &str, ext: &str, bytes: &[u8]) {
        let p = self.path_of(key, ext);
        if let Some(d) = p.parent() {
            let _ = fs::create_dir_all(d);
        }
        // Escritura atómica: nunca se lee un archivo a medias.
        let tmp = p.with_extension(format!("{ext}.tmp{}", std::process::id()));
        if fs::write(&tmp, bytes).is_ok() && fs::rename(&tmp, &p).is_err() {
            let _ = fs::remove_file(&tmp);
        }
        if self.puts.fetch_add(1, Ordering::Relaxed) % 200 == 199 {
            self.prune();
        }
    }

    /// Archivo guardado para `key` y su tipo (para entregarlo sin volver a leerlo en memoria).
    pub fn stored_file(&self, key: &str) -> Option<(PathBuf, &'static str)> {
        ["jpg", "png", "svg"].into_iter().map(|e| (self.path_of(key, e), ext_mime(e))).find(|(p, _)| p.is_file())
    }

    pub fn put(&self, key: &str, t: &Thumb) {
        self.write(key, mime_ext(t.mime), &t.bytes);
    }

    pub fn put_negative(&self, key: &str, e: &ThumbError) {
        match e {
            ThumbError::Unsupported | ThumbError::TooLarge => self.write(key, "none", b""),
            ThumbError::Failed(_) => self.write(key, "fail", b""),
            ThumbError::Cancelled => {}
        }
    }

    fn files(&self) -> Vec<(SystemTime, u64, PathBuf)> {
        let mut v = vec![];
        if let Ok(top) = fs::read_dir(&self.dir) {
            for sub in top.flatten() {
                if let Ok(rd) = fs::read_dir(sub.path()) {
                    for f in rd.flatten() {
                        if let Ok(m) = f.metadata() {
                            if m.is_file() {
                                v.push((m.modified().unwrap_or(UNIX_EPOCH), m.len(), f.path()));
                            }
                        }
                    }
                }
            }
        }
        v
    }

    /// Si la caché supera su límite, elimina lo más antiguo hasta quedar al 80 %.
    pub fn prune(&self) {
        let mut files = self.files();
        let total: u64 = files.iter().map(|f| f.1).sum();
        if total <= self.max_bytes {
            return;
        }
        files.sort_by_key(|f| f.0);
        let target = self.max_bytes / 10 * 8;
        let mut left = total;
        for (_, len, p) in files {
            if left <= target {
                break;
            }
            if fs::remove_file(&p).is_ok() {
                left = left.saturating_sub(len);
            }
        }
    }

    /// Vacía la caché. Devuelve los bytes liberados.
    pub fn clear(&self) -> u64 {
        let freed: u64 = self.files().iter().map(|f| f.1).sum();
        let _ = fs::remove_dir_all(&self.dir);
        freed
    }
}

// ───────────────────────── servicio ─────────────────────────

struct Limiter {
    used: Mutex<usize>,
    cv: Condvar,
    max: usize,
}

struct Permit<'a>(&'a Limiter);

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        *lock(&self.0.used) -= 1;
        self.0.cv.notify_one();
    }
}

impl Limiter {
    fn acquire(&self, cancelled: &dyn Fn() -> bool) -> Result<Permit<'_>, ThumbError> {
        let mut g = lock(&self.used);
        loop {
            if *g < self.max {
                *g += 1;
                return Ok(Permit(self));
            }
            if cancelled() {
                return Err(ThumbError::Cancelled);
            }
            g = self.cv.wait_timeout(g, Duration::from_millis(50)).unwrap_or_else(|e| e.into_inner()).0;
        }
    }
}

type Outcome = Result<Thumb, ThumbError>;

struct Flight {
    done: Mutex<Option<Outcome>>,
    cv: Condvar,
}

pub struct ThumbConfig {
    pub cache_dir: PathBuf,
    pub max_cache_bytes: u64,
    /// Máximo de miniaturas generándose a la vez.
    pub parallel: usize,
}

pub struct ThumbService {
    providers: Vec<Box<dyn ThumbProvider>>,
    cache: ThumbCache,
    epoch: AtomicU64,
    limiter: Limiter,
    inflight: Mutex<HashMap<String, Arc<Flight>>>,
}

fn mtime_ms(m: &fs::Metadata) -> i64 {
    m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis() as i64)
}

impl ThumbService {
    pub fn new(cfg: ThumbConfig, providers: Vec<Box<dyn ThumbProvider>>) -> Self {
        ThumbService {
            providers,
            cache: ThumbCache::new(cfg.cache_dir, cfg.max_cache_bytes),
            epoch: AtomicU64::new(0),
            limiter: Limiter { used: Mutex::new(0), cv: Condvar::new(), max: cfg.parallel.max(1) },
            inflight: Mutex::new(HashMap::new()),
        }
    }

    pub fn with_defaults(cache_dir: PathBuf) -> Self {
        Self::new(ThumbConfig { cache_dir, max_cache_bytes: 300 << 20, parallel: 3 }, default_providers())
    }

    /// Las solicitudes con una época anterior se descartan (p. ej. al cambiar de carpeta).
    pub fn set_epoch(&self, e: u64) {
        self.epoch.fetch_max(e, Ordering::SeqCst);
    }

    pub fn clear_cache(&self) -> u64 {
        self.cache.clear()
    }

    /// `epoch = None`: la solicitud no caduca por cambios de carpeta (p. ej. el panel de vista previa).
    pub fn get(&self, path: &Path, px: u32, epoch: Option<u64>) -> Result<Thumb, ThumbError> {
        if let Some(e) = epoch {
            self.set_epoch(e);
        }
        let cancelled = || epoch.map_or(false, |e| e < self.epoch.load(Ordering::SeqCst));
        if cancelled() {
            return Err(ThumbError::Cancelled);
        }
        let px = px.clamp(MIN_PX, MAX_PX);
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let chain: Vec<&dyn ThumbProvider> = self.providers.iter().map(|b| b.as_ref()).filter(|p| p.supports(&ext)).collect();
        if chain.is_empty() {
            return Err(ThumbError::Unsupported);
        }
        let meta = fs::metadata(path).map_err(|e| ThumbError::Failed(crate::ops::classify(&e).1))?;
        if !meta.is_file() {
            return Err(ThumbError::Unsupported);
        }
        let key = ThumbCache::key(path, meta.len(), mtime_ms(&meta), px);
        match self.cache.get(&key) {
            CacheHit::Hit(t) => return Ok(t),
            CacheHit::Negative(e) => return Err(e),
            CacheHit::Miss => {}
        }

        for _attempt in 0..2 {
            let (flight, leader) = {
                let mut m = lock(&self.inflight);
                match m.get(&key) {
                    Some(f) => (f.clone(), false),
                    None => {
                        let f = Arc::new(Flight { done: Mutex::new(None), cv: Condvar::new() });
                        m.insert(key.clone(), f.clone());
                        (f, true)
                    }
                }
            };
            let result = if leader {
                let r = self.compute(path, px, &key, &chain, &cancelled);
                *lock(&flight.done) = Some(r.clone());
                lock(&self.inflight).remove(&key);
                flight.cv.notify_all();
                r
            } else {
                // Otro hilo ya está generando exactamente esta miniatura: se espera su resultado.
                let mut g = lock(&flight.done);
                loop {
                    if let Some(r) = g.clone() {
                        break r;
                    }
                    if cancelled() {
                        return Err(ThumbError::Cancelled);
                    }
                    g = flight.cv.wait_timeout(g, Duration::from_millis(50)).unwrap_or_else(|e| e.into_inner()).0;
                }
            };
            // Si el líder se canceló pero esta solicitud sigue vigente, se reintenta por su cuenta.
            if result == Err(ThumbError::Cancelled) && !cancelled() {
                continue;
            }
            return result;
        }
        Err(ThumbError::Cancelled)
    }

    fn compute(&self, path: &Path, px: u32, key: &str, chain: &[&dyn ThumbProvider], cancelled: &dyn Fn() -> bool) -> Outcome {
        let _permit = self.limiter.acquire(cancelled)?;
        let mut last = ThumbError::Unsupported;
        for p in chain {
            if cancelled() {
                return Err(ThumbError::Cancelled);
            }
            // Frontera de pánicos: aquí se interpreta el contenido de archivos ajenos. Un decodificador que entre en pánico
            // cuenta como fallo de ESE archivo (se prueba el siguiente proveedor y se recuerda el fallo); el detalle va al registro.
            let rendered = crate::diagnostics::isolate(|| p.render(path, px, cancelled)).unwrap_or_else(|_| Err(ThumbError::Failed("No se pudo procesar el archivo (¿está dañado?).".into())));
            match rendered {
                Ok(t) => {
                    self.cache.put(key, &t);
                    return Ok(t);
                }
                Err(ThumbError::Cancelled) => return Err(ThumbError::Cancelled),
                Err(e) => last = e, // se prueba el siguiente proveedor que soporte la extensión
            }
        }
        self.cache.put_negative(key, &last);
        Err(last)
    }
}

// ───────────────────────── pruebas ─────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb, Rgba};
    use std::sync::atomic::AtomicBool;

    static N: AtomicUsize = AtomicUsize::new(0);
    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nova-th-{}-{}-{tag}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }
    fn svc(dir: &Path, parallel: usize, providers: Vec<Box<dyn ThumbProvider>>) -> ThumbService {
        ThumbService::new(ThumbConfig { cache_dir: dir.join("cache"), max_cache_bytes: 50 << 20, parallel }, providers)
    }
    fn no() -> bool {
        false
    }
    fn decode(t: &Thumb) -> DynamicImage {
        image::load_from_memory(&t.bytes).unwrap()
    }

    // ───── caché ─────
    #[test]
    fn cache_key_changes_when_file_or_request_changes() {
        let p = Path::new("C:\\a\\f.jpg");
        let k = ThumbCache::key(p, 10, 1000, 128);
        assert_eq!(k, ThumbCache::key(Path::new("c:\\A\\F.JPG"), 10, 1000, 128), "no distingue mayúsculas (Windows)");
        assert_ne!(k, ThumbCache::key(p, 11, 1000, 128), "tamaño");
        assert_ne!(k, ThumbCache::key(p, 10, 1001, 128), "fecha de modificación");
        assert_ne!(k, ThumbCache::key(p, 10, 1000, 256), "tamaño de miniatura");
        assert_ne!(k, ThumbCache::key(Path::new("C:\\a\\g.jpg"), 10, 1000, 128), "ruta");
    }

    #[test]
    fn cache_roundtrip_negative_markers_and_ttl() {
        let d = tmp("cache");
        let c = ThumbCache::new(d.join("c"), 1 << 20);
        let k = ThumbCache::key(Path::new("/x/a.png"), 1, 1, 64);
        assert!(matches!(c.get(&k), CacheHit::Miss));
        c.put(&k, &Thumb { mime: "image/png", bytes: vec![1, 2, 3] });
        match c.get(&k) {
            CacheHit::Hit(t) => assert_eq!((t.mime, t.bytes), ("image/png", vec![1, 2, 3])),
            _ => panic!("debería estar en caché"),
        }
        let k2 = ThumbCache::key(Path::new("/x/b.psd"), 1, 1, 64);
        c.put_negative(&k2, &ThumbError::Unsupported);
        assert!(matches!(c.get(&k2), CacheHit::Negative(ThumbError::Unsupported)));
        // Un marcador caducado se ignora y se borra.
        let marker = c.path_of(&k2, "none");
        let old = SystemTime::now() - UNSUPPORTED_TTL - Duration::from_secs(60);
        fs::File::options().write(true).open(&marker).unwrap().set_modified(old).unwrap();
        assert!(matches!(c.get(&k2), CacheHit::Miss));
        assert!(!marker.exists());
        // Los cancelados nunca se recuerdan.
        let k3 = ThumbCache::key(Path::new("/x/c.png"), 1, 1, 64);
        c.put_negative(&k3, &ThumbError::Cancelled);
        assert!(matches!(c.get(&k3), CacheHit::Miss));
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn cache_prune_and_clear() {
        let d = tmp("prune");
        let c = ThumbCache::new(d.join("c"), 10_000);
        for i in 0..30u32 {
            c.put(&ThumbCache::key(Path::new(&format!("/f{i}.png")), i as u64, 1, 64), &Thumb { mime: "image/jpeg", bytes: vec![0u8; 1000] });
        }
        c.prune();
        let left: u64 = c.files().iter().map(|f| f.1).sum();
        assert!(left <= 10_000, "{left}");
        assert!(left > 0);
        assert!(c.clear() > 0);
        assert!(c.files().is_empty());
        let _ = fs::remove_dir_all(d);
    }

    // ───── proveedor de imágenes ─────
    fn write_png(path: &Path, w: u32, h: u32, alpha: bool) {
        if alpha {
            ImageBuffer::from_fn(w, h, |x, _| Rgba([200u8, 30, 30, if x < w / 2 { 0 } else { 255 }])).save(path).unwrap();
        } else {
            ImageBuffer::from_fn(w, h, |x, y| Rgb([(x % 256) as u8, (y % 256) as u8, 90u8])).save(path).unwrap();
        }
    }

    #[test]
    fn raster_formats_produce_thumbnails_preserving_aspect_ratio() {
        let d = tmp("raster");
        let p = RasterProvider::default();
        let img = ImageBuffer::from_fn(400, 200, |x, y| Rgb([(x % 256) as u8, (y % 256) as u8, 40u8]));
        let dynimg = DynamicImage::ImageRgb8(img);
        for name in ["a.png", "a.jpg", "a.gif", "a.bmp", "a.webp", "a.tiff"] {
            let f = d.join(name);
            dynimg.save(&f).unwrap_or_else(|e| panic!("{name}: {e}"));
            let t = p.render(&f, 100, &no).unwrap_or_else(|e| panic!("{name}: {e:?}"));
            let out = decode(&t);
            assert_eq!((out.width(), out.height()), (100, 50), "{name}");
            assert!(t.mime == "image/jpeg" || t.mime == "image/png");
        }
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn opaque_becomes_jpeg_and_transparent_stays_png_and_small_images_are_not_upscaled() {
        let d = tmp("alpha");
        let p = RasterProvider::default();
        write_png(&d.join("o.png"), 300, 300, false);
        write_png(&d.join("t.png"), 300, 300, true);
        write_png(&d.join("s.png"), 20, 10, false);
        assert_eq!(p.render(&d.join("o.png"), 64, &no).unwrap().mime, "image/jpeg");
        let t = p.render(&d.join("t.png"), 64, &no).unwrap();
        assert_eq!(t.mime, "image/png");
        assert!(decode(&t).to_rgba8().pixels().any(|px| px[3] == 0), "se conserva la transparencia");
        let s = decode(&p.render(&d.join("s.png"), 256, &no).unwrap());
        assert_eq!((s.width(), s.height()), (20, 10));
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn jpeg_exif_orientation_is_applied() {
        // JPEG 200x100 con EXIF Orientation = 6 (girar 90° a la derecha): la miniatura debe ser vertical.
        let d = tmp("exif");
        let base = d.join("b.jpg");
        DynamicImage::ImageRgb8(ImageBuffer::from_fn(200, 100, |x, _| Rgb([(x % 256) as u8, 100u8, 50u8]))).save(&base).unwrap();
        let src = fs::read(&base).unwrap();
        let mut exif = b"Exif\0\0II*\0\x08\0\0\0".to_vec();
        exif.extend_from_slice(&[1, 0, 0x12, 0x01, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0]);
        let len = (exif.len() + 2) as u16;
        let mut out = vec![0xFF, 0xD8, 0xFF, 0xE1];
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(&exif);
        out.extend_from_slice(&src[2..]);
        let f = d.join("rot.jpg");
        fs::write(&f, out).unwrap();
        let t = decode(&RasterProvider::default().render(&f, 400, &no).unwrap());
        assert_eq!((t.width(), t.height()), (100, 200));
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn damaged_empty_and_oversized_files_fail_in_a_controlled_way() {
        let d = tmp("bad");
        let p = RasterProvider::default();
        fs::write(d.join("x.jpg"), b"esto no es una imagen").unwrap();
        fs::write(d.join("e.png"), b"").unwrap();
        write_png(&d.join("ok.png"), 64, 64, false);
        let full = fs::read(d.join("ok.png")).unwrap();
        fs::write(d.join("cut.png"), &full[..full.len() / 2]).unwrap();
        for n in ["x.jpg", "e.png", "cut.png", "no-existe.png"] {
            assert!(matches!(p.render(&d.join(n), 64, &no), Err(ThumbError::Failed(_))), "{n}");
        }
        let small = RasterProvider { max_file_bytes: 10, max_pixels: 100_000_000 };
        assert_eq!(small.render(&d.join("ok.png"), 64, &no), Err(ThumbError::TooLarge));
        let few = RasterProvider { max_file_bytes: 1 << 30, max_pixels: 100 };
        assert_eq!(few.render(&d.join("ok.png"), 64, &no), Err(ThumbError::TooLarge));
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn cancellation_is_honoured_before_decoding() {
        let d = tmp("cancelimg");
        write_png(&d.join("a.png"), 64, 64, false);
        assert_eq!(RasterProvider::default().render(&d.join("a.png"), 64, &|| true), Err(ThumbError::Cancelled));
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn svg_is_passed_through_safely_and_validated() {
        let d = tmp("svg");
        let p = SvgProvider { max_bytes: 1000 };
        fs::write(d.join("a.svg"), r#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"/>"#).unwrap();
        let t = p.render(&d.join("a.svg"), 64, &no).unwrap();
        assert_eq!(t.mime, "image/svg+xml");
        fs::write(d.join("b.svg"), "hola mundo").unwrap();
        assert!(matches!(p.render(&d.join("b.svg"), 64, &no), Err(ThumbError::Failed(_))));
        fs::write(d.join("c.svg"), format!("<svg>{}</svg>", "x".repeat(2000))).unwrap();
        assert_eq!(p.render(&d.join("c.svg"), 64, &no), Err(ThumbError::TooLarge));
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn bgra_conversion_unpremultiplies_and_fixes_unused_alpha() {
        // Premultiplicado: rojo al 50 % → B=0,G=0,R=64,A=128 → R=128 tras deshacer (127,5 redondeado).
        let img = bgra_to_rgba_image(2, 1, &[0, 0, 64, 128, 10, 20, 30, 255]).unwrap();
        assert_eq!(img.get_pixel(0, 0).0, [128, 0, 0, 128]);
        assert_eq!(img.get_pixel(1, 0).0, [30, 20, 10, 255], "BGR → RGB");
        // Alfa todo a 0 = manejador que no usa alfa: debe verse, no quedar invisible.
        let o = bgra_to_rgba_image(1, 2, &[1, 2, 3, 0, 4, 5, 6, 0]).unwrap();
        assert!(o.pixels().all(|p| p[3] == 255));
        assert!(bgra_to_rgba_image(10, 10, &[0; 8]).is_none(), "búfer demasiado corto");
    }

    // ───── servicio ─────
    struct Fake {
        calls: Arc<AtomicUsize>,
        live: Arc<AtomicUsize>,
        peak: Arc<AtomicUsize>,
        delay: Duration,
        result: Mutex<Outcome>,
        exts: &'static [&'static str],
    }
    impl Fake {
        fn new(result: Outcome, delay_ms: u64) -> Self {
            Fake { calls: Arc::new(AtomicUsize::new(0)), live: Arc::new(AtomicUsize::new(0)), peak: Arc::new(AtomicUsize::new(0)), delay: Duration::from_millis(delay_ms), result: Mutex::new(result), exts: &["png", "psd"] }
        }
    }
    impl ThumbProvider for Fake {
        fn name(&self) -> &'static str {
            "fake"
        }
        fn supports(&self, e: &str) -> bool {
            self.exts.contains(&e)
        }
        fn render(&self, _p: &Path, _px: u32, _c: &dyn Fn() -> bool) -> Outcome {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let now = self.live.fetch_add(1, Ordering::SeqCst) + 1;
            self.peak.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(self.delay);
            self.live.fetch_sub(1, Ordering::SeqCst);
            lock(&self.result).clone()
        }
    }
    fn ok_thumb() -> Outcome {
        Ok(Thumb { mime: "image/jpeg", bytes: vec![9; 10] })
    }

    #[test]
    fn identical_concurrent_requests_are_computed_once() {
        let d = tmp("dedupe");
        fs::write(d.join("a.png"), b"x").unwrap();
        let f = Fake::new(ok_thumb(), 150);
        let calls = f.calls.clone();
        let s = Arc::new(svc(&d, 3, vec![Box::new(f)]));
        let hs: Vec<_> = (0..6).map(|_| { let s = s.clone(); let p = d.join("a.png"); std::thread::spawn(move || s.get(&p, 128, None)) }).collect();
        for h in hs {
            assert_eq!(h.join().unwrap().unwrap().bytes, vec![9; 10]);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let _ = fs::remove_dir_all(d);
    }

    /// Decodificador que entra en pánico, como lo haría uno real con un archivo malformado.
    struct Explodes(Arc<AtomicUsize>);
    impl ThumbProvider for Explodes {
        fn name(&self) -> &'static str {
            "explodes"
        }
        fn supports(&self, _: &str) -> bool {
            true
        }
        fn render(&self, _: &Path, _: u32, _: &dyn Fn() -> bool) -> Result<Thumb, ThumbError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            panic!("decodificador roto");
        }
    }

    #[test]
    fn a_panicking_decoder_fails_only_that_file() {
        let d = tmp("panic");
        fs::write(d.join("a.png"), b"x").unwrap();
        fs::write(d.join("b.png"), b"yy").unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let s = svc(&d, 1, vec![Box::new(Explodes(calls.clone()))]);
        assert!(matches!(s.get(&d.join("a.png"), 64, None), Err(ThumbError::Failed(_))), "es un fallo visible, no un resultado vacío");
        assert!(matches!(s.get(&d.join("a.png"), 64, None), Err(ThumbError::Failed(_))));
        assert_eq!(calls.load(Ordering::SeqCst), 1, "el fallo se recuerda: no se vuelve a provocar el pánico");
        // El permiso de concurrencia (solo hay uno) se liberó: otro archivo no se queda esperando.
        assert!(matches!(s.get(&d.join("b.png"), 64, None), Err(ThumbError::Failed(_))));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn after_a_panicking_decoder_the_next_provider_is_tried() {
        let d = tmp("panic-next");
        fs::write(d.join("a.png"), b"x").unwrap();
        let s = svc(&d, 1, vec![Box::new(Explodes(Arc::new(AtomicUsize::new(0)))), Box::new(Fake::new(ok_thumb(), 0))]);
        assert_eq!(s.get(&d.join("a.png"), 64, None).unwrap().bytes, vec![9; 10]);
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn simultaneous_generation_never_exceeds_the_limit() {
        let d = tmp("limit");
        for i in 0..10 {
            fs::write(d.join(format!("{i}.png")), vec![i as u8; i + 1]).unwrap();
        }
        let f = Fake::new(ok_thumb(), 60);
        let peak = f.peak.clone();
        let s = Arc::new(svc(&d, 2, vec![Box::new(f)]));
        let hs: Vec<_> = (0..10).map(|i| { let s = s.clone(); let p = d.join(format!("{i}.png")); std::thread::spawn(move || s.get(&p, 64, None).unwrap()) }).collect();
        hs.into_iter().for_each(|h| drop(h.join().unwrap()));
        assert!(peak.load(Ordering::SeqCst) <= 2, "pico = {}", peak.load(Ordering::SeqCst));
        assert!(peak.load(Ordering::SeqCst) >= 2, "debería aprovechar el paralelismo permitido");
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn stale_epoch_requests_are_dropped_without_work() {
        let d = tmp("epoch");
        fs::write(d.join("a.png"), b"x").unwrap();
        let f = Fake::new(ok_thumb(), 0);
        let calls = f.calls.clone();
        let s = svc(&d, 3, vec![Box::new(f)]);
        s.set_epoch(5); // se cambió de carpeta
        assert_eq!(s.get(&d.join("a.png"), 64, Some(4)), Err(ThumbError::Cancelled));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(s.get(&d.join("a.png"), 64, Some(5)).is_ok(), "la época vigente sí se atiende");
        assert!(s.get(&d.join("a.png"), 64, None).is_ok(), "sin época nunca caduca");
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn queued_requests_are_cancelled_while_waiting_for_a_slot() {
        let d = tmp("queued");
        fs::write(d.join("a.png"), b"1").unwrap();
        fs::write(d.join("b.png"), b"22").unwrap();
        let f = Fake::new(ok_thumb(), 400);
        let calls = f.calls.clone();
        let s = Arc::new(svc(&d, 1, vec![Box::new(f)]));
        let (s1, p1) = (s.clone(), d.join("a.png"));
        let first = std::thread::spawn(move || s1.get(&p1, 64, Some(1)));
        std::thread::sleep(Duration::from_millis(100)); // a.png ocupa el único hueco
        let (s2, p2) = (s.clone(), d.join("b.png"));
        let second = std::thread::spawn(move || s2.get(&p2, 64, Some(1)));
        std::thread::sleep(Duration::from_millis(100));
        s.set_epoch(2); // el usuario cambia de carpeta: b.png ya no interesa
        assert_eq!(second.join().unwrap(), Err(ThumbError::Cancelled));
        assert!(first.join().unwrap().is_ok());
        assert_eq!(calls.load(Ordering::SeqCst), 1, "b.png nunca llegó a procesarse");
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn cache_serves_repeats_and_detects_changed_files() {
        let d = tmp("change");
        let a = d.join("a.png");
        fs::write(&a, b"v1").unwrap();
        let f = Fake::new(ok_thumb(), 0);
        let calls = f.calls.clone();
        let s = svc(&d, 3, vec![Box::new(f)]);
        s.get(&a, 64, None).unwrap();
        s.get(&a, 64, None).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1, "la segunda sale de la caché");
        s.get(&a, 256, None).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2, "otro tamaño = otra miniatura");
        fs::write(&a, b"version dos mas larga").unwrap(); // cambia tamaño y fecha
        s.get(&a, 64, None).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 3, "archivo modificado: no se sirve la miniatura vieja");
        // La caché sobrevive a un reinicio (otro servicio, misma carpeta).
        let f2 = Fake::new(ok_thumb(), 0);
        let c2 = f2.calls.clone();
        svc(&d, 3, vec![Box::new(f2)]).get(&a, 64, None).unwrap();
        assert_eq!(c2.load(Ordering::SeqCst), 0);
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn unsupported_and_failed_results_are_remembered_and_do_not_block() {
        let d = tmp("neg");
        fs::write(d.join("a.psd"), b"x").unwrap();
        fs::write(d.join("b.xyz"), b"x").unwrap();
        let f = Fake::new(Err(ThumbError::Unsupported), 0);
        let calls = f.calls.clone();
        let s = svc(&d, 3, vec![Box::new(f)]);
        assert_eq!(s.get(&d.join("a.psd"), 64, None), Err(ThumbError::Unsupported));
        assert_eq!(s.get(&d.join("a.psd"), 64, None), Err(ThumbError::Unsupported));
        assert_eq!(calls.load(Ordering::SeqCst), 1, "no se reintenta un formato sin proveedor");
        assert_eq!(s.get(&d.join("b.xyz"), 64, None), Err(ThumbError::Unsupported), "extensión que nadie soporta");
        assert_eq!(s.get(&d.join("falta.png"), 64, None).unwrap_err().kind(), "failed");
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn falls_back_to_the_next_provider_when_the_first_fails() {
        let d = tmp("chain");
        fs::write(d.join("a.png"), b"x").unwrap();
        let first = Fake::new(Err(ThumbError::Failed("códec no disponible".into())), 0);
        let second = Fake::new(ok_thumb(), 0);
        let (c1, c2) = (first.calls.clone(), second.calls.clone());
        let s = svc(&d, 3, vec![Box::new(first), Box::new(second)]);
        assert!(s.get(&d.join("a.png"), 64, None).is_ok());
        assert_eq!((c1.load(Ordering::SeqCst), c2.load(Ordering::SeqCst)), (1, 1));
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn real_service_end_to_end_with_real_images() {
        let d = tmp("e2e");
        write_png(&d.join("foto.png"), 800, 600, false);
        fs::write(d.join("roto.png"), b"basura").unwrap();
        fs::write(d.join("notas.txt"), b"hola").unwrap();
        let s = ThumbService::with_defaults(d.join("cache"));
        let t = s.get(&d.join("foto.png"), 128, Some(1)).unwrap();
        let img = decode(&t);
        assert_eq!((img.width(), img.height()), (128, 96));
        assert!(s.get(&d.join("roto.png"), 128, Some(1)).is_err());
        assert_eq!(s.get(&d.join("notas.txt"), 128, Some(1)), Err(ThumbError::Unsupported));
        let hit = s.get(&d.join("foto.png"), 128, Some(1)).unwrap();
        assert_eq!(hit.bytes, t.bytes, "la segunda lectura sale de disco, idéntica");
        assert!(s.clear_cache() > 0);
        let _ = AtomicBool::new(false);
        let _ = fs::remove_dir_all(d);
    }

    // ───── coherencia con la interfaz ─────
    #[test]
    fn extension_lists_match_the_frontend() {
        let ts = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src/lib/thumbKinds.ts");
        let Ok(src) = fs::read_to_string(&ts) else { return }; // el crate puede probarse sin el frontend
        let grab = |name: &str| -> Vec<String> {
            let start = src.find(&format!("export const {name}")).unwrap_or_else(|| panic!("falta {name} en thumbKinds.ts"));
            let body = &src[start..];
            let end = body.find("];").unwrap();
            body[..end].split('"').skip(1).step_by(2).map(String::from).collect()
        };
        let mut rust: Vec<String> = RASTER_EXTS.iter().chain(SVG_EXTS).chain(SHELL_EXTS).map(|s| s.to_string()).collect();
        let mut web: Vec<String> = grab("RASTER").into_iter().chain(grab("SVG")).chain(grab("SHELL")).collect();
        rust.sort();
        web.sort();
        assert_eq!(rust, web, "src/lib/thumbKinds.ts y nova-core::thumbs deben listar las mismas extensiones");
    }
}
