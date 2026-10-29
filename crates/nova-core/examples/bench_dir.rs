//! Mide la lectura real de una carpeta con el mismo código que usa la app.
//!
//! Uso (en Windows, PowerShell):
//!   cargo run --release --manifest-path crates/nova-core/Cargo.toml --example bench_dir -- "C:\Windows\System32"
//!
//! Imprime tiempo hasta el primer lote, tiempo total y el coste de serializar
//! cada lote a JSON (lo que viaja por IPC hacia la interfaz). Usa `--release`:
//! en debug los números no son representativos.

use nova_core::dir;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

fn main() {
    let path = std::env::args().nth(1).map(PathBuf::from).expect("uso: bench_dir <carpeta>");
    let t0 = Instant::now();
    let mut first: Option<Duration> = None;
    let (mut batches, mut json_bytes, mut json_time) = (0usize, 0usize, Duration::ZERO);

    let stats = dir::read_dir_batched(&path, dir::FIRST_BATCH, dir::BATCH, &AtomicBool::new(false), |b| {
        first.get_or_insert_with(|| t0.elapsed());
        batches += 1;
        let t = Instant::now();
        json_bytes += serde_json::to_vec(&b).expect("json").len();
        json_time += t.elapsed();
    })
    .expect("no se pudo leer la carpeta");

    let total = t0.elapsed();
    println!("carpeta:            {}", path.display());
    println!("entradas:           {} (omitidas: {})", stats.total, stats.skipped);
    println!("lotes:              {}", batches);
    println!("primer lote:        {:.1} ms", first.map(|d| d.as_secs_f64() * 1e3).unwrap_or(0.0));
    println!("total lectura+json: {:.1} ms", total.as_secs_f64() * 1e3);
    println!("  de los cuales json: {:.1} ms ({:.1} MB)", json_time.as_secs_f64() * 1e3, json_bytes as f64 / 1e6);
}
