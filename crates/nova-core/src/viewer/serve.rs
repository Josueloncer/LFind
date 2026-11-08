//! Entrega de contenido al visor por el protocolo propio `lfview`.
//!
//! La interfaz nunca pide una ruta: solo presenta un `token` que `ViewerService` emitió para un archivo concreto. Así la webview no
//! gana acceso general al disco (no hay `scope` amplio) y el contenido se trata como no confiable: tipo fijado por el backend tras
//! inspeccionar el archivo, `nosniff`, CSP propia que impide ejecutar nada y, en la práctica, solo se usa dentro de `<img>`, `<video>`,
//! `<audio>` y por `fetch` (PDF.js).

use std::collections::{HashMap, VecDeque};
use std::fs::{self, File};
use std::hash::{BuildHasher, Hasher};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

/// Tamaño máximo de un trozo con `Range`. El protocolo entrega el cuerpo en memoria: los trozos mantienen acotado su coste.
pub const MAX_RANGE_BYTES: u64 = 8 << 20;
/// Sin `Range` se entrega el archivo entero; el visor ya reduce antes las imágenes mayores que esto.
pub const MAX_FULL_BYTES: u64 = 256 << 20;
pub const MAX_TICKETS: usize = 64;

const ORIGINS: &[&str] = &["http://tauri.localhost", "https://tauri.localhost", "http://localhost:1420"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ticket {
    pub file: PathBuf,
    pub mime: &'static str,
    pub size: u64,
    pub mtime_ms: i64,
}

#[derive(Default)]
struct Table {
    by_token: HashMap<String, Ticket>,
    order: VecDeque<String>,
}

#[derive(Default)]
pub struct Tickets {
    table: Mutex<Table>,
    seq: AtomicU64,
}

pub fn mtime_ms(m: &fs::Metadata) -> i64 {
    m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis() as i64)
}

impl Tickets {
    fn lock(&self) -> std::sync::MutexGuard<'_, Table> {
        self.table.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn new_token(&self) -> String {
        // `RandomState` se siembra con aleatoriedad del sistema: no hace falta otra dependencia.
        let word = || {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u64(self.seq.fetch_add(1, Ordering::Relaxed));
            h.finish()
        };
        format!("{:016x}{:016x}", word(), word())
    }

    /// Permiso de lectura para `file`. El mismo archivo sin cambios conserva su token: el navegador reutiliza lo que ya decodificó.
    pub fn issue(&self, file: &Path, mime: &'static str) -> std::io::Result<String> {
        let m = fs::metadata(file)?;
        let t = Ticket { file: file.to_path_buf(), mime, size: m.len(), mtime_ms: mtime_ms(&m) };
        let mut g = self.lock();
        if let Some((k, _)) = g.by_token.iter().find(|(_, v)| **v == t) {
            return Ok(k.clone());
        }
        let token = self.new_token();
        g.by_token.insert(token.clone(), t);
        g.order.push_back(token.clone());
        while g.order.len() > MAX_TICKETS {
            if let Some(old) = g.order.pop_front() {
                g.by_token.remove(&old);
            }
        }
        Ok(token)
    }

    pub fn get(&self, token: &str) -> Option<Ticket> {
        self.lock().by_token.get(token).cloned()
    }

    pub fn clear(&self) {
        let mut g = self.lock();
        g.by_token.clear();
        g.order.clear();
    }

    pub fn len(&self) -> usize {
        self.lock().by_token.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// `Range: bytes=…` a un intervalo inclusivo, recortado a `MAX_RANGE_BYTES`. `None`: inválido o fuera del archivo.
pub fn parse_range(header: &str, len: u64) -> Option<(u64, u64)> {
    let spec = header.trim().strip_prefix("bytes=")?;
    if spec.contains(',') || len == 0 {
        return None;
    }
    let (a, b) = spec.split_once('-')?;
    let (start, end) = if a.is_empty() {
        let n: u64 = b.parse().ok()?;
        if n == 0 {
            return None;
        }
        (len.saturating_sub(n), len - 1)
    } else {
        let s: u64 = a.parse().ok()?;
        let e = if b.is_empty() { len - 1 } else { b.parse::<u64>().ok()?.min(len - 1) };
        (s, e)
    };
    if start > end || start >= len {
        return None;
    }
    Some((start, end.min(start + MAX_RANGE_BYTES - 1)))
}

pub struct ServeRequest<'a> {
    pub method: &'a str,
    pub token: &'a str,
    pub range: Option<&'a str>,
    pub origin: Option<&'a str>,
}

#[derive(Debug)]
pub struct ServeResponse {
    pub status: u16,
    pub headers: Vec<(&'static str, String)>,
    pub body: Vec<u8>,
}

fn base_headers(origin: Option<&str>) -> Vec<(&'static str, String)> {
    let mut h = vec![
        ("X-Content-Type-Options", "nosniff".to_string()),
        ("Content-Security-Policy", "default-src 'none'; style-src 'unsafe-inline'; sandbox".to_string()),
        ("Cross-Origin-Resource-Policy", "cross-origin".to_string()),
        ("Accept-Ranges", "bytes".to_string()),
        ("Vary", "Origin".to_string()),
    ];
    if let Some(o) = origin.filter(|o| ORIGINS.contains(o)) {
        h.push(("Access-Control-Allow-Origin", o.to_string()));
        h.push(("Access-Control-Expose-Headers", "Content-Range, Content-Length, Accept-Ranges".to_string()));
    }
    h
}

fn empty(status: u16, origin: Option<&str>) -> ServeResponse {
    ServeResponse { status, headers: base_headers(origin), body: vec![] }
}

/// Responde a una petición del protocolo. Comprueba de nuevo el archivo: si cambió desde que se emitió el permiso, no se sirve.
pub fn serve(tickets: &Tickets, req: &ServeRequest) -> ServeResponse {
    if req.method == "OPTIONS" {
        let mut r = empty(204, req.origin);
        r.headers.push(("Access-Control-Allow-Methods", "GET, HEAD".to_string()));
        r.headers.push(("Access-Control-Allow-Headers", "Range".to_string()));
        return r;
    }
    if req.method != "GET" && req.method != "HEAD" {
        return empty(405, req.origin);
    }
    let Some(t) = tickets.get(req.token) else { return empty(404, req.origin) };
    let Ok(m) = fs::metadata(&t.file) else { return empty(410, req.origin) };
    if m.len() != t.size || mtime_ms(&m) != t.mtime_ms {
        return empty(410, req.origin);
    }

    let mut headers = base_headers(req.origin);
    headers.push(("Content-Type", t.mime.to_string()));
    headers.push(("Cache-Control", "private, max-age=600".to_string()));

    let (status, start, end) = match req.range {
        Some(h) => match parse_range(h, t.size) {
            Some((s, e)) => (206, s, e),
            None => {
                let mut r = empty(416, req.origin);
                r.headers.push(("Content-Range", format!("bytes */{}", t.size)));
                return r;
            }
        },
        None if t.size > MAX_FULL_BYTES => return empty(413, req.origin),
        None => (200, 0, t.size.saturating_sub(1)),
    };
    let count = if t.size == 0 { 0 } else { end - start + 1 };
    if status == 206 {
        headers.push(("Content-Range", format!("bytes {start}-{end}/{}", t.size)));
    }
    headers.push(("Content-Length", count.to_string()));
    if req.method == "HEAD" {
        return ServeResponse { status, headers, body: vec![] };
    }
    let mut body = vec![0u8; count as usize];
    let read = File::open(&t.file).and_then(|mut f| {
        f.seek(SeekFrom::Start(start))?;
        f.read_exact(&mut body)
    });
    match read {
        Ok(()) => ServeResponse { status, headers, body },
        Err(_) => empty(500, req.origin),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, bytes: &[u8]) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nova-serve-{}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        let p = d.join(name);
        fs::write(&p, bytes).unwrap();
        p
    }

    fn get<'a>(token: &'a str, range: Option<&'a str>) -> ServeRequest<'a> {
        ServeRequest { method: "GET", token, range, origin: None }
    }

    fn header<'a>(r: &'a ServeResponse, name: &str) -> Option<&'a str> {
        r.headers.iter().find(|(k, _)| *k == name).map(|(_, v)| v.as_str())
    }

    #[test]
    fn ranges() {
        assert_eq!(parse_range("bytes=0-9", 100), Some((0, 9)));
        assert_eq!(parse_range("bytes=90-", 100), Some((90, 99)));
        assert_eq!(parse_range("bytes=-10", 100), Some((90, 99)));
        assert_eq!(parse_range("bytes=95-500", 100), Some((95, 99)));
        for bad in ["bytes=200-", "bytes=5-2", "bytes=a-b", "bytes=0-1,4-5", "items=0-1", "bytes=-0"] {
            assert_eq!(parse_range(bad, 100), None, "{bad}");
        }
        let big = 1 << 40;
        assert_eq!(parse_range("bytes=0-", big), Some((0, MAX_RANGE_BYTES - 1)), "los trozos están acotados");
    }

    #[test]
    fn only_issued_tokens_are_served() {
        let t = Tickets::default();
        let p = file("a.bin", b"0123456789");
        let tok = t.issue(&p, "image/png").unwrap();
        assert_eq!(serve(&t, &get("../../etc/passwd", None)).status, 404);
        assert_eq!(serve(&t, &get(&p.to_string_lossy(), None)).status, 404, "una ruta nunca es un permiso");
        let r = serve(&t, &get(&tok, None));
        assert_eq!((r.status, r.body.as_slice()), (200, &b"0123456789"[..]));
        assert_eq!(header(&r, "Content-Type"), Some("image/png"));
        assert_eq!(header(&r, "X-Content-Type-Options"), Some("nosniff"));
        assert!(header(&r, "Content-Security-Policy").unwrap().contains("sandbox"));
    }

    #[test]
    fn partial_content_and_invalid_range() {
        let t = Tickets::default();
        let tok = t.issue(&file("b.bin", b"0123456789"), "video/mp4").unwrap();
        let r = serve(&t, &get(&tok, Some("bytes=2-4")));
        assert_eq!((r.status, r.body.as_slice()), (206, &b"234"[..]));
        assert_eq!(header(&r, "Content-Range"), Some("bytes 2-4/10"));
        assert_eq!(serve(&t, &get(&tok, Some("bytes=50-"))).status, 416);
    }

    #[test]
    fn a_file_that_changes_is_no_longer_served() {
        let t = Tickets::default();
        let p = file("c.bin", b"abc");
        let tok = t.issue(&p, "image/png").unwrap();
        fs::write(&p, b"abcd").unwrap();
        assert_eq!(serve(&t, &get(&tok, None)).status, 410);
        let new = t.issue(&p, "image/png").unwrap();
        assert_ne!(new, tok, "archivo distinto, permiso distinto");
    }

    #[test]
    fn same_file_keeps_token_and_table_is_bounded() {
        let t = Tickets::default();
        let p = file("d.bin", b"x");
        assert_eq!(t.issue(&p, "image/png").unwrap(), t.issue(&p, "image/png").unwrap());
        for i in 0..MAX_TICKETS + 10 {
            t.issue(&file(&format!("e{i}.bin"), b"y"), "image/png").unwrap();
        }
        assert_eq!(t.len(), MAX_TICKETS);
        t.clear();
        assert!(t.is_empty());
    }

    #[test]
    fn cors_only_for_the_app_and_preflight() {
        let t = Tickets::default();
        let tok = t.issue(&file("f.bin", b"x"), "application/pdf").unwrap();
        let with = |origin| serve(&t, &ServeRequest { method: "GET", token: &tok, range: None, origin: Some(origin) });
        assert_eq!(header(&with("http://tauri.localhost"), "Access-Control-Allow-Origin"), Some("http://tauri.localhost"));
        assert_eq!(header(&with("https://evil.example"), "Access-Control-Allow-Origin"), None);
        assert_eq!(serve(&t, &ServeRequest { method: "OPTIONS", token: "", range: None, origin: Some("http://tauri.localhost") }).status, 204);
        assert_eq!(serve(&t, &ServeRequest { method: "POST", token: &tok, range: None, origin: None }).status, 405);
    }
}
