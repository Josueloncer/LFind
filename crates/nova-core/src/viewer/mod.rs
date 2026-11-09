//! Visor inteligente: decide cómo mostrar cada archivo y prepara su contenido.
//!
//! * El formato se determina inspeccionando el archivo, no solo por su extensión.
//! * Lo que WebView2 decodifica por sí mismo (JPG, PNG, WebP, GIF, BMP, ICO, AVIF, SVG, vídeo, audio) se entrega tal cual.
//! * Lo demás se convierte en una imagen acotada con decodificadores en Rust seguro, dentro de una frontera de pánicos, de uno en uno y
//!   cancelable. El resultado se guarda en una caché en disco cuya clave cambia si cambia el archivo.
//! * El contenido llega a la interfaz por `serve` mediante permisos por archivo; nunca se da acceso general al disco.
//!
//! Los PDF y los AI con contenido PDF se interpretan con PDF.js en un Worker de la interfaz, aislado del proceso principal.
//! PostScript no se ejecuta jamás: de EPS solo se usa la previsualización incluida.

pub mod embedded;
pub mod psd;
pub mod serve;

use crate::diagnostics::isolate;
use crate::thumbs::{encode_thumb, is_thumb_ext, CacheHit, RasterProvider, Thumb, ThumbCache, ThumbError, ThumbProvider, ThumbService};
use image::DynamicImage;
use serde::Serialize;
use serve::{mtime_ms, Tickets};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf, Prefix};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Lado mayor de las imágenes que genera el backend.
pub const MAX_RENDER_SIDE: u32 = 3072;
pub const MAX_CACHE_BYTES: u64 = 768 << 20;
/// Por encima de esto, una imagen se reduce en lugar de entregarse tal cual: decodificarla entera en la webview gastaría demasiada memoria.
pub const MAX_NATIVE_PIXELS: u64 = 48_000_000;
pub const MAX_NATIVE_BYTES: u64 = 128 << 20;
pub const MAX_SVG_BYTES: u64 = 32 << 20;
const SNIFF_BYTES: usize = 8 * 1024;
/// Lado de la vista que se pide al proveedor de miniaturas de Windows.
const SHELL_SIDE: u32 = 1024;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Fact {
    pub label: String,
    pub value: String,
}

fn fact(label: &str, value: impl Into<String>) -> Fact {
    Fact { label: label.into(), value: value.into() }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Source {
    Image { token: String, mime: String, width: Option<u32>, height: Option<u32>, reduced: bool, note: Option<String>, facts: Vec<Fact> },
    Pdf { token: String, size: u64, illustrator: bool },
    Video { token: String, mime: String },
    Audio { token: String, mime: String },
    Text,
    /// No hay representación fiable: `message` lo explica.
    Unsupported { message: String, facts: Vec<Fact> },
    /// La solicitud quedó obsoleta (se seleccionó otro archivo).
    Cancelled,
}

fn unsupported(m: impl Into<String>) -> Source {
    Source::Unsupported { message: m.into(), facts: vec![] }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Route {
    Native(&'static str),
    Svg,
    Raster,
    Psd,
    Eps,
    RawCamera,
    Pdf { illustrator: bool },
    Video(&'static str),
    Audio(&'static str),
    Text,
    Shell,
    NotImage,
    None,
}

fn sniff_image(h: &[u8]) -> Option<Route> {
    let is = |m: &[u8]| h.starts_with(m);
    if is(&[0xFF, 0xD8, 0xFF]) {
        return Some(Route::Native("image/jpeg"));
    }
    if is(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some(Route::Native("image/png"));
    }
    if is(b"GIF87a") || is(b"GIF89a") {
        return Some(Route::Native("image/gif"));
    }
    if h.len() >= 12 && &h[..4] == b"RIFF" && &h[8..12] == b"WEBP" {
        return Some(Route::Native("image/webp"));
    }
    if is(b"BM") {
        return Some(Route::Native("image/bmp"));
    }
    if is(&[0, 0, 1, 0]) {
        return Some(Route::Native("image/x-icon"));
    }
    if is(b"II*\0") || is(b"MM\0*") {
        return Some(Route::Raster);
    }
    if h.len() >= 12 && &h[4..8] == b"ftyp" && matches!(&h[8..12], b"avif" | b"avis") {
        return Some(Route::Native("image/avif"));
    }
    None
}

fn has_pdf_header(h: &[u8]) -> bool {
    h.len() >= 5 && h[..h.len().min(1024)].windows(5).any(|w| w == b"%PDF-")
}

fn looks_like_svg(h: &[u8]) -> bool {
    String::from_utf8_lossy(h).to_lowercase().contains("<svg")
}

fn route(ext: &str, head: &[u8]) -> Route {
    match ext {
        "pdf" => if has_pdf_header(head) { Route::Pdf { illustrator: false } } else { Route::NotImage },
        "ai" | "eps" | "epsf" | "ps" => {
            if has_pdf_header(head) {
                Route::Pdf { illustrator: ext == "ai" }
            } else {
                Route::Eps
            }
        }
        "psd" | "psb" => if head.starts_with(b"8BPS") { Route::Psd } else { Route::NotImage },
        "svg" => if looks_like_svg(head) { Route::Svg } else { Route::NotImage },
        "jpg" | "jpeg" | "jpe" | "jfif" | "png" | "gif" | "webp" | "bmp" | "ico" | "avif" | "tif" | "tiff" => sniff_image(head).unwrap_or(Route::NotImage),
        "heic" | "heif" => Route::Shell,
        "arw" | "nef" | "dng" | "cr2" | "orf" | "pef" | "srw" | "sr2" => Route::RawCamera,
        "mp4" | "m4v" | "mov" => Route::Video("video/mp4"),
        "webm" => Route::Video("video/webm"),
        "ogv" => Route::Video("video/ogg"),
        "mp3" => Route::Audio("audio/mpeg"),
        "m4a" | "aac" => Route::Audio("audio/mp4"),
        "wav" => Route::Audio("audio/wav"),
        "flac" => Route::Audio("audio/flac"),
        "ogg" | "oga" | "opus" => Route::Audio("audio/ogg"),
        e if crate::preview::is_text_ext(e) => Route::Text,
        e if is_thumb_ext(e) => Route::Shell,
        _ => Route::None,
    }
}

/// `\\.\dispositivo` y similares no son archivos: el visor nunca los lee.
fn is_device_path(p: &Path) -> bool {
    matches!(p.components().next(), Some(Component::Prefix(c)) if matches!(c.kind(), Prefix::DeviceNS(_)))
}

fn dims_fact(w: u32, h: u32) -> Fact {
    fact("Dimensiones", format!("{w} × {h} px"))
}

enum Fail {
    Cancelled,
    Message(String),
}

type Check<'a> = &'a dyn Fn() -> bool;

pub struct ViewerService {
    tickets: Tickets,
    cache: ThumbCache,
    thumbs: Arc<ThumbService>,
    current: Mutex<Option<Arc<AtomicBool>>>,
    /// Una sola conversión a la vez: acota el pico de memoria y no compite con la navegación.
    render_slot: Mutex<()>,
}

const CORRUPT: &str = "No se pudo leer este archivo: puede estar dañado.";

impl ViewerService {
    pub fn new(cache_dir: PathBuf, thumbs: Arc<ThumbService>) -> Self {
        ViewerService { tickets: Tickets::default(), cache: ThumbCache::new(cache_dir, MAX_CACHE_BYTES), thumbs, current: Mutex::new(None), render_slot: Mutex::new(()) }
    }

    pub fn tickets(&self) -> &Tickets {
        &self.tickets
    }

    fn begin(&self) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        if let Some(prev) = self.current.lock().unwrap_or_else(|e| e.into_inner()).replace(flag.clone()) {
            prev.store(true, Ordering::SeqCst);
        }
        flag
    }

    /// El visor se cerró: se abandona lo pendiente y se retiran todos los permisos.
    pub fn close(&self) {
        if let Some(prev) = self.current.lock().unwrap_or_else(|e| e.into_inner()).take() {
            prev.store(true, Ordering::SeqCst);
        }
        self.tickets.clear();
    }

    /// Prepara `path`. Una llamada nueva cancela la anterior.
    pub fn open(&self, path: &Path) -> Result<Source, String> {
        let flag = self.begin();
        let cancelled = || flag.load(Ordering::SeqCst);
        if is_device_path(path) {
            return Ok(unsupported("Esta ruta no es un archivo."));
        }
        let meta = fs::metadata(path).map_err(|e| crate::ops::classify(&e).1)?;
        if !meta.is_file() {
            return Ok(unsupported("Las carpetas no tienen vista previa."));
        }
        if crate::dir::is_cloud_only(&meta) {
            return Ok(unsupported("Este archivo está solo en la nube. Para no descargarlo sin tu permiso no se previsualiza; ábrelo para descargarlo."));
        }
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let mut head = vec![0u8; SNIFF_BYTES];
        let n = File::open(path).and_then(|mut f| f.read(&mut head)).map_err(|e| crate::ops::classify(&e).1)?;
        head.truncate(n);
        match self.prepare(path, &meta, &route(&ext, &head), &cancelled) {
            Ok(s) => Ok(s),
            Err(Fail::Cancelled) => Ok(Source::Cancelled),
            Err(Fail::Message(m)) => Ok(unsupported(m)),
        }
    }

    fn token(&self, file: &Path, mime: &'static str) -> Result<String, Fail> {
        self.tickets.issue(file, mime).map_err(|e| Fail::Message(crate::ops::classify(&e).1))
    }

    fn prepare(&self, path: &Path, meta: &fs::Metadata, route: &Route, cancelled: Check) -> Result<Source, Fail> {
        match route {
            Route::Native(mime) => self.native_image(path, meta, mime, cancelled),
            Route::Svg => {
                if meta.len() > MAX_SVG_BYTES {
                    return Ok(unsupported("Este SVG es demasiado grande para mostrarlo."));
                }
                Ok(Source::Image { token: self.token(path, "image/svg+xml")?, mime: "image/svg+xml".into(), width: None, height: None, reduced: false, note: None, facts: vec![] })
            }
            Route::Raster => {
                let dims = isolate(|| image::image_dimensions(path).ok()).ok().flatten();
                self.raster(path, meta, dims, cancelled)
            }
            Route::Pdf { illustrator } => Ok(Source::Pdf { token: self.token(path, "application/pdf")?, size: meta.len(), illustrator: *illustrator }),
            Route::Video(m) => Ok(Source::Video { token: self.token(path, m)?, mime: (*m).into() }),
            Route::Audio(m) => Ok(Source::Audio { token: self.token(path, m)?, mime: (*m).into() }),
            Route::Text => Ok(Source::Text),
            Route::Psd => self.psd(path, meta, cancelled),
            Route::Eps => self.eps(path, meta, cancelled),
            Route::RawCamera => self.raw(path, meta, cancelled),
            Route::Shell => self.shell(path, cancelled, vec![], "Vista reducida proporcionada por Windows."),
            Route::NotImage => Ok(unsupported("El contenido del archivo no coincide con su extensión, o está dañado.")),
            Route::None => Ok(unsupported("Este tipo de archivo no tiene vista previa.")),
        }
    }

    fn native_image(&self, path: &Path, meta: &fs::Metadata, mime: &'static str, cancelled: Check) -> Result<Source, Fail> {
        // La cabecera basta para conocer las dimensiones; AVIF no la lee `image` y las informa la webview.
        let dims = if mime == "image/avif" { None } else { isolate(|| image::image_dimensions(path).ok()).ok().flatten() };
        let too_big = meta.len() > MAX_NATIVE_BYTES || dims.map_or(false, |(w, h)| w as u64 * h as u64 > MAX_NATIVE_PIXELS);
        if too_big && mime != "image/avif" {
            return self.raster(path, meta, dims, cancelled);
        }
        let (width, height) = dims.map_or((None, None), |(w, h)| (Some(w), Some(h)));
        Ok(Source::Image { token: self.token(path, mime)?, mime: mime.into(), width, height, reduced: false, note: None, facts: vec![] })
    }

    /// Ocupa el turno de conversión, atento a la cancelación.
    fn wait_slot(&self, cancelled: Check) -> Result<std::sync::MutexGuard<'_, ()>, Fail> {
        loop {
            match self.render_slot.try_lock() {
                Ok(g) => return Ok(g),
                Err(std::sync::TryLockError::Poisoned(e)) => return Ok(e.into_inner()),
                Err(std::sync::TryLockError::WouldBlock) => {}
            }
            if cancelled() {
                return Err(Fail::Cancelled);
            }
            std::thread::sleep(Duration::from_millis(15));
        }
    }

    /// Devuelve la conversión en caché o la genera. `make` corre dentro de una frontera de pánicos.
    fn cached(&self, path: &Path, meta: &fs::Metadata, side: u32, cancelled: Check, make: &dyn Fn() -> Result<Thumb, Fail>) -> Result<(PathBuf, &'static str), Fail> {
        let key = ThumbCache::key(path, meta.len(), mtime_ms(meta), side);
        match self.cache.get(&key) {
            CacheHit::Hit(_) => {
                if let Some(f) = self.cache.stored_file(&key) {
                    return Ok(f);
                }
            }
            CacheHit::Negative(_) => return Err(Fail::Message(CORRUPT.into())),
            CacheHit::Miss => {}
        }
        let _slot = self.wait_slot(cancelled)?;
        if cancelled() {
            return Err(Fail::Cancelled);
        }
        let out = isolate(make).unwrap_or_else(|_| Err(Fail::Message(CORRUPT.into())));
        match out {
            Ok(t) => {
                self.cache.put(&key, &t);
                self.cache.prune();
                self.cache.stored_file(&key).ok_or_else(|| Fail::Message("No se pudo guardar la vista previa en la caché.".into()))
            }
            Err(Fail::Message(m)) => {
                self.cache.put_negative(&key, &ThumbError::Failed(m.clone()));
                Err(Fail::Message(m))
            }
            Err(Fail::Cancelled) => Err(Fail::Cancelled),
        }
    }

    fn rendered(&self, file: PathBuf, mime: &'static str, reduced: bool, note: Option<String>, facts: Vec<Fact>) -> Result<Source, Fail> {
        let dims = isolate(|| image::image_dimensions(&file).ok()).ok().flatten();
        let (width, height) = dims.map_or((None, None), |(w, h)| (Some(w), Some(h)));
        Ok(Source::Image { token: self.token(&file, mime)?, mime: mime.into(), width, height, reduced, note, facts })
    }

    fn raster(&self, path: &Path, meta: &fs::Metadata, dims: Option<(u32, u32)>, cancelled: Check) -> Result<Source, Fail> {
        let make = || {
            RasterProvider::default().render(path, MAX_RENDER_SIDE, cancelled).map_err(|e| match e {
                ThumbError::Cancelled => Fail::Cancelled,
                ThumbError::TooLarge => Fail::Message("La imagen es demasiado grande para generar una vista previa.".into()),
                ThumbError::Unsupported => Fail::Message("Esta variante del formato no se puede decodificar.".into()),
                ThumbError::Failed(_) => Fail::Message(CORRUPT.into()),
            })
        };
        let (file, mime) = self.cached(path, meta, MAX_RENDER_SIDE, cancelled, &make)?;
        let reduced = dims.map_or(false, |(w, h)| w.max(h) > MAX_RENDER_SIDE);
        let facts = dims.map(|(w, h)| vec![dims_fact(w, h)]).unwrap_or_default();
        self.rendered(file, mime, reduced, reduced.then(|| "Vista reducida: el original es muy grande.".to_string()), facts)
    }

    fn psd(&self, path: &Path, meta: &fs::Metadata, cancelled: Check) -> Result<Source, Fail> {
        let mut f = File::open(path).map_err(|e| Fail::Message(crate::ops::classify(&e).1))?;
        let info = match isolate(|| psd::read_info(&mut f)) {
            Ok(Ok(i)) => i,
            Ok(Err(psd::PsdError::Unsupported(m))) => return Ok(unsupported(m)),
            _ => return Ok(unsupported(CORRUPT)),
        };
        let mut facts = vec![dims_fact(info.width, info.height), fact("Modo de color", info.mode_name()), fact("Profundidad", format!("{} bits por canal", info.depth))];
        if let Some(d) = info.dpi {
            facts.push(fact("Resolución", format!("{} ppp", d.round() as u32)));
        }
        let long = info.width.max(info.height);
        let side = long.min(MAX_RENDER_SIDE);
        // Sin composición guardada solo hay una miniatura pequeña: se dice claramente.
        let mut note = (long > MAX_RENDER_SIDE).then(|| "Composición reducida del documento.".to_string());
        if !info.has_merged {
            note = Some("Este PSD se guardó sin «Maximizar compatibilidad»: se muestra la miniatura incorporada, no la composición completa.".into());
        }
        let make = || -> Result<Thumb, Fail> {
            let mut f = File::open(path).map_err(|_| Fail::Message(CORRUPT.into()))?;
            let thumb_only = !info.has_merged;
            if !thumb_only {
                let src = File::open(path).map_err(|_| Fail::Message(CORRUPT.into()))?;
                match psd::render_merged(src, &info, side, cancelled) {
                    Ok(img) => return encode_thumb(DynamicImage::ImageRgba8(img), side).map_err(|_| Fail::Message(CORRUPT.into())),
                    Err(psd::PsdError::Cancelled) => return Err(Fail::Cancelled),
                    Err(_) => {}
                }
            }
            match psd::read_thumbnail(&mut f, &info) {
                Some(img) => encode_thumb(img, side).map_err(|_| Fail::Message(CORRUPT.into())),
                None => Err(Fail::Message("Este PSD no contiene una composición ni una miniatura que se puedan mostrar.".into())),
            }
        };
        match self.cached(path, meta, side, cancelled, &make) {
            Ok((file, mime)) => {
                let mut src = self.rendered(file, mime, long > MAX_RENDER_SIDE || !info.has_merged, note, facts.clone())?;
                // Si la composición falló y se recurrió a la miniatura, las dimensiones de salida lo delatan.
                if let Source::Image { width: Some(w), height: Some(h), reduced, note, .. } = &mut src {
                    if info.has_merged && (*w).max(*h) < side / 2 {
                        *reduced = true;
                        *note = Some("No se pudo leer la composición: se muestra la miniatura incorporada.".into());
                    }
                }
                Ok(src)
            }
            Err(Fail::Message(m)) => Ok(Source::Unsupported { message: m, facts }),
            Err(e) => Err(e),
        }
    }

    fn eps(&self, path: &Path, meta: &fs::Metadata, cancelled: Check) -> Result<Source, Fail> {
        const NONE: &str = "Este archivo no incluye una vista previa ni contenido PDF. LFind no ejecuta PostScript, así que no puede mostrarlo.";
        let make = || -> Result<Thumb, Fail> {
            match embedded::eps_preview(path) {
                Ok(Some(img)) => encode_thumb(img, MAX_RENDER_SIDE).map_err(|_| Fail::Message(CORRUPT.into())),
                Ok(None) => Err(Fail::Message(NONE.into())),
                Err(_) => Err(Fail::Message(CORRUPT.into())),
            }
        };
        match self.cached(path, meta, MAX_RENDER_SIDE, cancelled, &make) {
            Ok((file, mime)) => self.rendered(file, mime, true, Some("Previsualización incorporada en el archivo.".into()), vec![]),
            // La caché negativa no distingue el motivo: el mensaje honesto en ambos casos es este.
            Err(Fail::Message(_)) => Ok(unsupported(NONE)),
            Err(e) => Err(e),
        }
    }

    fn raw(&self, path: &Path, meta: &fs::Metadata, cancelled: Check) -> Result<Source, Fail> {
        let make = || -> Result<Thumb, Fail> {
            match embedded::raw_preview(path) {
                Ok(Some(img)) => encode_thumb(img, MAX_RENDER_SIDE).map_err(|_| Fail::Message(CORRUPT.into())),
                _ => Err(Fail::Message("Sin vista previa incrustada.".into())),
            }
        };
        match self.cached(path, meta, MAX_RENDER_SIDE, cancelled, &make) {
            Ok((file, mime)) => self.rendered(file, mime, true, Some("Vista previa incrustada por la cámara; no es el revelado del RAW.".into()), vec![]),
            Err(Fail::Message(_)) => self.shell(path, cancelled, vec![], "Vista reducida proporcionada por Windows."),
            Err(e) => Err(e),
        }
    }

    /// Último recurso: la miniatura real de contenido que da Windows (si hay proveedor). Nunca el icono genérico.
    fn shell(&self, path: &Path, cancelled: Check, facts: Vec<Fact>, note: &str) -> Result<Source, Fail> {
        if cancelled() {
            return Err(Fail::Cancelled);
        }
        let meta = fs::metadata(path).map_err(|e| Fail::Message(crate::ops::classify(&e).1))?;
        let key = ThumbCache::key(path, meta.len(), mtime_ms(&meta), SHELL_SIDE);
        let hit = match self.cache.get(&key) {
            CacheHit::Hit(_) => self.cache.stored_file(&key),
            _ => None,
        };
        let (file, mime) = match hit {
            Some(f) => f,
            None => match self.thumbs.get(path, SHELL_SIDE, None) {
                Ok(t) => {
                    self.cache.put(&key, &t);
                    self.cache.prune();
                    self.cache.stored_file(&key).ok_or_else(|| Fail::Message("No se pudo guardar la vista previa en la caché.".into()))?
                }
                Err(ThumbError::Cancelled) => return Err(Fail::Cancelled),
                Err(_) => {
                    return Ok(Source::Unsupported { message: "Windows no tiene instalado un proveedor de vistas previas para este formato.".into(), facts });
                }
            },
        };
        self.rendered(file, mime, true, Some(note.to_string()), facts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::thumbs::ThumbConfig;

    fn tmp() -> PathBuf {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let d = std::env::temp_dir().join(format!("nova-viewer-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn service(d: &Path) -> ViewerService {
        let t = Arc::new(ThumbService::new(ThumbConfig { cache_dir: d.join("t"), max_cache_bytes: 1 << 20, parallel: 1 }, vec![]));
        ViewerService::new(d.join("v"), t)
    }

    fn png(w: u32, h: u32, p: &Path) {
        DynamicImage::new_rgba8(w, h).save(p).unwrap();
    }

    #[test]
    fn routes_use_content_not_only_extension() {
        assert_eq!(route("png", b"\x89PNG\r\n\x1a\n...."), Route::Native("image/png"));
        assert_eq!(route("png", b"\xFF\xD8\xFF\xE0"), Route::Native("image/jpeg"), "extensión equivocada, contenido válido");
        assert_eq!(route("jpg", b"<html><script>"), Route::NotImage, "lo que no es imagen no se sirve como imagen");
        assert_eq!(route("pdf", b"nada"), Route::NotImage);
        assert_eq!(route("pdf", b"\n\n%PDF-1.7"), Route::Pdf { illustrator: false });
        assert_eq!(route("ai", b"%PDF-1.6"), Route::Pdf { illustrator: true });
        assert_eq!(route("ai", b"%!PS-Adobe-3.0"), Route::Eps);
        assert_eq!(route("eps", b"%!PS-Adobe-3.0"), Route::Eps);
        assert_eq!(route("psd", b"8BPS"), Route::Psd);
        assert_eq!(route("psd", b"otra"), Route::NotImage);
        assert_eq!(route("svg", b"<?xml?><SVG>"), Route::Svg);
        assert_eq!(route("svg", b"hola"), Route::NotImage);
        assert_eq!(route("tif", b"II*\0"), Route::Raster);
        assert_eq!(route("mp4", b""), Route::Video("video/mp4"));
        assert_eq!(route("exe", b"MZ"), Route::None);
        assert_eq!(route("txt", b"hola"), Route::Text);
    }

    #[test]
    fn device_paths_are_never_read() {
        if cfg!(windows) {
            assert!(is_device_path(Path::new(r"\\.\PhysicalDrive0")));
            assert!(!is_device_path(Path::new(r"C:\a.png")));
        }
    }

    #[test]
    fn native_image_keeps_its_bytes_and_reports_dimensions() {
        let d = tmp();
        let p = d.join("a.png");
        png(30, 20, &p);
        let s = service(&d);
        match s.open(&p).unwrap() {
            Source::Image { token, width, height, reduced, .. } => {
                assert_eq!((width, height, reduced), (Some(30), Some(20), false));
                let r = serve::serve(s.tickets(), &serve::ServeRequest { method: "GET", token: &token, range: None, origin: None });
                assert_eq!(r.body, fs::read(&p).unwrap());
            }
            o => panic!("{o:?}"),
        }
    }

    #[test]
    fn tiff_is_converted_cached_and_invalidated_when_the_file_changes() {
        let d = tmp();
        let p = d.join("a.tif");
        DynamicImage::new_rgb8(40, 10).save(&p).unwrap();
        let s = service(&d);
        let first = s.open(&p).unwrap();
        let Source::Image { token, width, .. } = &first else { panic!("{first:?}") };
        assert_eq!(*width, Some(40));
        assert_eq!(s.open(&p).unwrap(), first, "segunda vez: misma caché y mismo permiso");
        let t = token.clone();
        std::thread::sleep(Duration::from_millis(30));
        DynamicImage::new_rgb8(80, 10).save(&p).unwrap();
        let Source::Image { token: t2, width: w2, .. } = s.open(&p).unwrap() else { panic!() };
        assert_ne!(t2, t);
        assert_eq!(w2, Some(80));
    }

    #[test]
    fn corrupt_and_unknown_files_explain_themselves() {
        let d = tmp();
        let s = service(&d);
        for (name, body) in [("a.png", &b"no soy png"[..]), ("b.bin", b"x"), ("c.psd", b"8BPS\0\x01rota"), ("d.pdf", b"nada")] {
            let p = d.join(name);
            fs::write(&p, body).unwrap();
            assert!(matches!(s.open(&p).unwrap(), Source::Unsupported { .. }), "{name}");
        }
        assert!(s.open(&d.join("no-existe.png")).is_err());
        assert!(matches!(s.open(&d).unwrap(), Source::Unsupported { .. }));
    }

    #[test]
    fn a_new_open_cancels_the_previous_and_close_revokes_everything() {
        let d = tmp();
        let s = service(&d);
        let first = s.begin();
        let _second = s.begin();
        assert!(first.load(Ordering::SeqCst));
        let p = d.join("a.png");
        png(4, 4, &p);
        let Source::Image { token, .. } = s.open(&p).unwrap() else { panic!() };
        s.close();
        assert_eq!(serve::serve(s.tickets(), &serve::ServeRequest { method: "GET", token: &token, range: None, origin: None }).status, 404);
    }
}
