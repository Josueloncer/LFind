//! Previsualizaciones que los propios archivos llevan dentro: EPS (TIFF binario o EPSI) y RAW de cámara (JPEG incrustado).
//! Nunca se interpreta PostScript ni se revelan los datos del sensor: solo se localiza y se decodifica la imagen ya incluida.

use image::{DynamicImage, GrayImage};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

const MAX_EMBEDDED: u64 = 128 << 20;
const MAX_IFDS: usize = 32;
const MAX_ENTRIES: u16 = 4096;

fn read_at(f: &mut File, at: u64, len: u64) -> io::Result<Vec<u8>> {
    if len > MAX_EMBEDDED {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Vista previa incrustada demasiado grande."));
    }
    f.seek(SeekFrom::Start(at))?;
    let mut b = vec![0; len as usize];
    f.read_exact(&mut b)?;
    Ok(b)
}

// ───────── EPS ─────────

pub const DOS_EPS: [u8; 4] = [0xC5, 0xD0, 0xD3, 0xC6];

/// Busca una previsualización rasterizada. `None`: el archivo no incluye ninguna utilizable.
pub fn eps_preview(path: &Path) -> io::Result<Option<DynamicImage>> {
    let mut f = File::open(path)?;
    let mut head = [0u8; 30];
    let n = f.read(&mut head)?;
    if n >= 28 && head[..4] == DOS_EPS {
        let le = |i: usize| u32::from_le_bytes([head[i], head[i + 1], head[i + 2], head[i + 3]]) as u64;
        let (tiff_at, tiff_len) = (le(20), le(24));
        if tiff_at > 0 && tiff_len > 0 {
            let b = read_at(&mut f, tiff_at, tiff_len)?;
            if let Some(img) = image::load_from_memory_with_format(&b, image::ImageFormat::Tiff).ok().or_else(|| palette_tiff(&b)) {
                return Ok(Some(img));
            }
        }
        let (ps_at, ps_len) = (le(4), le(8));
        let b = read_at(&mut f, ps_at, ps_len.min(32 << 20))?;
        return Ok(parse_epsi(&b).or_else(|| xmp_thumbnail(&b)));
    }
    f.seek(SeekFrom::Start(0))?;
    let mut b = vec![];
    f.take(32 << 20).read_to_end(&mut b)?;
    Ok(parse_epsi(&b).or_else(|| xmp_thumbnail(&b)))
}

/// TIFF de paleta de 8 bits sin comprimir: lo que escribe Illustrator como previsualización de EPS y la crate `image` no admite.
pub fn palette_tiff(b: &[u8]) -> Option<DynamicImage> {
    let t = match b.get(..2)? {
        b"II" => Tiff { le: true },
        b"MM" => Tiff { le: false },
        _ => return None,
    };
    let ifd = t.u32(b.get(4..8)?) as usize;
    let count = t.u16(b.get(ifd..ifd + 2)?) as usize;
    let (mut w, mut h, mut compression, mut photometric, mut bits, mut spp, mut rows) = (0u32, 0u32, 1u32, 0u32, 0u32, 1usize, 0u32);
    let (mut strips, mut strip_kind, mut strip_n, mut map) = (0usize, 3u16, 0usize, 0usize);
    for i in 0..count {
        let e = b.get(ifd + 2 + i * 12..ifd + 14 + i * 12)?;
        let (tag, kind, n) = (t.u16(&e[0..]), t.u16(&e[2..]), t.u32(&e[4..]) as usize);
        let v = if kind == 3 { t.u16(&e[8..]) as u32 } else { t.u32(&e[8..]) };
        match tag {
            256 => w = v,
            257 => h = v,
            258 => bits = v,
            259 => compression = v,
            262 => photometric = v,
            273 => (strips, strip_kind, strip_n) = (v as usize, kind, n),
            277 => spp = v as usize,
            278 => rows = v,
            320 => map = v as usize,
            _ => {}
        }
    }
    if compression != 1 || photometric != 3 || bits != 8 || !(1..=2).contains(&spp) || w == 0 || h == 0 || w > 16_384 || h > 16_384 || strip_n == 0 {
        return None;
    }
    let palette = b.get(map..map + 256 * 6)?;
    let channel = |c: usize, i: usize| t.u16(&palette[(c * 256 + i) * 2..]) >> 8;
    let row_len = w as usize * spp;
    let per_strip = if rows == 0 { h as usize } else { rows as usize };
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h as usize {
        let n = y / per_strip;
        if n >= strip_n {
            return None;
        }
        let start = if strip_n == 1 {
            strips
        } else if strip_kind == 3 {
            t.u16(b.get(strips + n * 2..strips + n * 2 + 2)?) as usize
        } else {
            t.u32(b.get(strips + n * 4..strips + n * 4 + 4)?) as usize
        };
        let row = b.get(start + (y % per_strip) * row_len..start + (y % per_strip) * row_len + row_len)?;
        for px in row.chunks_exact(spp) {
            let i = px[0] as usize;
            out.extend_from_slice(&[channel(0, i) as u8, channel(1, i) as u8, channel(2, i) as u8, if spp == 2 { px[1] } else { 255 }]);
        }
    }
    image::RgbaImage::from_raw(w, h, out).map(DynamicImage::ImageRgba8)
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    hay.get(from..)?.windows(needle.len()).position(|w| w == needle).map(|i| i + from)
}

fn base64_decode(text: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for &c in text {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' | b'\r' | b'\n' | b' ' | b'\t' => continue,
            _ => return None,
        };
        acc = acc << 6 | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// Miniatura JPEG que Adobe guarda en los metadatos XMP (`xmpGImg:image`, en base64). Es texto: no se interpreta nada del documento.
pub fn xmp_thumbnail(b: &[u8]) -> Option<DynamicImage> {
    const OPEN: &[u8] = b"<xmpGImg:image>";
    const CLOSE: &[u8] = b"</xmpGImg:image>";
    let mut best: Option<DynamicImage> = None;
    let mut from = 0;
    while let Some(start) = find(b, OPEN, from).map(|i| i + OPEN.len()) {
        let end = find(b, CLOSE, start)?;
        from = end;
        // Los saltos de línea van escritos como la entidad `&#xA;`, cuyas letras también serían base64 válido.
        let clean = String::from_utf8_lossy(&b[start..end]).replace("&#xA;", "").replace("&#xa;", "");
        let Some(img) = base64_decode(clean.as_bytes()).and_then(|d| image::load_from_memory_with_format(&d, image::ImageFormat::Jpeg).ok()) else { continue };
        if best.as_ref().map_or(true, |p| img.width() > p.width()) {
            best = Some(img);
        }
    }
    best
}

/// Interpreta el bloque `%%BeginPreview` de EPSI: un mapa de bits en hexadecimal dentro de comentarios.
pub fn parse_epsi(text: &[u8]) -> Option<DynamicImage> {
    const TAG: &[u8] = b"%%BeginPreview:";
    let at = text.windows(TAG.len()).position(|w| w == TAG)? + TAG.len();
    let rest = &text[at..];
    let eol = rest.iter().position(|b| *b == b'\n' || *b == b'\r')?;
    let args: Vec<u32> = std::str::from_utf8(&rest[..eol]).ok()?.split_whitespace().filter_map(|t| t.parse().ok()).collect();
    let (&w, &h, &depth) = (args.first()?, args.get(1)?, args.get(2)?);
    if w == 0 || h == 0 || w > 16_384 || h > 16_384 || !matches!(depth, 1 | 2 | 4 | 8) {
        return None;
    }
    let row = (w as usize * depth as usize).div_ceil(8);
    let need = row.checked_mul(h as usize)?;
    let mut bytes = Vec::with_capacity(need);
    let mut high: Option<u8> = None;
    for line in rest[eol..].split(|b| *b == b'\n' || *b == b'\r') {
        if line.starts_with(b"%%EndPreview") {
            break;
        }
        for c in line.iter().skip(1).filter_map(|b| (*b as char).to_digit(16)) {
            match high.take() {
                Some(h) => bytes.push(h << 4 | c as u8),
                None => high = Some(c as u8),
            }
        }
    }
    if bytes.len() < need {
        return None;
    }
    let max = (1u32 << depth) - 1;
    let mut gray = Vec::with_capacity((w * h) as usize);
    for y in 0..h as usize {
        for x in 0..w as usize {
            let bit = x * depth as usize;
            let v = (bytes[y * row + bit / 8] >> (8 - depth as usize - bit % 8)) as u32 & max;
            gray.push((255 - v * 255 / max) as u8); // en EPSI, 0 es blanco
        }
    }
    GrayImage::from_raw(w, h, gray).map(DynamicImage::ImageLuma8)
}

// ───────── RAW basados en TIFF (ARW, NEF, DNG, CR2…) ─────────

#[derive(Default)]
struct Tiff {
    le: bool,
}

impl Tiff {
    fn u16(&self, b: &[u8]) -> u16 {
        if self.le { u16::from_le_bytes([b[0], b[1]]) } else { u16::from_be_bytes([b[0], b[1]]) }
    }
    fn u32(&self, b: &[u8]) -> u32 {
        if self.le { u32::from_le_bytes([b[0], b[1], b[2], b[3]]) } else { u32::from_be_bytes([b[0], b[1], b[2], b[3]]) }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RawPreview {
    pub offset: u64,
    pub len: u64,
    /// Orientación EXIF (1–8) declarada en la primera IFD.
    pub orientation: u16,
}

/// Recorre las IFD (también las SubIFD) y devuelve el JPEG incrustado más grande.
pub fn find_raw_preview(f: &mut File) -> io::Result<Option<RawPreview>> {
    let mut head = [0u8; 8];
    f.seek(SeekFrom::Start(0))?;
    f.read_exact(&mut head)?;
    let t = match &head[..2] {
        b"II" => Tiff { le: true },
        b"MM" => Tiff { le: false },
        _ => return Ok(None),
    };
    let mut queue = vec![t.u32(&head[4..]) as u64];
    let (mut seen, mut best, mut orientation) = (0, None::<RawPreview>, 0u16);
    let size = f.metadata()?.len();

    while let Some(at) = queue.pop() {
        seen += 1;
        if seen > MAX_IFDS || at == 0 || at + 2 > size {
            continue;
        }
        let mut n = [0u8; 2];
        f.seek(SeekFrom::Start(at))?;
        f.read_exact(&mut n)?;
        let count = t.u16(&n);
        if count > MAX_ENTRIES {
            continue;
        }
        let mut entries = vec![0u8; count as usize * 12 + 4];
        if f.read_exact(&mut entries).is_err() {
            continue;
        }
        let (mut jpeg_at, mut jpeg_len, mut strip_at, mut strip_len, mut compression) = (0u64, 0u64, 0u64, 0u64, 0u16);
        for e in entries[..count as usize * 12].chunks_exact(12) {
            let (tag, kind, items) = (t.u16(&e[0..]), t.u16(&e[2..]), t.u32(&e[4..]));
            let single = if kind == 3 { t.u16(&e[8..]) as u64 } else { t.u32(&e[8..]) as u64 };
            match tag {
                259 => compression = single as u16,
                273 if items == 1 => strip_at = single,
                279 if items == 1 => strip_len = single,
                513 => jpeg_at = single,
                514 => jpeg_len = single,
                274 if seen == 1 => orientation = single as u16,
                330 => {
                    if items == 1 {
                        queue.push(single);
                    } else if items <= 8 {
                        let mut list = vec![0u8; items as usize * 4];
                        let back = f.stream_position()?;
                        f.seek(SeekFrom::Start(single))?;
                        if f.read_exact(&mut list).is_ok() {
                            queue.extend(list.chunks_exact(4).map(|c| t.u32(c) as u64));
                        }
                        f.seek(SeekFrom::Start(back))?;
                    }
                }
                _ => {}
            }
        }
        let next = t.u32(&entries[count as usize * 12..]) as u64;
        if next != 0 {
            queue.push(next);
        }
        for (o, l) in [(jpeg_at, jpeg_len), if matches!(compression, 6 | 7) { (strip_at, strip_len) } else { (0, 0) }] {
            if o > 0 && l > 2 && o + l <= size && l <= MAX_EMBEDDED && best.as_ref().map_or(true, |b| l > b.len) {
                best = Some(RawPreview { offset: o, len: l, orientation: 0 });
            }
        }
    }
    Ok(best.map(|b| RawPreview { orientation, ..b }))
}

/// JPEG incrustado de un RAW de cámara, ya orientado.
pub fn raw_preview(path: &Path) -> io::Result<Option<DynamicImage>> {
    let mut f = File::open(path)?;
    let Some(p) = find_raw_preview(&mut f)? else { return Ok(None) };
    let b = read_at(&mut f, p.offset, p.len)?;
    if b.len() < 3 || b[..3] != [0xFF, 0xD8, 0xFF] {
        return Ok(None);
    }
    let Ok(mut img) = image::load_from_memory_with_format(&b, image::ImageFormat::Jpeg) else { return Ok(None) };
    if let Some(o) = image::metadata::Orientation::from_exif(p.orientation.min(255) as u8) {
        img.apply_orientation(o);
    }
    Ok(Some(img))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp(name: &str, b: &[u8]) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("nova-emb-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join(name);
        std::fs::File::create(&p).unwrap().write_all(b).unwrap();
        p
    }

    fn jpeg(w: u32, h: u32) -> Vec<u8> {
        let mut o = std::io::Cursor::new(vec![]);
        DynamicImage::new_rgb8(w, h).write_to(&mut o, image::ImageFormat::Jpeg).unwrap();
        o.into_inner()
    }

    #[test]
    fn epsi_preview_is_decoded_and_ps_is_ignored() {
        let eps = b"%!PS-Adobe-3.0 EPSF-3.0\n%%BeginPreview: 8 2 1 2\n%FF\n%00\n%%EndPreview\nshowpage\n";
        let img = eps_preview(&tmp("a.eps", eps)).unwrap().unwrap().to_luma8();
        assert_eq!(img.dimensions(), (8, 2));
        assert_eq!(img.get_pixel(0, 0).0[0], 0, "1 es negro");
        assert_eq!(img.get_pixel(0, 1).0[0], 255);
    }

    #[test]
    fn eps_without_preview_or_with_broken_one_is_none() {
        assert!(eps_preview(&tmp("b.eps", b"%!PS-Adobe-3.0\nshowpage\n")).unwrap().is_none());
        assert!(parse_epsi(b"%%BeginPreview: 99999 2 1 2\n").is_none());
        assert!(parse_epsi(b"%%BeginPreview: 8 8 1 8\n%FF\n%%EndPreview\n").is_none(), "datos incompletos");
    }

    #[test]
    fn dos_eps_with_tiff_preview() {
        let mut tiff = std::io::Cursor::new(vec![]);
        DynamicImage::new_rgb8(5, 3).write_to(&mut tiff, image::ImageFormat::Tiff).unwrap();
        let tiff = tiff.into_inner();
        let mut f = DOS_EPS.to_vec();
        f.extend(30u32.to_le_bytes());
        f.extend(0u32.to_le_bytes());
        f.extend([0; 8]);
        f.extend(30u32.to_le_bytes());
        f.extend((tiff.len() as u32).to_le_bytes());
        f.extend([0xFF, 0xFF]);
        f.extend(&tiff);
        assert_eq!(eps_preview(&tmp("c.eps", &f)).unwrap().unwrap().width(), 5);
    }

    #[test]
    fn palette_tiff_with_alpha_and_broken_inputs() {
        let mut f = b"II*\0".to_vec();
        f.extend(8u32.to_le_bytes());
        let (pixels_at, map_at) = (200u32, 300u32);
        f.extend(8u16.to_le_bytes());
        for (tag, kind, v) in [(256u16, 4u16, 2u32), (257, 4, 1), (258, 3, 8), (259, 3, 1), (262, 3, 3), (273, 4, pixels_at), (277, 3, 2), (320, 3, map_at)] {
            entry(&mut f, tag, kind, v);
        }
        f.resize(pixels_at as usize, 0);
        f.extend([1, 255, 2, 0]); // índice + alfa, dos píxeles
        f.resize(map_at as usize, 0);
        let mut map = vec![0u8; 256 * 6];
        map[3] = 0xAA; // R[1] (byte alto, little endian)
        map[256 * 2 + 5] = 0xBB; // G[2]
        f.extend(map);
        let img = palette_tiff(&f).unwrap().to_rgba8();
        assert_eq!(img.get_pixel(0, 0).0, [0xAA, 0, 0, 255]);
        assert_eq!(img.get_pixel(1, 0).0, [0, 0xBB, 0, 0]);
        for cut in 0..f.len() {
            let _ = palette_tiff(&f[..cut]); // truncado: `None` o imagen, nunca pánico
        }
        assert!(palette_tiff(b"II").is_none());
    }

    #[test]
    fn xmp_thumbnail_is_found_and_entities_removed() {
        const AL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut b64 = String::new();
        for c in jpeg(6, 3).chunks(3) {
            let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
            for (i, s) in [18, 12, 6, 0].into_iter().enumerate() {
                b64.push(if i < c.len() + 1 { AL[(n >> s & 63) as usize] as char } else { '=' });
            }
            b64.push_str("&#xA;");
        }
        let doc = format!("%!PS-Adobe\n<xmpGImg:image>{b64}</xmpGImg:image>\nshowpage");
        assert_eq!(eps_preview(&tmp("x.eps", doc.as_bytes())).unwrap().unwrap().width(), 6);
        assert!(xmp_thumbnail(b"<xmpGImg:image>@@@</xmpGImg:image>").is_none());
    }

    fn entry(v: &mut Vec<u8>, tag: u16, kind: u16, value: u32) {
        v.extend(tag.to_le_bytes());
        v.extend(kind.to_le_bytes());
        v.extend(1u32.to_le_bytes());
        v.extend(value.to_le_bytes());
    }

    #[test]
    fn raw_picks_the_largest_embedded_jpeg_and_applies_orientation() {
        let (small, big) = (jpeg(4, 4), jpeg(20, 10));
        let mut f = b"II*\0".to_vec();
        f.extend(8u32.to_le_bytes());
        // IFD0: miniatura pequeña, orientación y SubIFD con la vista grande.
        let (ifd1_at, small_at) = (8 + 2 + 4 * 12 + 4u32, 200u32);
        let big_at = small_at + small.len() as u32;
        f.extend(4u16.to_le_bytes());
        entry(&mut f, 274, 3, 6);
        entry(&mut f, 330, 4, ifd1_at);
        entry(&mut f, 513, 4, small_at);
        entry(&mut f, 514, 4, small.len() as u32);
        f.extend(0u32.to_le_bytes());
        f.extend(2u16.to_le_bytes());
        entry(&mut f, 513, 4, big_at);
        entry(&mut f, 514, 4, big.len() as u32);
        f.extend(0u32.to_le_bytes());
        f.resize(small_at as usize, 0);
        f.extend(&small);
        f.extend(&big);
        let img = raw_preview(&tmp("r.arw", &f)).unwrap().unwrap();
        assert_eq!((img.width(), img.height()), (10, 20), "la orientación 6 gira la imagen");
    }

    #[test]
    fn raw_without_preview_or_corrupt_is_none_or_error_never_panic() {
        assert!(raw_preview(&tmp("n.arw", b"no es tiff....")).unwrap().is_none());
        let mut f = b"II*\0".to_vec();
        f.extend(8u32.to_le_bytes());
        f.extend(0xFFFFu16.to_le_bytes());
        assert!(raw_preview(&tmp("m.arw", &f)).unwrap().is_none());
    }
}
