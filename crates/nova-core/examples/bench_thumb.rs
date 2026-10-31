//! Mide la generación real de miniaturas con el mismo servicio que usa la aplicación y muestra
//! QUÉ formatos puede resolver este equipo (en Windows incluye los proveedores del sistema: PDF, PSD, AI, vídeo…).
//!
//! Uso (PowerShell):
//!   cargo run --release --manifest-path crates/nova-core/Cargo.toml --example bench_thumb -- "C:\Users\tu\Pictures" 128
//!
//! Hace dos pasadas sobre los archivos de la carpeta (sin recursión): la primera genera (caché vacía) y la
//! segunda lee de la caché. La caché de la prueba va en una carpeta temporal y se borra al terminar.
//! Usa `--release`: en debug las cifras de decodificación no son representativas.

use nova_core::thumbs::{is_thumb_ext, ThumbError, ThumbService};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Default)]
struct Row {
    ok: u32,
    unsupported: u32,
    toolarge: u32,
    failed: u32,
    ms: f64,
    bytes: usize,
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().expect("uso: bench_thumb <carpeta> [px]"));
    let px: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(128);
    let cache = std::env::temp_dir().join(format!("lfind-bench-thumb-{}", std::process::id()));
    let svc = ThumbService::with_defaults(cache.clone());

    let files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("no se pudo leer la carpeta")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().map_or(false, |e| is_thumb_ext(&e.to_string_lossy().to_lowercase())))
        .collect();
    println!("{} archivos candidatos en {} (miniaturas de {} px)\n", files.len(), dir.display(), px);

    for pass in ["1.ª pasada (genera)", "2.ª pasada (caché)"] {
        let mut by_ext: BTreeMap<String, Row> = BTreeMap::new();
        let t0 = Instant::now();
        for f in &files {
            let ext = f.extension().unwrap().to_string_lossy().to_lowercase();
            let t = Instant::now();
            let r = svc.get(f, px, None);
            let row = by_ext.entry(ext).or_default();
            row.ms += t.elapsed().as_secs_f64() * 1e3;
            match r {
                Ok(th) => {
                    row.ok += 1;
                    row.bytes += th.bytes.len();
                }
                Err(ThumbError::Unsupported) => row.unsupported += 1,
                Err(ThumbError::TooLarge) => row.toolarge += 1,
                Err(_) => row.failed += 1,
            }
        }
        println!("── {pass}: {:.0} ms en total ──", t0.elapsed().as_secs_f64() * 1e3);
        println!("{:<8} {:>4} {:>12} {:>9} {:>7} {:>9} {:>10}", "ext", "ok", "sin proveedor", "muy grande", "fallo", "ms/archivo", "KB/miniat.");
        for (ext, r) in &by_ext {
            let n = (r.ok + r.unsupported + r.toolarge + r.failed).max(1) as f64;
            println!("{:<8} {:>4} {:>12} {:>9} {:>7} {:>9.1} {:>10.1}", ext, r.ok, r.unsupported, r.toolarge, r.failed, r.ms / n, if r.ok > 0 { r.bytes as f64 / r.ok as f64 / 1024.0 } else { 0.0 });
        }
        println!();
    }
    println!("«sin proveedor» = ni LFind ni Windows pueden generar la miniatura de ese formato en este equipo.");
    let _ = std::fs::remove_dir_all(cache);
}
