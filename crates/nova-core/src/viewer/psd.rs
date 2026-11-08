//! Lectura de la composición de archivos PSD/PSB sin Photoshop. Rust seguro, sin bibliotecas nativas.
//!
//! Solo se leen la cabecera, los recursos necesarios y la sección «image data» (la composición aplanada que guarda Photoshop);
//! las capas se saltan con un `seek`. La composición se reduce mientras se lee: la memoria depende del tamaño de salida, no del
//! archivo, y los canales se recorren en orden para no saltar de un lado a otro en discos mecánicos.
//!
//! Si el documento se guardó sin «Maximizar compatibilidad», la composición no es fiable: se usa la miniatura incorporada.

use image::{DynamicImage, RgbaImage};
use std::io::{self, BufReader, Read, Seek, SeekFrom};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PsdError {
    Invalid(String),
    Unsupported(String),
    Cancelled,
}

impl From<io::Error> for PsdError {
    fn from(e: io::Error) -> Self {
        PsdError::Invalid(format!("El archivo está incompleto o no se pudo leer: {e}"))
    }
}

type R<T> = Result<T, PsdError>;

const MAX_DIMENSION: u32 = 300_000;
const MAX_THUMB_BYTES: u64 = 16 << 20;
const MAX_TABLE_BYTES: u64 = 256 << 20;

#[derive(Debug, Clone, PartialEq)]
pub struct PsdInfo {
    pub psb: bool,
    pub channels: u16,
    pub width: u32,
    pub height: u32,
    pub depth: u16,
    pub mode: u16,
    pub dpi: Option<f32>,
    /// `false` si se guardó sin «Maximizar compatibilidad».
    pub has_merged: bool,
    /// El primer canal extra es la transparencia de la composición.
    pub transparency: bool,
    thumb: Option<(u64, u64)>,
    image_data: u64,
    palette: Vec<u8>,
}

impl PsdInfo {
    pub fn mode_name(&self) -> &'static str {
        match self.mode {
            0 => "Mapa de bits",
            1 => "Escala de grises",
            2 => "Color indexado",
            3 => "RGB",
            4 => "CMYK",
            7 => "Multicanal",
            8 => "Duotono",
            9 => "Lab",
            _ => "Desconocido",
        }
    }
}

fn be16<T: Read>(r: &mut T) -> R<u16> {
    let mut b = [0; 2];
    r.read_exact(&mut b)?;
    Ok(u16::from_be_bytes(b))
}
fn be32<T: Read>(r: &mut T) -> R<u32> {
    let mut b = [0; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_be_bytes(b))
}
fn be64<T: Read>(r: &mut T) -> R<u64> {
    let mut b = [0; 8];
    r.read_exact(&mut b)?;
    Ok(u64::from_be_bytes(b))
}
fn skip<T: Seek>(r: &mut T, n: u64) -> R<()> {
    r.seek(SeekFrom::Current(i64::try_from(n).map_err(|_| PsdError::Invalid("Longitud de sección imposible.".into()))?))?;
    Ok(())
}

/// Lee la estructura sin tocar las capas ni los píxeles.
pub fn read_info<T: Read + Seek>(r: &mut T) -> R<PsdInfo> {
    let mut sig = [0; 4];
    r.read_exact(&mut sig)?;
    if &sig != b"8BPS" {
        return Err(PsdError::Invalid("No es un archivo de Photoshop válido.".into()));
    }
    let version = be16(r)?;
    if version != 1 && version != 2 {
        return Err(PsdError::Unsupported(format!("Versión de PSD desconocida ({version}).")));
    }
    let psb = version == 2;
    skip(r, 6)?;
    let channels = be16(r)?;
    let height = be32(r)?;
    let width = be32(r)?;
    let depth = be16(r)?;
    let mode = be16(r)?;
    if channels == 0 || channels > 56 || width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION || !matches!(depth, 1 | 8 | 16 | 32) {
        return Err(PsdError::Invalid("La cabecera del PSD no es válida.".into()));
    }

    let color_len = be32(r)? as u64;
    let mut palette = vec![];
    if mode == 2 && color_len == 768 {
        palette = vec![0; 768];
        r.read_exact(&mut palette)?;
    } else {
        skip(r, color_len)?;
    }

    let res_len = be32(r)? as u64;
    let res_start = r.stream_position()?;
    let res_end = res_start.checked_add(res_len).ok_or_else(|| PsdError::Invalid("Sección de recursos inválida.".into()))?;
    let (mut dpi, mut has_merged, mut thumb) = (None, true, None);
    while r.stream_position()? + 12 <= res_end {
        let mut tag = [0; 4];
        r.read_exact(&mut tag)?;
        if &tag != b"8BIM" {
            break;
        }
        let id = be16(r)?;
        let mut nl = [0; 1];
        r.read_exact(&mut nl)?;
        let pad = (nl[0] as u64 + 1) % 2; // el nombre (longitud + texto) ocupa un número par de bytes
        skip(r, nl[0] as u64 + pad)?;
        let size = be32(r)? as u64;
        let at = r.stream_position()?;
        match id {
            1005 if size >= 16 => dpi = Some(be32(r)? as f32 / 65536.0).filter(|d| *d > 0.0 && *d < 100_000.0),
            1057 if size >= 5 => {
                let _ver = be32(r)?;
                let mut b = [0; 1];
                r.read_exact(&mut b)?;
                has_merged = b[0] != 0;
            }
            1036 | 1033 if size > 28 && size <= MAX_THUMB_BYTES && (thumb.is_none() || id == 1036) => thumb = Some((at + 28, size - 28)),
            _ => {}
        }
        r.seek(SeekFrom::Start(at + size + size % 2))?;
    }
    r.seek(SeekFrom::Start(res_end))?;

    let layer_len = if psb { be64(r)? } else { be32(r)? as u64 };
    let layer_start = r.stream_position()?;
    let mut transparency = false;
    if layer_len >= 6 {
        let info_len = if psb { be64(r)? } else { be32(r)? as u64 };
        if info_len >= 2 {
            transparency = (be16(r)? as i16) < 0;
        }
    }
    let image_data = layer_start.checked_add(layer_len).ok_or_else(|| PsdError::Invalid("Sección de capas inválida.".into()))?;
    Ok(PsdInfo { psb, channels, width, height, depth, mode, dpi, has_merged, transparency, thumb, image_data, palette })
}

/// Miniatura JPEG que Photoshop incorpora al guardar. Suele medir unos 160 px.
pub fn read_thumbnail<T: Read + Seek>(r: &mut T, info: &PsdInfo) -> Option<DynamicImage> {
    let (at, len) = info.thumb?;
    r.seek(SeekFrom::Start(at)).ok()?;
    let mut b = vec![0; len as usize];
    r.read_exact(&mut b).ok()?;
    image::load_from_memory_with_format(&b, image::ImageFormat::Jpeg).ok()
}

fn unpack_bits(src: &[u8], dst: &mut [u8]) -> R<()> {
    let (mut i, mut o) = (0, 0);
    let bad = || PsdError::Invalid("Datos comprimidos dañados.".into());
    while o < dst.len() {
        let h = *src.get(i).ok_or_else(bad)? as i8;
        i += 1;
        if h >= 0 {
            let n = h as usize + 1;
            let s = src.get(i..i + n).ok_or_else(bad)?;
            dst.get_mut(o..o + n).ok_or_else(bad)?.copy_from_slice(s);
            i += n;
            o += n;
        } else if h != -128 {
            let n = (1 - h as isize) as usize;
            let v = *src.get(i).ok_or_else(bad)?;
            i += 1;
            dst.get_mut(o..o + n).ok_or_else(bad)?.fill(v);
            o += n;
        }
    }
    Ok(())
}

fn row_bytes(info: &PsdInfo) -> usize {
    match info.depth {
        1 => (info.width as usize).div_ceil(8),
        d => info.width as usize * d as usize / 8,
    }
}

/// Convierte una fila cruda a 8 bits por muestra.
fn to_8bit(info: &PsdInfo, raw: &[u8], out: &mut [u8]) {
    match info.depth {
        8 => out.copy_from_slice(raw),
        16 => out.iter_mut().zip(raw.chunks_exact(2)).for_each(|(o, c)| *o = c[0]),
        32 => out.iter_mut().zip(raw.chunks_exact(4)).for_each(|(o, c)| {
            let v = f32::from_be_bytes([c[0], c[1], c[2], c[3]]).clamp(0.0, 1.0);
            *o = (v.powf(1.0 / 2.2) * 255.0 + 0.5) as u8;
        }),
        _ => out.iter_mut().enumerate().for_each(|(i, o)| *o = if raw[i / 8] & (0x80 >> (i % 8)) != 0 { 0 } else { 255 }),
    }
}

fn shrink_row(src: &[u8], dst: &mut [u8], nearest: bool) {
    let (w, tw) = (src.len(), dst.len());
    if w == tw {
        dst.copy_from_slice(src);
        return;
    }
    for (x, d) in dst.iter_mut().enumerate() {
        let (a, b) = (x * w / tw, ((x + 1) * w / tw).max(x * w / tw + 1).min(w));
        *d = if nearest { src[(a + b) / 2 - usize::from((a + b) / 2 == w)] } else { (src[a..b].iter().map(|v| *v as u32).sum::<u32>() / (b - a) as u32) as u8 };
    }
}

fn lab_to_rgb(l: u8, a: u8, b: u8) -> [u8; 3] {
    let fy = (l as f32 * 100.0 / 255.0 + 16.0) / 116.0;
    let fx = fy + (a as f32 - 128.0) / 500.0;
    let fz = fy - (b as f32 - 128.0) / 200.0;
    let inv = |t: f32| if t > 0.206_893 { t * t * t } else { (t - 16.0 / 116.0) / 7.787 };
    let (x, y, z) = (0.9505 * inv(fx), inv(fy), 1.089 * inv(fz));
    let gamma = |c: f32| {
        let c = c.clamp(0.0, 1.0);
        ((if c <= 0.003_130_8 { 12.92 * c } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }) * 255.0 + 0.5) as u8
    };
    [gamma(3.2406 * x - 1.5372 * y - 0.4986 * z), gamma(-0.9689 * x + 1.8758 * y + 0.0415 * z), gamma(0.0557 * x - 0.204 * y + 1.057 * z)]
}

/// Lee la composición y la reduce para que su lado mayor no pase de `max_side`.
pub fn render_merged<T: Read + Seek>(raw: T, info: &PsdInfo, max_side: u32, cancelled: &dyn Fn() -> bool) -> R<RgbaImage> {
    let mut r = BufReader::with_capacity(1 << 18, raw);
    let color_channels: usize = match info.mode {
        3 | 9 => 3,
        4 => 4,
        0 | 1 | 2 | 7 | 8 => 1,
        m => return Err(PsdError::Unsupported(format!("Modo de color no admitido ({m})."))),
    };
    if (info.channels as usize) < color_channels {
        return Err(PsdError::Invalid("Faltan canales de color en el PSD.".into()));
    }
    let alpha = info.transparency && info.channels as usize > color_channels;
    let used = color_channels + usize::from(alpha);
    let (w, h) = (info.width as usize, info.height as usize);
    let shrink = (info.width.max(info.height) as f64 / max_side.max(1) as f64).max(1.0);
    let tw = ((w as f64 / shrink).round() as usize).max(1);
    let th = ((h as f64 / shrink).round() as usize).max(1);
    let nearest = info.mode == 2;

    // Filas de origen por fila de salida: dos muestras dan buena calidad sin leer todo el archivo.
    let mut samples: Vec<(usize, usize, bool)> = Vec::with_capacity(th * 2);
    for y in 0..th {
        let first = ((y as f64 + 0.25) * h as f64 / th as f64) as usize;
        let second = ((y as f64 + 0.75) * h as f64 / th as f64) as usize;
        samples.push((first.min(h - 1), y, false));
        if !nearest && second != first && second < h {
            samples.push((second, y, true));
        }
    }

    r.seek(SeekFrom::Start(info.image_data))?;
    let compression = be16(&mut r)?;
    let rb = row_bytes(info);
    if rb == 0 {
        return Err(PsdError::Invalid("Dimensiones inválidas.".into()));
    }
    let base = info.image_data + 2;

    // Offsets y longitudes de cada fila usada, por canal.
    let mut located: Vec<Vec<(u64, u32)>> = vec![Vec::with_capacity(samples.len()); used];
    match compression {
        0 => {
            for (c, list) in located.iter_mut().enumerate() {
                list.extend(samples.iter().map(|s| (base + (c * h * rb + s.0 * rb) as u64, rb as u32)));
            }
        }
        1 => {
            let entry = if info.psb { 4u64 } else { 2 };
            let table = (h as u64).checked_mul(info.channels as u64).and_then(|n| n.checked_mul(entry)).filter(|n| *n <= MAX_TABLE_BYTES).ok_or_else(|| PsdError::Invalid("Tabla de filas imposible.".into()))?;
            let mut offset = base + table;
            for list in located.iter_mut() {
                let mut next = 0;
                for row in 0..h {
                    let len = if info.psb { be32(&mut r)? } else { be16(&mut r)? as u32 };
                    while next < samples.len() && samples[next].0 == row {
                        list.push((offset, len));
                        next += 1;
                    }
                    offset += len as u64;
                }
            }
        }
        c => return Err(PsdError::Unsupported(format!("Compresión de la composición no admitida ({c})."))),
    }
    let limit = rb + rb / 8 + 16;

    let mut planes: Vec<Vec<u8>> = Vec::with_capacity(used);
    let samples_per_row = if info.depth == 1 { w } else { rb / (info.depth as usize / 8) };
    let (mut packed, mut unpacked, mut row8, mut small) = (vec![0u8; limit], vec![0u8; rb], vec![0u8; samples_per_row], vec![0u8; tw]);
    for list in &located {
        let mut plane = vec![0u8; tw * th];
        for (i, &(at, len)) in list.iter().enumerate() {
            if i % 32 == 0 && cancelled() {
                return Err(PsdError::Cancelled);
            }
            let here = r.stream_position()?;
            if at >= here && at - here < (1 << 18) {
                r.seek_relative((at - here) as i64)?;
            } else {
                r.seek(SeekFrom::Start(at))?;
            }
            if compression == 0 {
                r.read_exact(&mut unpacked)?;
            } else {
                let len = len as usize;
                if len > limit {
                    return Err(PsdError::Invalid("Fila comprimida con longitud imposible.".into()));
                }
                r.read_exact(&mut packed[..len])?;
                unpack_bits(&packed[..len], &mut unpacked)?;
            }
            to_8bit(info, &unpacked, &mut row8);
            shrink_row(&row8, &mut small, nearest);
            let (_, y, second) = samples[i];
            let dst = &mut plane[y * tw..(y + 1) * tw];
            if second {
                dst.iter_mut().zip(&small).for_each(|(d, s)| *d = ((*d as u16 + *s as u16 + 1) / 2) as u8);
            } else {
                dst.copy_from_slice(&small);
            }
        }
        planes.push(plane);
    }

    let mut out = Vec::with_capacity(tw * th * 4);
    for i in 0..tw * th {
        let a = if alpha { planes[color_channels][i] } else { 255 };
        // Con transparencia, Photoshop guarda el color mezclado sobre blanco: se deshace la mezcla.
        let unmatte = |v: u8| if alpha && a > 0 && a < 255 { (((v as i32 - (255 - a as i32)) * 255 / a as i32).clamp(0, 255)) as u8 } else { v };
        let rgb = match info.mode {
            3 => [unmatte(planes[0][i]), unmatte(planes[1][i]), unmatte(planes[2][i])],
            4 => {
                // En CMYK los valores están invertidos: 255 es sin tinta.
                let k = planes[3][i] as u32;
                [(planes[0][i] as u32 * k / 255) as u8, (planes[1][i] as u32 * k / 255) as u8, (planes[2][i] as u32 * k / 255) as u8]
            }
            9 => lab_to_rgb(planes[0][i], planes[1][i], planes[2][i]),
            2 if info.palette.len() == 768 => {
                let v = planes[0][i] as usize;
                [info.palette[v], info.palette[256 + v], info.palette[512 + v]]
            }
            _ => [unmatte(planes[0][i]); 3],
        };
        out.extend_from_slice(&[rgb[0], rgb[1], rgb[2], a]);
    }
    RgbaImage::from_raw(tw as u32, th as u32, out).ok_or_else(|| PsdError::Invalid("Tamaño de imagen inconsistente.".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn no() -> bool {
        false
    }

    fn resource(id: u16, data: &[u8]) -> Vec<u8> {
        let mut v = b"8BIM".to_vec();
        v.extend(id.to_be_bytes());
        v.extend([0, 0]);
        v.extend((data.len() as u32).to_be_bytes());
        v.extend(data);
        if data.len() % 2 == 1 {
            v.push(0);
        }
        v
    }

    /// PSD mínimo: `planes` por canal, sin comprimir o con PackBits literal.
    fn psd(w: u32, h: u32, mode: u16, planes: &[Vec<u8>], rle: bool, resources: &[u8], layer_count: Option<i16>) -> Vec<u8> {
        let mut v = b"8BPS".to_vec();
        v.extend(1u16.to_be_bytes());
        v.extend([0; 6]);
        v.extend((planes.len() as u16).to_be_bytes());
        v.extend(h.to_be_bytes());
        v.extend(w.to_be_bytes());
        v.extend(8u16.to_be_bytes());
        v.extend(mode.to_be_bytes());
        v.extend(0u32.to_be_bytes());
        v.extend((resources.len() as u32).to_be_bytes());
        v.extend(resources);
        match layer_count {
            Some(c) => {
                v.extend(6u32.to_be_bytes());
                v.extend(2u32.to_be_bytes());
                v.extend(c.to_be_bytes());
            }
            None => v.extend(0u32.to_be_bytes()),
        }
        v.extend(u16::from(rle).to_be_bytes());
        if rle {
            // Cada fila: un literal (cabecera w-1 + w bytes).
            for _ in 0..planes.len() * h as usize {
                v.extend((w as u16 + 1).to_be_bytes());
            }
            for p in planes {
                for row in p.chunks(w as usize) {
                    v.push((w - 1) as u8);
                    v.extend(row);
                }
            }
        } else {
            planes.iter().for_each(|p| v.extend(p));
        }
        v
    }

    fn solid(w: u32, h: u32, v: u8) -> Vec<u8> {
        vec![v; (w * h) as usize]
    }

    fn render(bytes: Vec<u8>, side: u32) -> RgbaImage {
        let mut c = Cursor::new(bytes);
        let i = read_info(&mut c).unwrap();
        render_merged(c, &i, side, &no).unwrap()
    }

    #[test]
    fn reads_header_and_resources() {
        let mut res = resource(1005, &[0, 44, 0, 0, 0, 1, 0, 2, 0, 44, 0, 0, 0, 1, 0, 2]);
        res.extend(resource(1057, &[0, 0, 0, 1, 0]));
        let mut c = Cursor::new(psd(4, 2, 3, &[solid(4, 2, 1), solid(4, 2, 2), solid(4, 2, 3)], false, &res, None));
        let i = read_info(&mut c).unwrap();
        assert_eq!((i.width, i.height, i.channels, i.depth, i.mode), (4, 2, 3, 8, 3));
        assert_eq!(i.dpi, Some(44.0));
        assert!(!i.has_merged, "sin «Maximizar compatibilidad»");
        assert_eq!(i.mode_name(), "RGB");
    }

    #[test]
    fn renders_rgb_raw_and_rle_the_same() {
        let planes = [solid(8, 8, 200), solid(8, 8, 100), solid(8, 8, 50)];
        for rle in [false, true] {
            let img = render(psd(8, 8, 3, &planes, rle, &[], None), 100);
            assert_eq!(img.dimensions(), (8, 8));
            assert_eq!(img.get_pixel(3, 3).0, [200, 100, 50, 255], "rle={rle}");
        }
    }

    #[test]
    fn large_documents_are_reduced_while_reading() {
        let (w, h) = (4000, 2000);
        let mut r = solid(w, h, 0);
        r.iter_mut().enumerate().for_each(|(i, v)| *v = if (i as u32 % w) < w / 2 { 10 } else { 240 });
        let img = render(psd(w, h, 3, &[r.clone(), r.clone(), r], false, &[], None), 400);
        assert_eq!(img.dimensions(), (400, 200));
        assert_eq!(img.get_pixel(50, 100).0[0], 10);
        assert_eq!(img.get_pixel(350, 100).0[0], 240);
    }

    #[test]
    fn cmyk_gray_and_transparency() {
        // CMYK invertido: 255 = sin tinta → blanco; K a 0 → negro.
        let white = render(psd(2, 2, 4, &[solid(2, 2, 255), solid(2, 2, 255), solid(2, 2, 255), solid(2, 2, 255)], false, &[], None), 10);
        assert_eq!(white.get_pixel(0, 0).0, [255, 255, 255, 255]);
        let black = render(psd(2, 2, 4, &[solid(2, 2, 255), solid(2, 2, 255), solid(2, 2, 255), solid(2, 2, 0)], false, &[], None), 10);
        assert_eq!(black.get_pixel(0, 0).0, [0, 0, 0, 255]);
        let gray = render(psd(2, 2, 1, &[solid(2, 2, 77)], false, &[], None), 10);
        assert_eq!(gray.get_pixel(1, 1).0, [77, 77, 77, 255]);
        // Recuento de capas negativo: el canal extra es la transparencia de la composición.
        let t = render(psd(2, 2, 3, &[solid(2, 2, 255), solid(2, 2, 128), solid(2, 2, 128), solid(2, 2, 128)], false, &[], Some(-1)), 10);
        assert_eq!(t.get_pixel(0, 0).0[3], 128);
        // Sin ese indicador, el canal extra es una máscara del usuario y no afecta a la imagen.
        let m = render(psd(2, 2, 3, &[solid(2, 2, 9), solid(2, 2, 9), solid(2, 2, 9), solid(2, 2, 0)], false, &[], Some(1)), 10);
        assert_eq!(m.get_pixel(0, 0).0, [9, 9, 9, 255]);
    }

    #[test]
    fn corrupt_files_are_errors_never_panics() {
        assert!(read_info(&mut Cursor::new(b"nada".to_vec())).is_err());
        let good = psd(16, 16, 3, &[solid(16, 16, 1), solid(16, 16, 2), solid(16, 16, 3)], true, &[], None);
        for cut in (0..good.len()).step_by(7) {
            let mut c = Cursor::new(good[..cut].to_vec());
            if let Ok(i) = read_info(&mut c) {
                assert!(render_merged(c, &i, 8, &no).is_err(), "truncado en {cut}");
            }
        }
        // Bytes alterados: puede salir mal la imagen, pero nunca debe entrar en pánico.
        for seed in 0..good.len() {
            let mut b = good.clone();
            b[seed] ^= 0xA5;
            let mut c = Cursor::new(b);
            if let Ok(i) = read_info(&mut c) {
                if i.width.max(i.height) < 4096 {
                    let _ = render_merged(c, &i, 8, &no);
                }
            }
        }
    }

    #[test]
    fn cancellation_stops_the_work() {
        let bytes = psd(64, 64, 3, &[solid(64, 64, 1), solid(64, 64, 2), solid(64, 64, 3)], false, &[], None);
        let mut c = Cursor::new(bytes);
        let i = read_info(&mut c).unwrap();
        assert_eq!(render_merged(c, &i, 32, &|| true), Err(PsdError::Cancelled));
    }

    #[test]
    fn unpack_bits_handles_runs_and_rejects_overflow() {
        let mut d = [0; 5];
        unpack_bits(&[0xFE, 7, 1, 1, 2], &mut d).unwrap();
        assert_eq!(d, [7, 7, 7, 1, 2]);
        assert!(unpack_bits(&[0xFE], &mut d).is_err());
        assert!(unpack_bits(&[0x7F, 1], &mut d).is_err());
    }
}
