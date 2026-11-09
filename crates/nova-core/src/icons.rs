//! Iconos del sistema (los que muestra el Explorador de Windows), distintos de las miniaturas.
//!
//! * **Icono del sistema** = el icono asociado al TIPO de archivo o el propio de ese archivo (EXE, LNK, carpeta
//!   personalizada…). Es lo que se ve en una carpeta sin miniaturas.
//! * **Miniatura** = contenido del archivo reducido (`thumbs`). Son dos servicios y dos cachés independientes.
//!
//! Para no pedir un icono por archivo, cada elemento se clasifica en una de dos fuentes:
//!  * `Type`: icono compartido por extensión (`docx`, `mp4`, `zip`, `dll`…) o genérico de carpeta. Se resuelve
//!    UNA vez por sesión y tamaño (clave `t-docx-64`, `d-64`) con independencia de cuántos archivos haya.
//!  * `Path`: icono propio del archivo (`exe`, `lnk`, `url`, `ico`…, carpetas con `desktop.ini` que declara icono
//!    y raíces de unidad). Clave con ruta, tamaño y fecha: si el archivo cambia, la clave cambia (invalidación).
//!
//! Nunca se ejecuta nada: el proveedor de Windows solo LEE el recurso de icono (SHGetFileInfo / lista de imágenes del shell).
//! La interfaz envía lotes (`resolve`) y dice qué claves compartidas ya tiene (`have`) para no recibir bytes repetidos.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Súbela si cambia el aspecto de los iconos (invalida la caché en disco).
pub const CACHE_VERSION: u32 = 2;
pub const DISK_MAX_BYTES: u64 = 64 * 1024 * 1024;
pub const FAILED_TTL: Duration = Duration::from_secs(5 * 60);
const MEM_MAX: usize = 2000;

/// Extensiones cuyo icono es propio de cada archivo (se leen del archivo). Debe coincidir con `src/lib/iconKinds.ts`.
pub const PER_FILE_EXTS: &[&str] = &["exe", "scr", "lnk", "url", "ico", "cpl", "appref-ms"];

/// Imagen RGBA con alfa recto, tal como la entrega el proveedor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawIcon {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

pub trait IconProvider: Send + Sync {
    /// Icono asociado a un tipo de archivo (`ext` en minúsculas y sin punto; vacío = genérico) o a «carpeta». No toca el disco.
    fn type_icon(&self, ext: &str, dir: bool, px: u32) -> Result<RawIcon, String>;
    /// Icono propio de un archivo o carpeta existente (respeta iconos de accesos directos, EXE, `desktop.ini`).
    fn file_icon(&self, path: &Path, px: u32) -> Result<RawIcon, String>;
}

/// Proveedor sin soporte (sistemas que no son Windows): la interfaz conserva sus glifos.
pub struct NoIcons;
impl IconProvider for NoIcons {
    fn type_icon(&self, _: &str, _: bool, _: u32) -> Result<RawIcon, String> {
        Err("Los iconos del sistema solo existen en Windows.".into())
    }
    fn file_icon(&self, _: &Path, _: u32) -> Result<RawIcon, String> {
        Err("Los iconos del sistema solo existen en Windows.".into())
    }
}

pub fn default_provider() -> Arc<dyn IconProvider> {
    #[cfg(windows)]
    {
        Arc::new(crate::icons_win::WinIcons)
    }
    #[cfg(not(windows))]
    {
        Arc::new(NoIcons)
    }
}

// ───────────────────────── clasificación ─────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Type { ext: String, dir: bool },
    Path { path: PathBuf, dir: bool },
}

/// Tamaños de icono que se piden: 32, 48, 64, 128 o 256.
pub fn bucket(px: u32) -> u32 {
    match px {
        0..=32 => 32,
        33..=48 => 48,
        49..=64 => 64,
        65..=128 => 128,
        _ => 256,
    }
}

pub fn ext_of(path: &Path) -> String {
    let e = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    // Solo extensiones «normales»: algo raro se trata como archivo genérico (y evita cadenas absurdas hacia la API).
    if !e.is_empty() && e.len() <= 16 && e.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        e
    } else {
        String::new()
    }
}

fn is_drive_root(path: &Path) -> bool {
    let s = path.to_string_lossy();
    let t = s.trim_end_matches(['\\', '/']);
    let b = t.as_bytes();
    b.len() == 2 && b[1] == b':' && b[0].is_ascii_alphabetic()
}

/// ¿Declara el `desktop.ini` un icono propio? Admite UTF-8 y UTF-16 LE (el formato habitual de Windows).
pub fn desktop_ini_declares_icon(bytes: &[u8]) -> bool {
    let text: String = if bytes.starts_with(&[0xFF, 0xFE]) {
        let u: Vec<u16> = bytes[2..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&u)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    };
    text.lines().any(|l| {
        let k = l.split('=').next().unwrap_or("").trim().to_ascii_lowercase();
        k == "iconfile" || k == "iconresource" || k == "clsid"
    })
}

fn custom_folder_icon(dir: &Path) -> Option<fs::Metadata> {
    let ini = dir.join("desktop.ini");
    let md = fs::metadata(&ini).ok()?;
    if !md.is_file() || md.len() > 16 * 1024 {
        return None;
    }
    let bytes = fs::read(&ini).ok()?;
    desktop_ini_declares_icon(&bytes).then_some(md)
}

pub fn classify(path: &Path, dir: bool) -> Source {
    if dir {
        if is_drive_root(path) || custom_folder_icon(path).is_some() {
            Source::Path { path: path.to_path_buf(), dir: true }
        } else {
            Source::Type { ext: String::new(), dir: true }
        }
    } else {
        let e = ext_of(path);
        // Leer el icono propio de un archivo que solo está en la nube lo descargaría: se usa el del tipo.
        if PER_FILE_EXTS.contains(&e.as_str()) && !fs::metadata(path).is_ok_and(|m| crate::dir::is_cloud_only(&m)) {
            Source::Path { path: path.to_path_buf(), dir: false }
        } else {
            Source::Type { ext: e, dir: false }
        }
    }
}

// ───────────────────────── claves ─────────────────────────

/// Clave de un icono compartido. La interfaz la reproduce (`iconKinds.ts`): cualquier cambio aquí exige cambiarla allí.
pub fn type_key(ext: &str, dir: bool, px: u32) -> String {
    if dir {
        format!("d-{px}")
    } else {
        format!("t-{}-{px}", if ext.is_empty() { "_" } else { ext })
    }
}

fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Clave de un icono propio: cambia si cambia la ruta, el tamaño/fecha del archivo (o del `desktop.ini`), el tamaño pedido o la versión.
pub fn path_key(path: &Path, a: u64, b: i64, px: u32) -> String {
    let p = path.to_string_lossy().to_lowercase().replace('/', "\\");
    format!("f-{:016x}-{a}-{b}-{px}-v{CACHE_VERSION}", fnv1a(&p))
}

fn mtime_ms(md: &fs::Metadata) -> i64 {
    md.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis() as i64)
}

// ───────────────────────── caché en disco (solo iconos propios) ─────────────────────────

struct DiskCache {
    dir: PathBuf,
    puts: Mutex<usize>,
}

impl DiskCache {
    fn path_of(&self, key: &str, ext: &str) -> PathBuf {
        self.dir.join(&key[..2.min(key.len())]).join(format!("{key}.{ext}"))
    }
    fn get(&self, key: &str) -> Option<Vec<u8>> {
        fs::read(self.path_of(key, "png")).ok().filter(|b| !b.is_empty())
    }
    fn is_negative(&self, key: &str) -> bool {
        let p = self.path_of(key, "fail");
        match fs::metadata(&p) {
            Ok(m) => {
                let fresh = m.modified().ok().and_then(|t| t.elapsed().ok()).map_or(false, |age| age < FAILED_TTL);
                if !fresh {
                    let _ = fs::remove_file(&p);
                }
                fresh
            }
            Err(_) => false,
        }
    }
    fn write(&self, key: &str, ext: &str, bytes: &[u8]) {
        let p = self.path_of(key, ext);
        if let Some(d) = p.parent() {
            let _ = fs::create_dir_all(d);
        }
        let tmp = p.with_extension(format!("{ext}.tmp{}", std::process::id()));
        if fs::write(&tmp, bytes).is_ok() && fs::rename(&tmp, &p).is_err() {
            let _ = fs::remove_file(&tmp);
        }
        let mut n = lock(&self.puts);
        *n += 1;
        if *n % 200 == 0 {
            self.prune();
        }
    }
    /// Borra lo más antiguo hasta bajar del límite.
    fn prune(&self) {
        let mut files: Vec<(SystemTime, u64, PathBuf)> = vec![];
        let Ok(rd) = fs::read_dir(&self.dir) else { return };
        for d in rd.flatten() {
            if let Ok(inner) = fs::read_dir(d.path()) {
                for f in inner.flatten() {
                    if let Ok(m) = f.metadata() {
                        files.push((m.modified().unwrap_or(UNIX_EPOCH), m.len(), f.path()));
                    }
                }
            }
        }
        let mut total: u64 = files.iter().map(|f| f.1).sum();
        if total <= DISK_MAX_BYTES {
            return;
        }
        files.sort_by_key(|f| f.0);
        for (_, len, p) in files {
            if total <= DISK_MAX_BYTES * 8 / 10 {
                break;
            }
            if fs::remove_file(&p).is_ok() {
                total = total.saturating_sub(len);
            }
        }
    }
    fn clear(&self) -> u64 {
        let mut freed = 0;
        if let Ok(rd) = fs::read_dir(&self.dir) {
            for d in rd.flatten() {
                if let Ok(inner) = fs::read_dir(d.path()) {
                    for f in inner.flatten() {
                        freed += f.metadata().map_or(0, |m| m.len());
                    }
                }
                let _ = fs::remove_dir_all(d.path());
            }
        }
        freed
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// ───────────────────────── codificación ─────────────────────────

/// Reduce (nunca amplía) a `px` y codifica en PNG.
pub type Encoder = fn(&RawIcon, u32) -> Result<Vec<u8>, String>;

pub fn encode_png(icon: &RawIcon, px: u32) -> Result<Vec<u8>, String> {
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    use image::{ExtendedColorType, ImageEncoder, RgbaImage};
    let img = RgbaImage::from_raw(icon.w, icon.h, icon.rgba.clone()).ok_or("Icono con tamaño inconsistente.")?;
    let img = if icon.w > px || icon.h > px {
        let s = px as f32 / icon.w.max(icon.h) as f32;
        let (nw, nh) = (((icon.w as f32 * s).round() as u32).max(1), ((icon.h as f32 * s).round() as u32).max(1));
        image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };
    let mut out = Vec::with_capacity(8 * 1024);
    PngEncoder::new_with_quality(&mut out, CompressionType::Fast, FilterType::Adaptive)
        .write_image(img.as_raw(), img.width(), img.height(), ExtendedColorType::Rgba8)
        .map_err(|e| format!("No se pudo codificar el icono: {e}"))?;
    Ok(out)
}

/// Tamaños con los que se diseñan los iconos.
const ICON_FRAMES: [u32; 10] = [16, 20, 24, 32, 40, 48, 64, 72, 96, 128];

/// Recupera el icono real cuando Windows lo entrega arrinconado.
///
/// Si un archivo no tiene icono de 256 px, la lista grande del sistema no lo escala: lo deja en la esquina superior izquierda de un
/// lienzo transparente. Al reducir ese lienzo, el icono queda diminuto y desplazado. Solo actúa si todo el contenido cabe en el cuadrante
/// superior izquierdo, algo que un icono diseñado a ese tamaño no hace. Recorta al marco original: no deforma ni altera píxeles ni alfa.
pub fn without_corner_padding(icon: RawIcon) -> RawIcon {
    let (w, h) = (icon.w as usize, icon.h as usize);
    if w != h || icon.w < 64 || icon.rgba.len() != w * h * 4 {
        return icon;
    }
    let (mut right, mut bottom) = (0usize, 0usize);
    for (i, p) in icon.rgba.chunks_exact(4).enumerate() {
        if p[3] > 8 {
            right = right.max(i % w + 1);
            bottom = bottom.max(i / w + 1);
        }
    }
    if right == 0 || right > w / 2 || bottom > h / 2 {
        return icon;
    }
    let content = right.max(bottom) as u32;
    let frame = ICON_FRAMES.iter().copied().find(|f| *f >= content).unwrap_or(content);
    let side = frame as usize;
    let mut rgba = Vec::with_capacity(side * side * 4);
    for row in icon.rgba.chunks_exact(w * 4).take(side) {
        rgba.extend_from_slice(&row[..side * 4]);
    }
    RawIcon { w: frame, h: frame, rgba }
}

// ───────────────────────── servicio ─────────────────────────

#[derive(Clone)]
enum Job {
    Type { ext: String, dir: bool },
    Path { path: PathBuf },
}

pub struct IconBatch {
    /// Una clave por elemento pedido (en el mismo orden). `None` = sin icono del sistema: la interfaz conserva su glifo.
    pub keys: Vec<Option<String>>,
    /// PNG de las claves que la interfaz no tenía (`have`). Sin duplicados.
    pub icons: Vec<(String, Arc<Vec<u8>>)>,
}

pub struct IconService {
    provider: Arc<dyn IconProvider>,
    disk: DiskCache,
    /// Iconos compartidos de la sesión (`None` = el sistema no pudo darlo; no se reintenta en esta sesión).
    mem: Mutex<HashMap<String, Option<Arc<Vec<u8>>>>>,
    encoder: Encoder,
    deadline: Duration,
    workers: usize,
}

impl IconService {
    pub fn new(provider: Arc<dyn IconProvider>, cache_dir: PathBuf, encoder: Encoder, deadline: Duration, workers: usize) -> Self {
        IconService { provider, disk: DiskCache { dir: cache_dir, puts: Mutex::new(0) }, mem: Mutex::new(HashMap::new()), encoder, deadline, workers: workers.max(1) }
    }

    pub fn with_defaults(cache_dir: PathBuf) -> Self {
        Self::new(default_provider(), cache_dir, encode_png, Duration::from_secs(5), 2)
    }

    /// Vacía las cachés (memoria y disco). Devuelve los bytes liberados en disco.
    pub fn clear_cache(&self) -> u64 {
        lock(&self.mem).clear();
        self.disk.clear()
    }

    pub fn resolve(&self, items: &[(PathBuf, bool)], px: u32, have: &HashSet<String>) -> IconBatch {
        let px = bucket(px);
        // 1) Clasificar y calcular la clave de cada elemento (una `stat` solo para los iconos propios).
        let mut keys: Vec<Option<String>> = Vec::with_capacity(items.len());
        let mut jobs: HashMap<String, Job> = HashMap::new();
        for (path, dir) in items {
            let key = match classify(path, *dir) {
                Source::Type { ext, dir } => {
                    let k = type_key(&ext, dir, px);
                    jobs.entry(k.clone()).or_insert(Job::Type { ext, dir });
                    Some(k)
                }
                Source::Path { path, dir } => match fs::metadata(&path) {
                    Ok(md) => {
                        let k = if dir {
                            let ini = fs::metadata(path.join("desktop.ini")).map_or(0, |m| mtime_ms(&m) as u64);
                            path_key(&path, ini, mtime_ms(&md), px)
                        } else {
                            path_key(&path, md.len(), mtime_ms(&md), px)
                        };
                        jobs.entry(k.clone()).or_insert(Job::Path { path });
                        Some(k)
                    }
                    Err(_) => None,
                },
            };
            keys.push(key);
        }

        // 2) Resolver cada clave única: tenía la interfaz → nada; memoria/disco → bytes; si no → trabajo.
        let mut ready: HashMap<String, Option<Arc<Vec<u8>>>> = HashMap::new();
        let mut todo: VecDeque<(String, Job)> = VecDeque::new();
        for (k, job) in jobs {
            if have.contains(&k) {
                continue;
            }
            match &job {
                Job::Type { .. } => match lock(&self.mem).get(&k) {
                    Some(v) => {
                        ready.insert(k, v.clone());
                    }
                    None => todo.push_back((k, job)),
                },
                Job::Path { .. } => {
                    if let Some(b) = self.disk.get(&k) {
                        ready.insert(k, Some(Arc::new(b)));
                    } else if self.disk.is_negative(&k) {
                        ready.insert(k, None);
                    } else {
                        todo.push_back((k, job));
                    }
                }
            }
        }

        // 3) Extracción en hilos propios con tiempo límite: un manejador de icono colgado no bloquea la aplicación
        //    (su hilo se abandona; el resto del lote sigue en el otro hilo).
        if !todo.is_empty() {
            let total = todo.len();
            let queue = Arc::new(Mutex::new(todo));
            let (tx, rx) = mpsc::channel::<(String, Result<Vec<u8>, String>)>();
            for _ in 0..self.workers.min(total) {
                let (queue, tx, provider, encoder) = (queue.clone(), tx.clone(), self.provider.clone(), self.encoder);
                let _ = std::thread::Builder::new().name("lfind-icons".into()).spawn(move || loop {
                    let next = lock(&queue).pop_front();
                    let Some((k, job)) = next else { break };
                    let r = match &job {
                        Job::Type { ext, dir } => provider.type_icon(ext, *dir, px),
                        Job::Path { path } => provider.file_icon(path, px),
                    }
                    .and_then(|raw| encoder(&raw, px));
                    if tx.send((k, r)).is_err() {
                        break;
                    }
                });
            }
            drop(tx);
            let end = Instant::now() + self.deadline;
            let mut got = 0;
            while got < total {
                let Some(left) = end.checked_duration_since(Instant::now()) else { break };
                match rx.recv_timeout(left) {
                    Ok((k, r)) => {
                        got += 1;
                        let is_type = k.starts_with("t-") || k.starts_with("d-");
                        match r {
                            Ok(b) => {
                                let b = Arc::new(b);
                                if is_type {
                                    self.remember(&k, Some(b.clone()));
                                } else {
                                    self.disk.write(&k, "png", &b);
                                }
                                ready.insert(k, Some(b));
                            }
                            Err(_) => {
                                if is_type {
                                    self.remember(&k, None);
                                } else {
                                    self.disk.write(&k, "fail", b"");
                                }
                                ready.insert(k, None);
                            }
                        }
                    }
                    Err(_) => break, // tiempo agotado o todos los hilos terminaron
                }
            }
        }

        // 4) Armar la respuesta.
        let mut icons = vec![];
        for (k, v) in &ready {
            if let Some(b) = v {
                icons.push((k.clone(), b.clone()));
            }
        }
        icons.sort_by(|a, b| a.0.cmp(&b.0));
        let keys = keys
            .into_iter()
            .map(|k| k.filter(|k| have.contains(k) || matches!(ready.get(k), Some(Some(_)))))
            .collect();
        IconBatch { keys, icons }
    }

    fn remember(&self, k: &str, v: Option<Arc<Vec<u8>>>) {
        let mut m = lock(&self.mem);
        if m.len() >= MEM_MAX {
            m.clear(); // simple y suficiente: se vuelven a pedir los pocos tipos en uso
        }
        m.insert(k.to_string(), v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Fake {
        type_calls: AtomicUsize,
        file_calls: AtomicUsize,
        delay: Duration,
        fail_exts: Vec<&'static str>,
    }
    fn fake(delay_ms: u64) -> Arc<Fake> {
        Arc::new(Fake { type_calls: AtomicUsize::new(0), file_calls: AtomicUsize::new(0), delay: Duration::from_millis(delay_ms), fail_exts: vec!["nada"] })
    }
    impl IconProvider for Arc<Fake> {
        fn type_icon(&self, ext: &str, dir: bool, _px: u32) -> Result<RawIcon, String> {
            self.type_calls.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(self.delay);
            if self.fail_exts.contains(&ext) {
                return Err("sin icono".into());
            }
            Ok(RawIcon { w: 2, h: 2, rgba: vec![ext.len() as u8 + dir as u8; 16] })
        }
        fn file_icon(&self, path: &Path, _px: u32) -> Result<RawIcon, String> {
            self.file_calls.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(self.delay);
            Ok(RawIcon { w: 2, h: 2, rgba: vec![path.to_string_lossy().len() as u8; 16] })
        }
    }
    fn enc(i: &RawIcon, _px: u32) -> Result<Vec<u8>, String> {
        Ok(i.rgba.clone())
    }
    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("lfind-icons-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }
    fn svc(f: &Arc<Fake>, dir: &Path, deadline_ms: u64) -> IconService {
        IconService::new(Arc::new(f.clone()), dir.join("cache"), enc, Duration::from_millis(deadline_ms), 2)
    }
    fn none() -> HashSet<String> {
        HashSet::new()
    }

    #[test]
    fn classification_separates_shared_type_icons_from_per_file_icons() {
        let d = tmp("class");
        assert_eq!(classify(Path::new("C:\\a\\Informe.DOCX"), false), Source::Type { ext: "docx".into(), dir: false });
        assert_eq!(classify(Path::new("C:\\a\\x.dll"), false), Source::Type { ext: "dll".into(), dir: false }, "las DLL comparten icono, como en el Explorador");
        for e in PER_FILE_EXTS {
            assert!(matches!(classify(Path::new(&format!("C:\\a\\x.{e}")), false), Source::Path { .. }), "{e}");
        }
        assert_eq!(classify(Path::new("C:\\a\\LEEME"), false), Source::Type { ext: String::new(), dir: false });
        assert_eq!(classify(Path::new("C:\\a\\raro.ex e!"), false), Source::Type { ext: String::new(), dir: false });
        assert!(matches!(classify(Path::new("C:\\"), true), Source::Path { .. }), "las raíces de unidad tienen icono propio");
        assert!(matches!(classify(Path::new("D:"), true), Source::Path { .. }));
        assert_eq!(classify(&d, true), Source::Type { ext: String::new(), dir: true }, "carpeta normal → icono genérico compartido");
        let custom = d.join("Fotos");
        fs::create_dir_all(&custom).unwrap();
        fs::write(custom.join("desktop.ini"), "[.ShellClassInfo]\r\nIconResource=C:\\x.dll,5\r\n").unwrap();
        assert!(matches!(classify(&custom, true), Source::Path { dir: true, .. }), "desktop.ini con icono → icono propio");
        let plain = d.join("Docs");
        fs::create_dir_all(&plain).unwrap();
        fs::write(plain.join("desktop.ini"), "[.ShellClassInfo]\r\nLocalizedResourceName=@x,-1\r\n").unwrap();
        assert!(matches!(classify(&plain, true), Source::Type { dir: true, .. }), "desktop.ini sin icono no genera una petición por carpeta");
        let _ = fs::remove_dir_all(d);
    }

    /// Lienzo transparente con un cuadrado opaco en `[x0, x1) × [y0, y1)`.
    fn canvas(side: u32, x0: u32, y0: u32, x1: u32, y1: u32) -> RawIcon {
        let mut rgba = vec![0u8; (side * side * 4) as usize];
        for y in y0..y1 {
            for x in x0..x1 {
                let i = ((y * side + x) * 4) as usize;
                rgba[i..i + 4].copy_from_slice(&[10, 20, 30, 200]);
            }
        }
        RawIcon { w: side, h: side, rgba }
    }

    #[test]
    fn a_small_icon_left_in_the_corner_recovers_its_frame() {
        let out = without_corner_padding(canvas(256, 0, 0, 48, 48));
        assert_eq!((out.w, out.h, out.rgba.len()), (48, 48, 48 * 48 * 4));
        assert!(out.rgba.chunks_exact(4).all(|p| p == [10, 20, 30, 200]), "conserva color y alfa");
        let with_margin = without_corner_padding(canvas(256, 6, 3, 42, 43));
        assert_eq!(with_margin.w, 48, "mantiene el margen propio del icono");
        assert_eq!(&with_margin.rgba[(3 * 48 + 6) * 4..(3 * 48 + 6) * 4 + 4], &[10, 20, 30, 200], "sin desplazar el contenido dentro de su marco");
        assert_eq!(without_corner_padding(canvas(256, 0, 0, 32, 20)).w, 32, "contenido no cuadrado: marco cuadrado, sin deformar");
    }

    #[test]
    fn icons_that_use_their_canvas_are_left_untouched() {
        for icon in [canvas(256, 0, 0, 256, 256), canvas(256, 64, 64, 192, 192), canvas(256, 0, 0, 129, 40), canvas(256, 120, 0, 140, 40), canvas(256, 0, 0, 0, 0), canvas(48, 0, 0, 16, 16)] {
            assert_eq!(without_corner_padding(icon.clone()), icon);
        }
        let rect = RawIcon { w: 256, h: 128, rgba: vec![0; 256 * 128 * 4] };
        assert_eq!(without_corner_padding(rect.clone()), rect);
    }

    #[test]
    fn desktop_ini_is_read_in_utf8_and_utf16() {
        assert!(desktop_ini_declares_icon(b"[.ShellClassInfo]\r\nIconFile=a.ico\r\nIconIndex=0\r\n"));
        assert!(desktop_ini_declares_icon(b"[.ShellClassInfo]\niconresource = x.dll,1"));
        assert!(!desktop_ini_declares_icon(b"[.ShellClassInfo]\r\nInfoTip=Icon\r\n"), "solo cuentan las claves, no el texto");
        let mut u16b = vec![0xFF, 0xFE];
        for u in "[.ShellClassInfo]\r\nIconResource=C:\\a.dll,3\r\n".encode_utf16() {
            u16b.extend_from_slice(&u.to_le_bytes());
        }
        assert!(desktop_ini_declares_icon(&u16b));
    }

    #[test]
    fn keys_are_stable_and_match_what_the_ui_builds() {
        assert_eq!(type_key("docx", false, 64), "t-docx-64");
        assert_eq!(type_key("", false, 32), "t-_-32");
        assert_eq!(type_key("", true, 256), "d-256");
        assert_eq!(bucket(20), 32);
        assert_eq!(bucket(40), 48);
        assert_eq!(bucket(64), 64);
        assert_eq!(bucket(96), 128);
        assert_eq!(bucket(512), 256);
        let a = path_key(Path::new("C:/A/x.exe"), 10, 5, 64);
        assert_eq!(a, path_key(Path::new("c:\\a\\X.EXE"), 10, 5, 64), "sin distinguir mayúsculas ni barras");
        assert_ne!(a, path_key(Path::new("C:/A/x.exe"), 11, 5, 64));
        assert_ne!(a, path_key(Path::new("C:/A/x.exe"), 10, 6, 64));
        assert_ne!(a, path_key(Path::new("C:/A/x.exe"), 10, 5, 128));
    }

    #[test]
    fn extensions_match_the_ui_replica_in_iconKinds_ts() {
        // Mismos casos que `iconService.test.ts` («extensiones como Path::extension»).
        for (name, want) in [("Informe.DOCX", "docx"), ("a.tar.gz", "gz"), (".gitignore", ""), ("LEEME", ""), ("fin.", ""), ("raro.ex e!", "")] {
            assert_eq!(ext_of(Path::new(name)), want, "{name}");
        }
    }

    #[test]
    fn hundreds_of_files_of_one_type_need_a_single_extraction() {
        let d = tmp("shared");
        let f = fake(0);
        let s = svc(&f, &d, 2000);
        let items: Vec<(PathBuf, bool)> = (0..300).map(|i| (PathBuf::from(format!("C:\\x\\f{i}.docx")), false)).chain((0..50).map(|i| (PathBuf::from(format!("C:\\x\\g{i}.zip")), false))).collect();
        let b = s.resolve(&items, 64, &none());
        assert_eq!(f.type_calls.load(Ordering::SeqCst), 2);
        assert_eq!(b.icons.len(), 2);
        assert!(b.keys.iter().all(|k| k.is_some()));
        assert_eq!(b.keys[0], Some("t-docx-64".into()));
        // Segunda pasada: sale de la memoria, sin llamar al proveedor.
        let b2 = s.resolve(&items, 64, &none());
        assert_eq!(f.type_calls.load(Ordering::SeqCst), 2);
        assert_eq!(b2.icons.len(), 2);
        // Si la interfaz ya los tiene, no se reenvían bytes.
        let have: HashSet<String> = ["t-docx-64".to_string(), "t-zip-64".to_string()].into();
        let b3 = s.resolve(&items, 64, &have);
        assert!(b3.icons.is_empty());
        assert!(b3.keys.iter().all(|k| k.is_some()));
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn per_file_icons_are_cached_on_disk_and_invalidated_when_the_file_changes() {
        let d = tmp("perfile");
        let exe = d.join("app.exe");
        fs::write(&exe, b"MZ").unwrap();
        let f = fake(0);
        let s = svc(&f, &d, 2000);
        let items = vec![(exe.clone(), false)];
        let k1 = s.resolve(&items, 64, &none()).keys[0].clone().unwrap();
        assert_eq!(f.file_calls.load(Ordering::SeqCst), 1);
        // Otro servicio (otra sesión) reutiliza el disco.
        let s2 = svc(&f, &d, 2000);
        assert_eq!(s2.resolve(&items, 64, &none()).keys[0].as_deref(), Some(k1.as_str()));
        assert_eq!(f.file_calls.load(Ordering::SeqCst), 1, "segunda sesión: sin extraer otra vez");
        // El archivo cambia → clave nueva → se vuelve a extraer.
        fs::write(&exe, b"MZ-nueva-version-mas-grande").unwrap();
        let k2 = s2.resolve(&items, 64, &none()).keys[0].clone().unwrap();
        assert_ne!(k1, k2);
        assert_eq!(f.file_calls.load(Ordering::SeqCst), 2);
        // Un archivo que no existe no produce clave ni llamada.
        let b = s2.resolve(&[(d.join("fantasma.exe"), false)], 64, &none());
        assert_eq!(b.keys, vec![None]);
        assert_eq!(f.file_calls.load(Ordering::SeqCst), 2);
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn failures_keep_the_ui_glyph_and_are_not_retried_every_time() {
        let d = tmp("fail");
        let f = fake(0);
        let s = svc(&f, &d, 2000);
        let items = vec![(PathBuf::from("C:\\a\\x.nada"), false), (PathBuf::from("C:\\a\\y.txt"), false)];
        let b = s.resolve(&items, 64, &none());
        assert_eq!(b.keys, vec![None, Some("t-txt-64".into())]);
        let calls = f.type_calls.load(Ordering::SeqCst);
        s.resolve(&items, 64, &none());
        assert_eq!(f.type_calls.load(Ordering::SeqCst), calls, "el fallo se recuerda durante la sesión");
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn a_stuck_handler_cannot_block_the_batch() {
        let d = tmp("slow");
        let f = fake(1500);
        let s = svc(&f, &d, 150);
        let t0 = Instant::now();
        let b = s.resolve(&[(PathBuf::from("C:\\a\\x.pdf"), false)], 64, &none());
        assert!(t0.elapsed() < Duration::from_millis(1000), "volvió por el tiempo límite, no por el proveedor");
        assert_eq!(b.keys, vec![None]);
        assert!(b.icons.is_empty());
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn clear_cache_empties_memory_and_disk() {
        let d = tmp("clear");
        let exe = d.join("a.exe");
        fs::write(&exe, b"MZ").unwrap();
        let f = fake(0);
        let s = svc(&f, &d, 2000);
        s.resolve(&[(exe.clone(), false), (PathBuf::from("C:\\q\\a.txt"), false)], 64, &none());
        assert!(s.clear_cache() > 0);
        s.resolve(&[(exe, false), (PathBuf::from("C:\\q\\a.txt"), false)], 64, &none());
        assert_eq!(f.file_calls.load(Ordering::SeqCst), 2);
        assert_eq!(f.type_calls.load(Ordering::SeqCst), 2);
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn png_encoder_downsizes_and_never_upsizes() {
        let big = RawIcon { w: 256, h: 256, rgba: vec![200; 256 * 256 * 4] };
        let small = encode_png(&big, 64).unwrap();
        assert_eq!(&small[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        let tiny = RawIcon { w: 16, h: 16, rgba: vec![200; 16 * 16 * 4] };
        assert!(encode_png(&tiny, 64).is_ok());
        assert!(encode_png(&RawIcon { w: 4, h: 4, rgba: vec![0; 3] }, 64).is_err());
    }
}
