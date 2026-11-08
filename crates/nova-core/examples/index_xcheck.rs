//! Validación cruzada del índice: escribe en un directorio el esquema y las sentencias SQL reales de `sqlgen`, un conjunto de datos
//! determinista, una batería de consultas con su SQL generado y el resultado que da `MemStore` (la referencia).
//! `scripts/index_xcheck.py` carga eso en SQLite real, ejecuta el mismo SQL y compara resultados y orden.
//!
//! Uso: `cargo run --example index_xcheck -- <directorio>` y luego `python scripts/index_xcheck.py <directorio>`.

use nova_core::index::classify::{depth_of, path_key};
use nova_core::index::model::*;
use nova_core::index::sqlgen::{self, P};
use nova_core::index::store::{IndexStore, MemStore};
use std::fmt::Write as _;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn pick<'a, T>(&mut self, v: &'a [T]) -> &'a T {
        &v[(self.next() as usize) % v.len()]
    }
}

const WORDS: &[&str] = &["informe", "foto", "vacaciones", "proyecto", "factura", "música", "vídeo", "canción", "curso", "tesis", "backup", "diseño", "logo", "cliente", "notas", "final", "borrador", "scan", "reporte", "datos", "a", "ab", "x_y", "50%", "o'neil", "mi-archivo"];
const EXTS: &[&str] = &["pdf", "png", "jpg", "psd", "docx", "xlsx", "mp4", "txt", "zip", "rs", "ts", "dll", "", "mp3", "pptx", "iso"];

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\t', "\\t").replace('\n', "\\n")
}

pub fn main() {
    let dir = std::env::args().nth(1).expect("uso: index_xcheck <directorio>");
    std::fs::create_dir_all(&dir).unwrap();
    let w = |name: &str, s: &str| std::fs::write(format!("{dir}/{name}"), s).unwrap();
    w("schema.sql", sqlgen::SCHEMA);
    w("fts.sql", sqlgen::FTS_SCHEMA);
    w("upsert.sql", sqlgen::UPSERT);
    w("sweep.sql", sqlgen::SWEEP);
    w("remove_tree.sql", sqlgen::REMOVE_TREE);

    let store = MemStore::new();
    let roots = ["C:\\", "D:\\", "E:\\datos"];
    let ids: Vec<i64> = roots.iter().map(|r| store.ensure_root(r).unwrap()).collect();
    let mut rng = Rng(42);
    let mut entries = vec![];
    let mut tsv = String::new();
    for i in 0..30_000usize {
        let ri = (rng.next() % 3) as usize;
        let is_dir = rng.next() % 10 == 0;
        let base: String = (0..(1 + rng.next() % 3)).map(|_| *rng.pick(WORDS)).collect::<Vec<_>>().join(if rng.next() % 2 == 0 { " " } else { "_" });
        let ext = if is_dir { "" } else { *rng.pick(EXTS) };
        let name = if ext.is_empty() { format!("{base}{}", i % 97) } else { format!("{base}{}.{ext}", i % 97) };
        let sub: Vec<String> = (0..(rng.next() % 4)).map(|_| format!("{}{}", rng.pick(WORDS), rng.next() % 5)).collect();
        let prefix = roots[ri].trim_end_matches('\\');
        let path = format!("{prefix}\\{}{}{i}_{name}", sub.join("\\"), if sub.is_empty() { "" } else { "\\" });
        let key = path_key(&path);
        let size = if is_dir { 0 } else if rng.next() % 50 == 0 { 1_073_741_824 + rng.next() % 4_000_000_000 } else { rng.next() % 5_000_000 };
        let modified = Some(1_600_000_000_000i64 + (rng.next() % 200_000_000_000) as i64);
        let created = if rng.next() % 4 == 0 { None } else { Some(1_500_000_000_000i64 + (rng.next() % 200_000_000_000) as i64) };
        let accessed = if rng.next() % 3 == 0 { modified.map(|m| m + 1000) } else { None };
        let e = IndexedEntry { root_id: ids[ri], depth: depth_of(&key), key, path, name_lc: name.to_lowercase(), name, ext: ext.to_string(), is_dir, size, modified, created, accessed };
        let o = |v: Option<i64>| v.map_or("\\N".to_string(), |x| x.to_string());
        writeln!(tsv, "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}", e.root_id, esc(&e.key), esc(&e.path), esc(&e.name), esc(&e.name_lc), e.ext, e.is_dir as i32, e.size, o(e.modified), o(e.created), o(e.accessed), e.depth, 1).unwrap();
        entries.push(e);
    }
    for (i, id) in ids.iter().enumerate() {
        let g = store.begin_scan(*id).unwrap();
        assert_eq!(g, 1);
        let batch: Vec<_> = entries.iter().filter(|e| e.root_id == ids[i]).cloned().collect();
        store.upsert(*id, g, &batch).unwrap();
    }
    w("entries.tsv", &tsv);

    // Batería de consultas.
    let day = 86_400_000i64;
    let mut qs: Vec<Query> = vec![];
    let t = |s: &str| Query { text: s.into(), limit: 60, ..Default::default() };
    for s in ["foto", "FOTO", "fo", "a", "ab", "o'neil", "50%", "x_y", "música", "canción", "foto vacaciones", "ab foto", "informe final backup", "mi-archivo", "datos 1", "zzzz", "\"foto\"", "ñandú", "foto_", ".pdf", "foto.", "informe 4"] {
        qs.push(t(s));
    }
    for s in ["foto", "informe", "a", "informe.pdf", "logo", "zzz"] {
        qs.push(Query { text: s.into(), exact: true, limit: 60, ..Default::default() });
    }
    for e in ["psd", "PNG", ".pdf", "", "nada"] {
        qs.push(Query { exts: if e.is_empty() { vec![] } else { vec![e.into()] }, limit: 60, ..Default::default() });
    }
    qs.push(Query { exts: vec!["psd".into(), "png".into(), "jpg".into()], limit: 60, ..Default::default() });
    for c in ["image", "video", "audio", "archive", "code", "doc", "sheet", "slides", "pdf", "design", "exec", "folder", "desconocida"] {
        qs.push(Query { category: Some(c.into()), limit: 60, ..Default::default() });
    }
    qs.push(Query { category: Some("image".into()), exts: vec!["psd".into()], limit: 60, ..Default::default() });
    for k in [KindFilter::File, KindFilter::Dir] {
        qs.push(Query { kind: k, text: "foto".into(), limit: 60, ..Default::default() });
    }
    qs.push(Query { min_size: Some(1 << 30), limit: 60, ..Default::default() });
    qs.push(Query { min_size: Some(1 << 30), exts: vec!["psd".into()], limit: 60, ..Default::default() });
    qs.push(Query { max_size: Some(1000), kind: KindFilter::File, limit: 60, ..Default::default() });
    qs.push(Query { min_size: Some(10_000), max_size: Some(20_000), limit: 60, ..Default::default() });
    qs.push(Query { min_size: Some(u64::MAX), limit: 60, ..Default::default() });
    let m = 1_600_000_000_000i64;
    qs.push(Query { modified: DateRange { after: Some(m + 150_000 * day), before: None }, limit: 60, ..Default::default() });
    qs.push(Query { modified: DateRange { after: Some(m + 10 * day), before: Some(m + 900 * day) }, text: "info".into(), limit: 60, ..Default::default() });
    qs.push(Query { created: DateRange { before: Some(1_500_000_000_000 + 1000 * day), after: None }, limit: 60, ..Default::default() });
    qs.push(Query { accessed: DateRange { after: Some(m), before: None }, limit: 60, ..Default::default() });
    qs.push(Query { category: Some("pdf".into()), modified: DateRange { after: Some(m + 100_000 * day), before: None }, limit: 60, ..Default::default() });
    qs.push(Query { drives: vec!["D:\\".into()], root_ids: Some(vec![ids[1]]), text: "foto".into(), limit: 60, ..Default::default() });
    qs.push(Query { root_ids: Some(vec![ids[0], ids[2]]), exts: vec!["pdf".into()], limit: 60, ..Default::default() });
    qs.push(Query { root_ids: Some(vec![]), text: "foto".into(), limit: 60, ..Default::default() });
    for f in ["C:\\", "C:\\foto0", "c:\\música1\\", "D:\\datos2", "E:\\datos", "C:\\no\\existe"] {
        qs.push(Query { folder: Some(f.into()), limit: 60, ..Default::default() });
    }
    qs.push(Query { folder: Some("C:\\foto0".into()), text: "informe".into(), kind: KindFilter::File, limit: 60, ..Default::default() });
    for (s, d) in [(SortBy::Name, false), (SortBy::Name, true), (SortBy::Size, false), (SortBy::Size, true), (SortBy::Modified, false), (SortBy::Modified, true)] {
        qs.push(Query { sort: s, desc: d, text: "foto".into(), limit: 60, ..Default::default() });
        qs.push(Query { sort: s, desc: d, limit: 60, ..Default::default() });
    }
    for off in [0usize, 60, 120, 5000] {
        qs.push(Query { text: "o".into(), offset: off, limit: 60, ..Default::default() });
    }
    qs.push(Query { text: "e".into(), limit: 5000, ..Default::default() });
    qs.push(Query { limit: 0, ..Default::default() });
    qs.push(Query::default());
    qs.push(Query { text: "foto".into(), exts: vec!["jpg".into()], kind: KindFilter::File, min_size: Some(100), max_size: Some(4_000_000), modified: DateRange { after: Some(m), before: None }, folder: Some("C:\\".into()), root_ids: Some(vec![ids[0]]), sort: SortBy::Size, desc: true, limit: 40, ..Default::default() });

    let cancel = Arc::new(AtomicBool::new(false));
    let mut out = String::new();
    for (i, q) in qs.iter().enumerate() {
        let page = store.query(q, &cancel).unwrap();
        let expected: Vec<String> = page.hits.iter().map(|h| esc(&h.path)).collect();
        for fts in [true, false] {
            let b = sqlgen::build_query(q, fts);
            let params: Vec<String> = b.params.iter().map(|p| match p { P::I(v) => format!("I:{v}"), P::T(s) => format!("T:{}", esc(s)) }).collect();
            writeln!(out, "{i}\t{}\t{}\t{}\t{}\t{}", fts as i32, esc(&b.sql), params.join("\x1f"), page.more as i32, expected.join("\x1f")).unwrap();
        }
    }
    w("queries.tsv", &out);
    eprintln!("{} entradas, {} consultas ({} casos)", entries.len(), qs.len(), qs.len() * 2);
}
