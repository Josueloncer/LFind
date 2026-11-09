//! Imágenes de fondo elegidas por el usuario. Se reducen aquí y se guardan en una carpeta propia,
//! con un identificador derivado del contenido. La imagen original no se toca y puede moverse o borrarse sin que el fondo se pierda.

use crate::b64;
use crate::thumbs::{RasterProvider, ThumbProvider};
use std::fs;
use std::path::{Path, PathBuf};

pub const MAX_BYTES: usize = 6 * 1024 * 1024;
/// Lado mayor de una imagen de fondo: más detalle no se llegaría a ver.
pub const MAX_SIDE: u32 = 1920;
const MAX_FILES: usize = 64;

fn valid_id(id: &str) -> bool {
    id.len() == 16 && id.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn fnv(data: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in data {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

fn kind_of(b: &[u8]) -> Option<&'static str> {
    if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if b.len() > 12 && &b[..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        Some("image/webp")
    } else if b.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("image/png")
    } else {
        None
    }
}

pub struct Backgrounds {
    dir: PathBuf,
}

impl Backgrounds {
    pub fn new(dir: PathBuf) -> Self {
        Backgrounds { dir }
    }

    fn path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.img"))
    }

    /// Reduce la imagen de `path` y la guarda. Un archivo que solo está en la nube se rechaza: abrirlo lo descargaría.
    pub fn import(&self, path: &Path) -> Result<String, String> {
        let meta = fs::metadata(path).map_err(|e| format!("No se pudo abrir la imagen: {e}"))?;
        if crate::dir::is_cloud_only(&meta) {
            return Err("Esta imagen solo está en la nube. Descárgala primero para usarla como fondo.".into());
        }
        let raster = RasterProvider::default();
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        if !meta.is_file() || !raster.supports(&ext) {
            return Err("Formato de imagen no admitido.".into());
        }
        let image = raster.render(path, MAX_SIDE, &|| false).map_err(|e| e.message())?;
        self.store(&image.bytes)
    }

    /// Guarda una imagen ya reducida y devuelve su identificador.
    fn store(&self, bytes: &[u8]) -> Result<String, String> {
        if bytes.len() > MAX_BYTES {
            return Err("La imagen es demasiado grande.".into());
        }
        kind_of(bytes).ok_or("Formato de imagen no admitido.")?;
        fs::create_dir_all(&self.dir).map_err(|e| format!("No se pudo crear la carpeta de fondos: {e}"))?;
        if fs::read_dir(&self.dir).map(|d| d.count()).unwrap_or(0) >= MAX_FILES {
            return Err("Hay demasiadas imágenes guardadas. Quita alguna que no uses.".into());
        }
        let id = fnv(bytes);
        let dest = self.path(&id);
        if !dest.exists() {
            let tmp = dest.with_extension("tmp");
            fs::write(&tmp, bytes).map_err(|e| format!("No se pudo guardar la imagen: {e}"))?;
            fs::rename(&tmp, &dest).map_err(|e| format!("No se pudo guardar la imagen: {e}"))?;
        }
        Ok(id)
    }

    /// `data:` URL de una imagen guardada, o `None` si ya no existe.
    pub fn load(&self, id: &str) -> Option<String> {
        if !valid_id(id) {
            return None;
        }
        let bytes = fs::read(self.path(id)).ok()?;
        Some(format!("data:{};base64,{}", kind_of(&bytes)?, b64::encode(&bytes)))
    }

    /// Retira las imágenes que ya no usa ninguna personalización. Solo actúa en la carpeta propia.
    pub fn retain(&self, keep: &[String]) -> usize {
        let Ok(rd) = fs::read_dir(&self.dir) else { return 0 };
        let mut removed = 0;
        for e in rd.flatten() {
            let p: PathBuf = e.path();
            let id = stem(&p);
            if p.extension().is_some_and(|x| x == "img") && !keep.iter().any(|k| *k == id) && fs::remove_file(&p).is_ok() {
                removed += 1;
            }
        }
        removed
    }
}

fn stem(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(n: &str) -> Backgrounds {
        let d = std::env::temp_dir().join(format!("lfind-bg-{n}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        Backgrounds::new(d)
    }
    const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3];

    #[test]
    fn saves_and_loads_by_content() {
        let s = store("a");
        let id = s.store(JPEG).unwrap();
        assert_eq!(id, s.store(JPEG).unwrap(), "la misma imagen no se duplica");
        assert_eq!(s.load(&id).unwrap(), format!("data:image/jpeg;base64,{}", b64::encode(JPEG)));
    }

    #[test]
    fn rejects_what_is_not_an_image_or_too_big() {
        let s = store("b");
        assert!(s.store(b"hola, no soy una imagen").is_err());
        let mut big = JPEG.to_vec();
        big.resize(MAX_BYTES + 1, 0);
        assert!(s.store(&big).is_err());
    }

    fn picture(dir: &Path, name: &str, w: u32, h: u32) -> PathBuf {
        fs::create_dir_all(dir).unwrap();
        let p = dir.join(name);
        image::RgbImage::from_pixel(w, h, image::Rgb([200, 90, 20])).save(&p).unwrap();
        p
    }

    fn side_of(s: &Backgrounds, id: &str) -> u32 {
        let bytes = fs::read(s.path(id)).unwrap();
        let img = image::load_from_memory(&bytes).unwrap();
        img.width().max(img.height())
    }

    #[test]
    fn imports_a_local_picture_reducing_it() {
        let s = store("e");
        let src = picture(&s.dir.with_extension("src"), "grande.png", 3000, 1500);
        let id = s.import(&src).unwrap();
        assert_eq!(side_of(&s, &id), MAX_SIDE);
        assert!(s.load(&id).unwrap().starts_with("data:image/jpeg;base64,"));
        assert!(src.exists(), "la imagen original no se toca");
    }

    #[test]
    fn rejects_files_that_are_not_pictures() {
        let s = store("f");
        let dir = s.dir.with_extension("src");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("nota.txt"), b"hola").unwrap();
        fs::write(dir.join("rota.png"), b"no soy un png").unwrap();
        assert!(s.import(&dir.join("nota.txt")).is_err());
        assert!(s.import(&dir.join("rota.png")).is_err());
        assert!(s.import(&dir.join("no-existe.png")).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn never_opens_a_picture_that_is_only_in_the_cloud() {
        let s = store("g");
        let src = picture(&s.dir.with_extension("src"), "nube.png", 40, 40);
        assert!(std::process::Command::new("attrib").arg("+O").arg(&src).status().unwrap().success());
        assert!(s.import(&src).unwrap_err().contains("nube"));
        assert!(std::process::Command::new("attrib").arg("-O").arg(&src).status().unwrap().success());
    }

    #[test]
    fn ids_cannot_escape_the_folder() {
        let s = store("c");
        for bad in ["..\\..\\x", "../../secreto", "", "abc", "0123456789abcdeg"] {
            assert!(s.load(bad).is_none());
        }
    }

    #[test]
    fn retain_only_removes_unused_images() {
        let s = store("d");
        let a = s.store(JPEG).unwrap();
        let b = s.store(&[0xFF, 0xD8, 0xFF, 9]).unwrap();
        fs::write(s.dir.join("otro.txt"), b"x").unwrap();
        assert_eq!(s.retain(&[a.clone()]), 1);
        assert!(s.load(&a).is_some() && s.load(&b).is_none() && s.dir.join("otro.txt").exists());
    }
}
