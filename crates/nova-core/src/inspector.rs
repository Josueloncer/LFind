//! Inspector de archivos. Primero lo que da el sistema de archivos; las propiedades de contenido (dimensiones, duración, EXIF, autor…)
//! las aporta el sistema de propiedades de Windows, que solo lee las cabeceras. Un archivo que solo está en la nube se consulta sin abrirlo.
//! El hash no se calcula nunca sin que se pida.

use crate::dir::is_cloud_only;
use crate::viewer::Fact;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, UNIX_EPOCH};

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub title: String,
    pub facts: Vec<Fact>,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Inspection {
    pub name: String,
    pub ext: String,
    pub path: String,
    pub parent: String,
    pub kind: &'static str,
    pub size: u64,
    pub created: Option<i64>,
    pub modified: Option<i64>,
    pub accessed: Option<i64>,
    pub attributes: Vec<String>,
    pub read_only: bool,
    pub cloud: bool,
    pub owner: Option<String>,
    pub link_target: Option<String>,
    /// Carpetas: elementos directos, hasta un tope.
    pub children: Option<u32>,
    pub children_capped: bool,
    pub sections: Vec<Section>,
}

const MAX_CHILDREN: u32 = 50_000;
const PROPERTIES_TIMEOUT: Duration = Duration::from_secs(3);

fn ms(t: std::io::Result<std::time::SystemTime>) -> Option<i64> {
    t.ok().and_then(|x| x.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64)
}

/// Nombre de cada atributo de Windows activo.
pub fn attribute_names(attrs: u32) -> Vec<String> {
    const NAMES: [(u32, &str); 13] = [
        (0x1, "Solo lectura"),
        (0x2, "Oculto"),
        (0x4, "Sistema"),
        (0x20, "Listo para archivar"),
        (0x100, "Temporal"),
        (0x200, "Disperso"),
        (0x400, "Punto de análisis"),
        (0x800, "Comprimido"),
        (0x1000, "Sin conexión"),
        (0x2000, "Sin indexar"),
        (0x4000, "Cifrado"),
        (0x8_0000, "Siempre en este dispositivo"),
        (0x10_0000, "Solo en línea"),
    ];
    NAMES.iter().filter(|(bit, _)| attrs & bit != 0).map(|(_, n)| n.to_string()).collect()
}

#[cfg(windows)]
fn attrs_of(m: &fs::Metadata) -> u32 {
    use std::os::windows::fs::MetadataExt;
    m.file_attributes()
}
#[cfg(not(windows))]
fn attrs_of(_m: &fs::Metadata) -> u32 {
    0
}

/// Windows guarda el códec como un GUID. Se traduce a su nombre; lo desconocido se deja tal cual.
pub fn friendly_codec(raw: &str, audio: bool) -> String {
    let t = raw.trim();
    let code = t.strip_prefix('{').and_then(|s| s.split('-').next()).and_then(|h| u32::from_str_radix(h, 16).ok());
    let Some(code) = code.filter(|_| t.to_uppercase().ends_with("-0000-0010-8000-00AA00389B71}")) else { return raw.to_string() };
    if audio {
        let name = match code {
            0x1 => "PCM",
            0x3 => "PCM (coma flotante)",
            0x50 | 0x55 => "MP3",
            0xFF | 0x1610 | 0x1602 => "AAC",
            0x160 | 0x161 | 0x162 | 0x163 => "WMA",
            0x2000 => "AC-3",
            0x2001 => "DTS",
            0xF1AC => "FLAC",
            0x6C61 => "ALAC",
            0x704F => "Opus",
            0x674F | 0x6750 | 0x6751 => "Vorbis",
            _ => return raw.to_string(),
        };
        return name.to_string();
    }
    let four: String = code.to_le_bytes().iter().map(|b| *b as char).collect();
    let name = match four.to_uppercase().as_str() {
        "H264" | "AVC1" | "X264" => "H.264 (AVC)",
        "H265" | "HEVC" | "HVC1" | "HEV1" => "H.265 (HEVC)",
        "MJPG" => "Motion JPEG",
        "VP80" => "VP8",
        "VP90" => "VP9",
        "AV01" => "AV1",
        "WVC1" => "VC-1",
        "WMV3" => "WMV 9",
        "MP4V" | "MP4S" | "M4S2" => "MPEG-4",
        "MP43" => "MPEG-4 v3",
        "DIVX" | "DX50" => "DivX",
        "XVID" => "Xvid",
        _ if four.chars().all(|c| c.is_ascii_alphanumeric()) => return four,
        _ => return raw.to_string(),
    };
    name.to_string()
}

fn find(data: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    data.get(from..)?.windows(needle.len()).position(|w| w == needle).map(|i| i + from)
}

fn number_after(data: &[u8], at: usize) -> Option<u64> {
    let digits: Vec<u8> = data.get(at..)?.iter().skip_while(|c| c.is_ascii_whitespace()).take_while(|c| c.is_ascii_digit()).copied().collect();
    std::str::from_utf8(&digits).ok()?.parse().ok()
}

/// El último nodo raíz del árbol de páginas que esté a la vista. Un nodo hijo (con /Parent) solo cuenta una parte.
fn pdf_root_count(data: &[u8]) -> Option<u64> {
    let (mut at, mut found) = (0, None);
    while let Some(c) = find(data, b"/Count", at) {
        at = c + 6;
        let start = data[..c].windows(2).rposition(|w| w == b"<<").unwrap_or(0);
        let end = find(data, b">>", c).unwrap_or(data.len());
        let dict = &data[start..end];
        if find(dict, b"/Pages", 0).is_some() && find(dict, b"/Kids", 0).is_some() && find(dict, b"/Parent", 0).is_none() {
            found = number_after(data, c + 6).or(found);
        }
    }
    found
}

/// Páginas de un PDF con lo que se ve sin descomprimir nada. Un PDF editado guarda revisiones: ante la duda, es mejor no dar el dato.
fn pdf_pages(head: &[u8], tail: &[u8], size: u64) -> Option<u64> {
    if let Some(l) = find(head, b"/Linearized", 0) {
        let end = find(head, b">>", l).unwrap_or(head.len());
        let dict = &head[l..end];
        // /L es el tamaño al linearizar: si ya no coincide, el archivo se editó después y /N puede estar desfasado.
        let intact = find(dict, b"/L ", 0).and_then(|i| number_after(dict, i + 2)) == Some(size);
        let mut at = 0;
        while let Some(n) = find(dict, b"/N", at).filter(|_| intact) {
            if dict.get(n + 2).is_some_and(|c| !c.is_ascii_alphabetic()) {
                if let Some(v) = number_after(dict, n + 2) {
                    return Some(v);
                }
            }
            at = n + 2;
        }
    }
    if let Some(n) = pdf_root_count(tail) {
        return Some(n);
    }
    let revisions = tail.windows(5).filter(|w| w == b"%%EOF").count();
    if revisions <= 1 { pdf_root_count(head) } else { None }
}

const PDF_HEAD: usize = 64 * 1024;
const PDF_TAIL: u64 = 256 * 1024;

/// Windows no trae quien lea las propiedades de un PDF. Se mira solo el principio y el final, nunca el archivo entero.
fn pdf_section(path: &Path, size: u64) -> Option<Section> {
    let mut f = File::open(path).ok()?;
    let mut head = vec![0u8; PDF_HEAD.min(size as usize)];
    f.read_exact(&mut head).ok()?;
    if !head.starts_with(b"%PDF-") {
        return None;
    }
    let version: String = head[5..].iter().take(8).take_while(|c| c.is_ascii_digit() || **c == b'.').map(|c| *c as char).collect();
    let back = PDF_TAIL.min(size);
    f.seek(SeekFrom::End(-(back as i64))).ok()?;
    let mut tail = vec![0u8; back as usize];
    f.read_exact(&mut tail).ok()?;
    let mut facts = vec![Fact { label: "Versión de PDF".into(), value: version }];
    if let Some(n) = pdf_pages(&head, &tail, size) {
        facts.push(Fact { label: "Páginas".into(), value: n.to_string() });
    }
    if find(&tail, b"/Encrypt", 0).is_some() {
        facts.push(Fact { label: "Protección".into(), value: "Cifrado".into() });
    }
    Some(Section { title: "Documento".into(), facts })
}

fn image_section(path: &Path, ext: &str) -> Option<Section> {
    if !crate::thumbs::RASTER_EXTS.contains(&ext) {
        return None;
    }
    // Solo la cabecera: no se decodifica la imagen.
    let (w, h) = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?.into_dimensions().ok()?;
    Some(Section { title: "Imagen".into(), facts: vec![Fact { label: "Dimensiones".into(), value: format!("{w} × {h} px") }] })
}

/// Reúne lo que se sabe de `path`. No lee el contenido de archivos en la nube y nunca lee un archivo entero.
pub fn inspect(path: &Path) -> Result<Inspection, String> {
    let meta = fs::symlink_metadata(path).map_err(|e| crate::ops::classify(&e).1)?;
    let attrs = attrs_of(&meta);
    let link = meta.file_type().is_symlink();
    let cloud = is_cloud_only(&meta);
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let is_dir = meta.is_dir() || (link && fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false));
    let mut out = Inspection {
        name: path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned()),
        ext: ext.clone(),
        path: path.display().to_string(),
        parent: path.parent().map(|p| p.display().to_string()).unwrap_or_default(),
        kind: if link { "link" } else if is_dir { "dir" } else { "file" },
        size: if is_dir { 0 } else { meta.len() },
        created: ms(meta.created()),
        modified: ms(meta.modified()),
        accessed: ms(meta.accessed()),
        attributes: attribute_names(attrs),
        read_only: meta.permissions().readonly(),
        cloud,
        owner: platform::owner(path),
        link_target: link.then(|| fs::read_link(path).ok().map(|t| t.display().to_string())).flatten(),
        ..Default::default()
    };
    if is_dir && !link {
        if let Ok(rd) = fs::read_dir(path) {
            let mut n = 0u32;
            for _ in rd {
                n += 1;
                if n >= MAX_CHILDREN {
                    out.children_capped = true;
                    break;
                }
            }
            out.children = Some(n);
        }
        return Ok(out);
    }
    out.sections = platform::properties(path, cloud, PROPERTIES_TIMEOUT);
    if !cloud && !out.sections.iter().any(|s| s.facts.iter().any(|f| f.label == "Dimensiones")) {
        out.sections.extend(image_section(path, &ext));
    }
    if !cloud && ext == "pdf" && !out.sections.iter().any(|s| s.title == "Documento") {
        out.sections.extend(pdf_section(path, meta.len()));
    }
    Ok(out)
}

const HASH_BUF: usize = 256 * 1024;

/// SHA-256 leyendo por bloques, sin cargar el archivo en memoria. Rechaza lo que solo está en la nube: leerlo lo descargaría.
pub fn hash_file(path: &Path, cancel: &AtomicBool, on_progress: &mut dyn FnMut(u64, u64)) -> Result<Option<String>, String> {
    let meta = fs::metadata(path).map_err(|e| crate::ops::classify(&e).1)?;
    if is_cloud_only(&meta) {
        return Err("Este archivo solo está en la nube: calcular su hash obligaría a descargarlo.".into());
    }
    if !meta.is_file() {
        return Err("Solo se puede calcular el hash de un archivo.".into());
    }
    let mut file = File::open(path).map_err(|e| crate::ops::classify(&e).1)?;
    let (mut hasher, mut buf, mut done) = (Sha256::new(), vec![0u8; HASH_BUF], 0u64);
    let mut last = Instant::now();
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let n = file.read(&mut buf).map_err(|e| crate::ops::classify(&e).1)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        done += n as u64;
        if last.elapsed() >= Duration::from_millis(150) {
            last = Instant::now();
            on_progress(done, meta.len());
        }
    }
    Ok(Some(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()))
}

#[cfg(windows)]
mod platform {
    use super::*;
    use crate::viewer::Fact;
    use std::ffi::c_void;
    use std::ptr::null_mut;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Guid(u32, u16, u16, [u8; 8]);
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct PropKey {
        fmt: Guid,
        pid: u32,
    }
    #[repr(C)]
    struct PropVariant {
        vt: u16,
        r: [u16; 3],
        data: [usize; 2],
    }
    #[repr(C)]
    struct StoreVtbl {
        _qi: usize,
        _add: usize,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
        _count: usize,
        _at: usize,
        get_value: unsafe extern "system" fn(*mut c_void, *const PropKey, *mut PropVariant) -> i32,
    }
    #[repr(C)]
    struct Store {
        vtbl: *const StoreVtbl,
    }

    #[link(name = "shell32")]
    extern "system" {
        fn SHGetPropertyStoreFromParsingName(path: *const u16, ctx: *mut c_void, flags: u32, riid: *const Guid, out: *mut *mut c_void) -> i32;
    }
    #[link(name = "propsys")]
    extern "system" {
        fn PSFormatForDisplayAlloc(key: *const PropKey, value: *const PropVariant, flags: u32, out: *mut *mut u16) -> i32;
    }
    #[link(name = "ole32")]
    extern "system" {
        fn CoInitializeEx(r: *mut c_void, c: u32) -> i32;
        fn CoUninitialize();
        fn PropVariantClear(v: *mut PropVariant) -> i32;
        fn CoTaskMemFree(p: *mut c_void);
    }
    #[link(name = "advapi32")]
    extern "system" {
        fn GetNamedSecurityInfoW(name: *const u16, kind: u32, info: u32, owner: *mut *mut c_void, group: *mut *mut c_void, dacl: *mut *mut c_void, sacl: *mut *mut c_void, sd: *mut *mut c_void) -> u32;
        fn LookupAccountSidW(system: *const u16, sid: *mut c_void, name: *mut u16, cch: *mut u32, domain: *mut u16, cch_d: *mut u32, use_: *mut u32) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn LocalFree(h: *mut c_void) -> *mut c_void;
    }

    const IID_PROPERTY_STORE: Guid = Guid(0x886D_8EEB, 0x8CF2, 0x4446, [0x8D, 0x02, 0xCD, 0xBA, 0x1D, 0xBD, 0xCF, 0x99]);
    const GPS_DEFAULT: u32 = 0;
    /// Solo lo que se conoce sin abrir el archivo.
    const GPS_FASTPROPERTIESONLY: u32 = 0x2;
    const COINIT: u32 = 0x2 | 0x4;

    const IMAGE: Guid = Guid(0x6444_048F, 0x4C8B, 0x11D1, [0x8B, 0x70, 0x08, 0x00, 0x36, 0xB1, 0x1A, 0x03]);
    const AUDIO: Guid = Guid(0x6444_0490, 0x4C8B, 0x11D1, [0x8B, 0x70, 0x08, 0x00, 0x36, 0xB1, 0x1A, 0x03]);
    const VIDEO: Guid = Guid(0x6444_0491, 0x4C8B, 0x11D1, [0x8B, 0x70, 0x08, 0x00, 0x36, 0xB1, 0x1A, 0x03]);
    const PHOTO: Guid = Guid(0x14B8_1DA1, 0x0135, 0x4D31, [0x96, 0xD9, 0x6C, 0xBF, 0xC9, 0x67, 0x1A, 0x99]);
    const DOC: Guid = Guid(0xF29F_85E0, 0x4FF9, 0x1068, [0xAB, 0x91, 0x08, 0x00, 0x2B, 0x27, 0xB3, 0xD9]);
    const MUSIC: Guid = Guid(0x56A3_372E, 0xCE9C, 0x11D2, [0x9F, 0x0E, 0x00, 0x60, 0x97, 0xC6, 0x86, 0xF6]);

    const KEYS: &[(&str, &str, Guid, u32)] = &[
        ("Imagen", "Ancho", IMAGE, 3),
        ("Imagen", "Alto", IMAGE, 4),
        ("Imagen", "Resolución horizontal", IMAGE, 5),
        ("Imagen", "Resolución vertical", IMAGE, 6),
        ("Imagen", "Profundidad de color", IMAGE, 7),
        ("Cámara (EXIF)", "Fabricante", PHOTO, 271),
        ("Cámara (EXIF)", "Modelo", PHOTO, 272),
        ("Cámara (EXIF)", "Fecha de la foto", PHOTO, 36867),
        ("Cámara (EXIF)", "Exposición", PHOTO, 33434),
        ("Cámara (EXIF)", "Apertura", PHOTO, 33437),
        ("Cámara (EXIF)", "ISO", PHOTO, 34855),
        ("Cámara (EXIF)", "Distancia focal", PHOTO, 37386),
        ("Multimedia", "Duración", AUDIO, 3),
        ("Multimedia", "Ancho de vídeo", VIDEO, 3),
        ("Multimedia", "Alto de vídeo", VIDEO, 4),
        ("Multimedia", "Fotogramas por segundo", VIDEO, 6),
        ("Multimedia", "Velocidad de vídeo", VIDEO, 8),
        ("Multimedia", "Códec de vídeo", VIDEO, 10),
        ("Multimedia", "Velocidad de audio", AUDIO, 4),
        ("Multimedia", "Frecuencia de audio", AUDIO, 5),
        ("Multimedia", "Canales de audio", AUDIO, 7),
        ("Multimedia", "Códec de audio", AUDIO, 10),
        ("Multimedia", "Artista", MUSIC, 2),
        ("Multimedia", "Álbum", MUSIC, 4),
        ("Contenido", "Título", DOC, 2),
        ("Contenido", "Asunto", DOC, 3),
        ("Contenido", "Autor", DOC, 4),
        ("Documento", "Páginas", DOC, 14),
        ("Documento", "Palabras", DOC, 15),
        ("Documento", "Aplicación", DOC, 18),
    ];

    fn wide(p: &Path) -> Vec<u16> {
        p.to_string_lossy().encode_utf16().chain(Some(0)).collect()
    }

    unsafe fn read_all(path: &Path, flags: u32) -> Vec<Section> {
        let com = CoInitializeEx(null_mut(), COINIT) >= 0;
        let mut out: Vec<Section> = vec![];
        let name = wide(path);
        let mut raw: *mut c_void = null_mut();
        if SHGetPropertyStoreFromParsingName(name.as_ptr(), null_mut(), flags, &IID_PROPERTY_STORE, &mut raw) >= 0 && !raw.is_null() {
            let vt = (*(raw as *mut Store)).vtbl;
            for (section, label, fmt, pid) in KEYS {
                let key = PropKey { fmt: *fmt, pid: *pid };
                let mut value = PropVariant { vt: 0, r: [0; 3], data: [0; 2] };
                if ((*vt).get_value)(raw, &key, &mut value) >= 0 && value.vt != 0 {
                    let mut text: *mut u16 = null_mut();
                    if PSFormatForDisplayAlloc(&key, &value, 0, &mut text) >= 0 && !text.is_null() {
                        let len = (0..).take_while(|i| *text.add(*i) != 0).count();
                        let shown = String::from_utf16_lossy(std::slice::from_raw_parts(text, len)).replace(['\u{200e}', '\u{200f}', '\u{202a}', '\u{202c}'], "");
                        CoTaskMemFree(text as *mut c_void);
                        if !shown.trim().is_empty() {
                            let value = if label.starts_with("Códec") { friendly_codec(shown.trim(), label.ends_with("audio")) } else { shown.trim().to_string() };
                            let fact = Fact { label: (*label).into(), value };
                            match out.iter_mut().find(|s| s.title == *section) {
                                Some(s) => s.facts.push(fact),
                                None => out.push(Section { title: (*section).into(), facts: vec![fact] }),
                            }
                        }
                    }
                    PropVariantClear(&mut value);
                }
            }
            ((*vt).release)(raw);
        }
        if com {
            CoUninitialize();
        }
        out
    }

    /// Con tiempo límite: un manejador de formato defectuoso no debe dejar colgado el inspector.
    pub fn properties(path: &Path, cloud: bool, timeout: Duration) -> Vec<Section> {
        let (tx, rx) = std::sync::mpsc::channel();
        let p = path.to_path_buf();
        let flags = if cloud { GPS_FASTPROPERTIESONLY } else { GPS_DEFAULT };
        let ok = std::thread::Builder::new().name("lfind-inspector".into()).spawn(move || {
            let _ = tx.send(unsafe { read_all(&p, flags) });
        });
        if ok.is_err() {
            return vec![];
        }
        rx.recv_timeout(timeout).unwrap_or_default()
    }

    pub fn owner(path: &Path) -> Option<String> {
        const SE_FILE_OBJECT: u32 = 1;
        const OWNER_SECURITY_INFORMATION: u32 = 1;
        let name = wide(path);
        unsafe {
            let (mut sid, mut sd): (*mut c_void, *mut c_void) = (null_mut(), null_mut());
            if GetNamedSecurityInfoW(name.as_ptr(), SE_FILE_OBJECT, OWNER_SECURITY_INFORMATION, &mut sid, null_mut(), null_mut(), null_mut(), &mut sd) != 0 {
                return None;
            }
            let (mut account, mut domain) = ([0u16; 256], [0u16; 256]);
            let (mut a, mut d, mut use_) = (256u32, 256u32, 0u32);
            let found = LookupAccountSidW(std::ptr::null(), sid, account.as_mut_ptr(), &mut a, domain.as_mut_ptr(), &mut d, &mut use_) != 0;
            LocalFree(sd);
            found.then(|| {
                let acc = String::from_utf16_lossy(&account[..a as usize]);
                let dom = String::from_utf16_lossy(&domain[..d as usize]);
                if dom.is_empty() { acc } else { format!("{dom}\\{acc}") }
            })
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::*;
    pub fn properties(_p: &Path, _cloud: bool, _t: Duration) -> Vec<Section> {
        vec![]
    }
    pub fn owner(_p: &Path) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicU32;

    static N: AtomicU32 = AtomicU32::new(0);
    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("lfind-insp-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir_all(&d).unwrap();
        d
    }
    #[cfg(windows)]
    fn attrib(flag: &str, p: &Path) {
        assert!(std::process::Command::new("attrib").arg(flag).arg(p).status().unwrap().success());
    }

    #[test]
    fn hash_matches_a_known_value() {
        let d = tmp();
        let f = d.join("abc.txt");
        fs::write(&f, b"abc").unwrap();
        let h = hash_file(&f, &AtomicBool::new(false), &mut |_, _| {}).unwrap().unwrap();
        assert_eq!(h, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn hash_can_be_cancelled_and_reads_large_files_by_blocks() {
        let d = tmp();
        let f = d.join("g.bin");
        fs::write(&f, vec![7u8; HASH_BUF * 3 + 11]).unwrap();
        assert_eq!(hash_file(&f, &AtomicBool::new(true), &mut |_, _| {}).unwrap(), None);
        assert_eq!(hash_file(&f, &AtomicBool::new(false), &mut |_, _| {}).unwrap().unwrap().len(), 64);
        fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn folders_count_their_items_without_a_size() {
        let d = tmp();
        fs::write(d.join("a"), b"1").unwrap();
        fs::create_dir(d.join("b")).unwrap();
        let i = inspect(&d).unwrap();
        assert_eq!((i.kind, i.children, i.size, i.children_capped), ("dir", Some(2), 0, false));
        fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn reports_basic_data_and_image_dimensions() {
        let d = tmp();
        let f = d.join("foto.png");
        image::RgbImage::new(40, 20).save(&f).unwrap();
        let i = inspect(&f).unwrap();
        assert_eq!((i.name.as_str(), i.ext.as_str(), i.kind), ("foto.png", "png", "file"));
        assert_eq!(i.size, fs::metadata(&f).unwrap().len());
        assert!(i.created.is_some() && i.modified.is_some());
        let shown = serde_json::to_string(&i.sections).unwrap();
        assert!(shown.contains("40"), "{shown}");
        fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn codec_guids_become_names() {
        assert_eq!(friendly_codec("{34363248-0000-0010-8000-00AA00389B71}", false), "H.264 (AVC)");
        assert_eq!(friendly_codec("{47504A4D-0000-0010-8000-00AA00389B71}", false), "Motion JPEG");
        assert_eq!(friendly_codec("{31435641-0000-0010-8000-00AA00389B71}", false), "H.264 (AVC)");
        assert_eq!(friendly_codec("{00000055-0000-0010-8000-00AA00389B71}", true), "MP3");
        assert_eq!(friendly_codec("{00001610-0000-0010-8000-00AA00389B71}", true), "AAC");
        assert_eq!(friendly_codec("{DEADBEEF-1111-2222-3333-444455556666}", false), "{DEADBEEF-1111-2222-3333-444455556666}");
        assert_eq!(friendly_codec("Texto normal", true), "Texto normal");
    }

    #[test]
    fn pdf_pages_from_linearized_header_or_page_tree() {
        assert_eq!(pdf_pages(b"%PDF-1.6 1 0 obj << /Linearized 1 /L 99 /H [1 2] /O 3 /E 4 /N 27 /T 5 >>", b"", 99), Some(27));
        assert_eq!(pdf_pages(b"%PDF-1.6 1 0 obj << /Linearized 1 /L 99 /N 27 >>", b"", 150), None, "se editó después de linearizar");
        assert_eq!(pdf_pages(b"<< /Type /Pages /Kids [1 0 R] /Count 7 >>", b"%%EOF x %%EOF", 1000), None, "varias revisiones: la cabecera puede ser antigua");
        assert_eq!(pdf_pages(b"", b"<< /Type /Pages /Kids [1 0 R] /Count 3 >> %%EOF << /Type /Pages /Kids [1 0 R] /Count 8 >>", 1000), Some(8), "gana la última revisión");
        assert_eq!(pdf_pages(b"%PDF-1.4", b"3 0 obj << /Type /Pages /Kids [4 0 R] /Count 12 >> endobj", 0), Some(12));
        assert_eq!(pdf_pages(b"%PDF-1.4", b"<< /Type /Pages /Kids [1 0 R] /Count 3 /Parent 2 0 R >> << /Type /Pages /Kids [4 0 R] /Count 9 >>", 0), Some(9));
        assert_eq!(pdf_pages(b"%PDF-1.7", b"<< /Type /Catalog /Count 44 >>", 0), None, "un /Count ajeno no son páginas");
        assert_eq!(pdf_pages(b"%PDF-1.4", b"<< /Type /Pages /Kids [1 0 R] /Count 196 /Parent 2 0 R >>", 0), None, "un nodo hijo no es el total");
    }

    #[test]
    fn a_pdf_is_read_only_at_its_edges() {
        let d = tmp();
        let f = d.join("doc.pdf");
        let mut body = b"%PDF-1.5\n".to_vec();
        body.extend(vec![b' '; 2_000_000]);
        body.extend(b"<< /Type /Pages /Kids [1 0 R] /Count 5 >>");
        fs::write(&f, &body).unwrap();
        let i = inspect(&f).unwrap();
        let s = serde_json::to_string(&i.sections).unwrap();
        assert!(s.contains("Versión de PDF") && s.contains("1.5") && s.contains("\"5\""), "{s}");
        fs::remove_dir_all(d).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn a_cloud_only_pdf_is_never_opened() {
        let d = tmp();
        let f = d.join("nube.pdf");
        fs::write(&f, b"%PDF-1.5 << /Type /Pages /Kids [1 0 R] /Count 5 >>").unwrap();
        attrib("+O", &f);
        assert!(inspect(&f).unwrap().sections.iter().all(|s| !s.facts.iter().any(|x| x.label == "Páginas")));
        attrib("-O", &f);
        fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn a_missing_file_is_an_error() {
        assert!(inspect(Path::new("Z:\\no\\existe.txt")).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn names_windows_attributes_and_its_owner() {
        let d = tmp();
        let f = d.join("a.txt");
        fs::write(&f, b"x").unwrap();
        attrib("+R", &f);
        attrib("+H", &f);
        let i = inspect(&f).unwrap();
        assert!(i.attributes.iter().any(|a| a == "Oculto") && i.read_only);
        assert!(i.owner.as_deref().is_some_and(|o| !o.is_empty()));
        attrib("-H", &f);
        attrib("-R", &f);
        fs::remove_dir_all(d).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn cloud_only_files_are_described_but_never_read() {
        let d = tmp();
        let f = d.join("nube.png");
        image::RgbImage::new(40, 20).save(&f).unwrap();
        attrib("+O", &f);
        let i = inspect(&f).unwrap();
        assert!(i.cloud);
        let e = hash_file(&f, &AtomicBool::new(false), &mut |_, _| {}).unwrap_err();
        assert!(e.contains("nube"));
        attrib("-O", &f);
        fs::remove_dir_all(d).unwrap();
    }
}
