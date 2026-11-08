//! Prueba el visor con archivos reales: `viewer_probe <carpeta-salida> <archivo>...`
//! Imprime cómo se resuelve cada archivo y copia las conversiones a la carpeta de salida para revisarlas.
//! Las rutas no se imprimen: solo extensión, tamaño y resultado.

use nova_core::thumbs::{ThumbConfig, ThumbService};
use nova_core::viewer::serve::{serve, ServeRequest};
use nova_core::viewer::{Source, ViewerService};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let out = PathBuf::from(args.next().expect("uso: viewer_probe <salida> <archivo>..."));
    std::fs::create_dir_all(&out).unwrap();
    let thumbs = Arc::new(ThumbService::new(
        ThumbConfig { cache_dir: out.join("t"), max_cache_bytes: 1 << 30, parallel: 1 },
        nova_core::thumbs::default_providers(),
    ));
    let svc = ViewerService::new(out.join("cache"), thumbs);
    for (i, f) in args.enumerate() {
        let p = Path::new(&f);
        let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let mb = std::fs::metadata(p).map_or(0.0, |m| m.len() as f64 / 1048576.0);
        let t = Instant::now();
        let r = svc.open(p);
        let ms = t.elapsed().as_millis();
        let line = match &r {
            Ok(Source::Image { token, mime, width, height, reduced, note, facts }) => {
                let resp = serve(svc.tickets(), &ServeRequest { method: "GET", token, range: None, origin: None });
                let name = format!("{i:03}-{ext}.{}", mime.rsplit('/').next().unwrap_or("bin").replace("svg+xml", "svg"));
                if *reduced || mime.contains("jpeg") || mime.contains("png") {
                    let _ = std::fs::write(out.join(&name), &resp.body);
                }
                let fx: Vec<String> = facts.iter().map(|f| f.value.clone()).collect();
                format!("IMAGE {mime} {width:?}x{height:?} http={} bytes={} reduced={reduced} note={note:?} {fx:?} -> {name}", resp.status, resp.body.len())
            }
            Ok(Source::Pdf { illustrator, size, .. }) => format!("PDF illustrator={illustrator} size={size}"),
            Ok(Source::Unsupported { message, .. }) => format!("UNSUPPORTED {message}"),
            Ok(o) => format!("{o:?}").chars().take(60).collect(),
            Err(e) => format!("ERROR {e}"),
        };
        println!("{i:03} .{ext:<5} {mb:>9.2} MB {ms:>7} ms  {line}");
    }
}
