//! Pruebas del motor con árboles reales en disco y el almacén en memoria.
use super::engine::StartOpts;
use super::scanner::{scan_root, ScanCtl};
use super::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

static SEQ: AtomicU64 = AtomicU64::new(0);

struct Tmp(PathBuf);
impl Tmp {
    fn new() -> Tmp {
        let p = std::env::temp_dir().join(format!("lfind-idx-{}-{}", std::process::id(), SEQ.fetch_add(1, Ordering::SeqCst)));
        fs::create_dir_all(&p).unwrap();
        Tmp(p)
    }
    fn p(&self, rel: &str) -> PathBuf {
        self.0.join(rel)
    }
    fn file(&self, rel: &str, bytes: usize) -> PathBuf {
        let p = self.p(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, vec![b'x'; bytes]).unwrap();
        p
    }
    fn root(&self) -> String {
        self.0.to_string_lossy().into_owned()
    }
}
impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn nocancel() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}
fn opts() -> StartOpts {
    StartOpts { yield_ms: 0, ..Default::default() }
}
fn none() -> ExcludeRules {
    ExcludeRules::default()
}
fn wait(ix: &Indexer) {
    let t = Instant::now();
    while ix.is_running() {
        assert!(t.elapsed() < Duration::from_secs(60), "el indexado no terminó");
        std::thread::sleep(Duration::from_millis(5));
    }
    ix.stop(); // recoge el hilo
}
fn index(ix: &Indexer, roots: &[&str], force: bool) {
    assert!(ix.start(roots.iter().map(|s| s.to_string()).collect(), none(), StartOpts { force, ..opts() }).unwrap());
    wait(ix);
}
fn names(ix: &Indexer, q: Query) -> Vec<String> {
    ix.search(&q, &nocancel()).unwrap().hits.into_iter().map(|h| h.name).collect()
}
fn q(text: &str) -> Query {
    Query { text: text.into(), ..Default::default() }
}
fn new_ix() -> Indexer {
    Indexer::new(Arc::new(MemStore::new()))
}

#[test]
fn initial_index_finds_files_and_folders() {
    let t = Tmp::new();
    t.file("docs/Informe Final.pdf", 10);
    t.file("docs/notas.txt", 5);
    t.file("fotos/vacaciones/playa.png", 7);
    let ix = new_ix();
    index(&ix, &[&t.root()], false);
    let st = ix.status().unwrap();
    assert_eq!((st.total_files, st.total_dirs), (3, 3));
    assert_eq!(st.phase, Phase::Idle);
    assert_eq!(st.roots[0].state, RootState::Complete);
    assert_eq!(names(&ix, q("informe")), vec!["Informe Final.pdf"]);
    assert_eq!(names(&ix, q("INFORME final")), vec!["Informe Final.pdf"], "varias palabras, sin distinguir mayúsculas, en cualquier orden");
    assert_eq!(names(&ix, q("vacac")), vec!["vacaciones"], "consulta parcial; encuentra carpetas");
    assert!(names(&ix, q("inexistente")).is_empty());
}

#[test]
fn rescan_does_not_duplicate() {
    let t = Tmp::new();
    t.file("a/uno.txt", 1);
    t.file("a/dos.txt", 1);
    let ix = new_ix();
    index(&ix, &[&t.root()], false);
    index(&ix, &[&t.root()], true);
    index(&ix, &[&t.root()], true);
    let st = ix.status().unwrap();
    assert_eq!((st.total_files, st.total_dirs), (2, 1));
    assert_eq!(names(&ix, q("uno")).len(), 1);
}

#[test]
fn incremental_added_deleted_renamed_moved() {
    let t = Tmp::new();
    t.file("a/viejo.txt", 1);
    t.file("a/borrar.txt", 1);
    t.file("carpeta/dentro/hijo.txt", 1);
    let ix = new_ix();
    index(&ix, &[&t.root()], false);

    t.file("a/nuevo.txt", 1); // alta
    fs::remove_file(t.p("a/borrar.txt")).unwrap(); // baja
    fs::rename(t.p("a/viejo.txt"), t.p("a/renombrado.txt")).unwrap(); // renombrado
    fs::create_dir_all(t.p("destino")).unwrap();
    fs::rename(t.p("carpeta"), t.p("destino/carpeta")).unwrap(); // movimiento de un árbol
    index(&ix, &[&t.root()], true);

    assert_eq!(names(&ix, q("nuevo")), vec!["nuevo.txt"]);
    assert!(names(&ix, q("borrar")).is_empty());
    assert!(names(&ix, q("viejo")).is_empty());
    assert_eq!(names(&ix, q("renombrado")), vec!["renombrado.txt"]);
    let hit = ix.search(&q("hijo"), &nocancel()).unwrap().hits;
    assert_eq!(hit.len(), 1);
    assert!(hit[0].path.replace('\\', "/").contains("destino/carpeta/dentro/hijo.txt"));
    let st = ix.status().unwrap();
    assert_eq!(st.total_files, 3); // nuevo, renombrado, hijo
}

#[test]
fn modified_size_is_refreshed() {
    let t = Tmp::new();
    let f = t.file("dato.bin", 10);
    let ix = new_ix();
    index(&ix, &[&t.root()], false);
    assert_eq!(ix.search(&q("dato"), &nocancel()).unwrap().hits[0].size, 10);
    fs::write(&f, vec![0u8; 999]).unwrap();
    index(&ix, &[&t.root()], true);
    assert_eq!(ix.search(&q("dato"), &nocancel()).unwrap().hits[0].size, 999);
}

#[test]
fn combined_filters() {
    let t = Tmp::new();
    t.file("diseno/logo.psd", 5000);
    t.file("diseno/logo.png", 100);
    t.file("diseno/banner.psd", 50);
    t.file("trabajo/informe.pdf", 300);
    t.file("trabajo/informe viejo.pdf", 300);
    t.file("trabajo/reporte.docx", 2000);
    fs::create_dir_all(t.p("trabajo/logo-carpeta")).unwrap();
    let ix = new_ix();
    index(&ix, &[&t.root()], false);

    assert_eq!(names(&ix, Query { exts: vec!["psd".into()], ..Default::default() }).len(), 2, "todos los PSD");
    assert_eq!(names(&ix, Query { category: Some("image".into()), ..Default::default() }), vec!["logo.png"]);
    assert_eq!(names(&ix, Query { text: "logo".into(), exts: vec![".PSD".into()], min_size: Some(1000), ..Default::default() }), vec!["logo.psd"]);
    assert_eq!(names(&ix, Query { min_size: Some(1000), sort: SortBy::Size, desc: true, ..Default::default() }), vec!["logo.psd", "reporte.docx"]);
    assert_eq!(names(&ix, Query { max_size: Some(100), kind: KindFilter::File, sort: SortBy::Name, ..Default::default() }), vec!["banner.psd", "logo.png"]);
    assert_eq!(names(&ix, Query { text: "logo".into(), kind: KindFilter::Dir, ..Default::default() }), vec!["logo-carpeta"], "carpetas con determinado nombre");
    assert_eq!(names(&ix, Query { category: Some("folder".into()), sort: SortBy::Name, ..Default::default() }), vec!["diseno", "logo-carpeta", "trabajo"]);
    // Exacta: «informe» no es «informe viejo.pdf»; con extensión también vale.
    assert_eq!(names(&ix, Query { text: "informe".into(), exact: true, ..Default::default() }), vec!["informe.pdf"]);
    assert_eq!(names(&ix, Query { text: "INFORME.PDF".into(), exact: true, ..Default::default() }), vec!["informe.pdf"]);
    assert_eq!(names(&ix, q("informe")).len(), 2, "parcial");
    // Carpeta.
    let f = t.p("trabajo").to_string_lossy().into_owned();
    assert_eq!(names(&ix, Query { folder: Some(f), kind: KindFilter::File, ..Default::default() }).len(), 3);
    // Fechas: todo se creó ahora; un rango futuro no devuelve nada y uno que incluye «ahora» devuelve todo.
    let now = engine::now_ms();
    let hours = 3_600_000;
    assert!(names(&ix, Query { modified: DateRange { after: Some(now + hours), before: None }, ..Default::default() }).is_empty());
    assert_eq!(names(&ix, Query { kind: KindFilter::File, modified: DateRange { after: Some(now - hours), before: Some(now + hours) }, ..Default::default() }).len(), 6);
    assert!(names(&ix, Query { modified: DateRange { before: Some(now - 365 * 24 * hours), after: None }, ..Default::default() }).is_empty(), "documentos modificados hace más de un año: ninguno");
}

#[test]
fn relevance_ordering() {
    let t = Tmp::new();
    t.file("m/foto.jpg", 1); // nombre igual (sin extensión)
    t.file("m/foto vacaciones.jpg", 1); // empieza por
    t.file("m/mi foto.jpg", 1); // palabra
    t.file("m/fotografia.jpg", 1); // empieza por
    t.file("m/xfotox.jpg", 1); // contiene
    let ix = new_ix();
    index(&ix, &[&t.root()], false);
    assert_eq!(names(&ix, q("foto")), vec!["foto.jpg", "fotografia.jpg", "foto vacaciones.jpg", "mi foto.jpg", "xfotox.jpg"]);
}

#[test]
fn multiple_roots_and_offline() {
    let (a, b) = (Tmp::new(), Tmp::new());
    a.file("x/comun.txt", 1);
    b.file("y/comun.txt", 1);
    b.file("y/solo-b.txt", 1);
    let ix = new_ix();
    index(&ix, &[&a.root(), &b.root()], false);
    assert_eq!(names(&ix, q("comun")).len(), 2);
    assert_eq!(names(&ix, Query { text: "comun".into(), drives: vec![a.root()], ..Default::default() }).len(), 1, "filtro por unidad");
    assert_eq!(names(&ix, Query { text: "solo".into(), drives: vec![a.root()], ..Default::default() }).len(), 0);
    // Una unidad que se desconecta deja de aparecer, pero su índice se conserva y vuelve sin reindexar.
    let gone = b.root();
    index(&ix, &[&a.root(), &gone], false);
    assert_eq!(names(&ix, q("solo")).len(), 1);
    index(&ix, &[&a.root(), "/ruta/que/no/existe-lfind"], false);
    let st = ix.status().unwrap();
    assert!(st.roots.iter().any(|r| !r.online));
    index(&ix, &[&a.root()], false); // b ya no se pide
    assert!(names(&ix, q("solo")).is_empty(), "b oculta");
    let id_b = st.roots.iter().find(|r| r.path == gone).unwrap().id;
    index(&ix, &[&a.root(), &gone], false);
    assert_eq!(names(&ix, q("solo")).len(), 1, "b vuelve");
    assert_eq!(ix.status().unwrap().roots.iter().find(|r| r.path == gone).unwrap().id, id_b, "misma raíz, sin duplicar");
}

#[test]
fn exclusions() {
    let t = Tmp::new();
    t.file("Windows/sys.dll", 1);
    t.file("proyecto/Windows/propio.txt", 1);
    t.file("proyecto/node_modules/pkg/index.js", 1);
    t.file("privado/secreto.txt", 1);
    t.file("proyecto/cache.tmp", 1);
    t.file("proyecto/bueno.txt", 1);
    let rules = ExcludeRules::new(vec![t.p("privado").to_string_lossy().into_owned()], vec!["node_modules".into()], vec!["Windows".into()], vec!["tmp".into()]);
    let ix = new_ix();
    assert!(ix.start(vec![t.root()], rules, opts()).unwrap());
    wait(&ix);
    assert!(names(&ix, q("sys")).is_empty(), "Windows en la raíz excluida");
    assert_eq!(names(&ix, q("propio")).len(), 1, "Windows en un subnivel no se excluye");
    assert!(names(&ix, q("index")).is_empty());
    assert!(names(&ix, q("secreto")).is_empty());
    assert!(names(&ix, q("cache")).is_empty());
    assert_eq!(names(&ix, q("bueno")).len(), 1);
}

#[test]
fn unreadable_root_is_logged_not_fatal() {
    let t = Tmp::new();
    t.file("ok/bien.txt", 1);
    let ix = new_ix();
    // La raíz inexistente se marca desconectada; la buena se indexa igualmente.
    index(&ix, &["/no/existe/lfind-xyz", &t.root()], false);
    assert_eq!(names(&ix, q("bien")).len(), 1);
    // Directorio ilegible (si el usuario de las pruebas no es administrador).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let locked = t.p("cerrado");
        fs::create_dir_all(&locked).unwrap();
        fs::write(locked.join("dentro.txt"), b"x").unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        if fs::read_dir(&locked).is_err() {
            index(&ix, &[&t.root()], true);
            let st = ix.status().unwrap();
            assert!(st.issues.iter().any(|i| i.path.ends_with("cerrado")), "el error de acceso queda registrado");
            assert!(st.skipped >= 1);
            assert_eq!(names(&ix, q("bien")).len(), 1, "el resto sigue indexado");
        } else {
            eprintln!("(omitido: se ejecuta con privilegios y puede leer el directorio protegido)");
        }
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn symlink_cycles_do_not_loop() {
    let t = Tmp::new();
    t.file("a/b/archivo.txt", 1);
    std::os::unix::fs::symlink(&t.0, t.p("a/b/ciclo")).unwrap();
    let ix = new_ix();
    index(&ix, &[&t.root()], false);
    assert_eq!(names(&ix, q("archivo")).len(), 1, "no se desciende por el enlace");
    assert_eq!(names(&ix, q("ciclo")).len(), 1, "el enlace sí queda indexado");
}

#[test]
fn restart_keeps_state_and_skips_fresh_roots() {
    let t = Tmp::new();
    t.file("a.txt", 1);
    let store: Arc<MemStore> = Arc::new(MemStore::new());
    {
        let ix = Indexer::new(store.clone());
        index(&ix, &[&t.root()], false);
    }
    // «Reinicio»: nuevo Indexer sobre el mismo almacén. Nada que reindexar si la unidad está completa y reciente.
    let ix2 = Indexer::new(store.clone());
    t.file("b.txt", 1);
    index(&ix2, &[&t.root()], false);
    assert_eq!(names(&ix2, q("a.txt")).len(), 1, "el índice persiste");
    assert!(names(&ix2, q("b.txt")).is_empty(), "no se reescanea una unidad reciente");
    // Pero la reconciliación por antigüedad sí la detecta.
    assert!(ix2.start(vec![t.root()], none(), StartOpts { reconcile_ms: 1, ..opts() }).unwrap());
    std::thread::sleep(Duration::from_millis(5));
    wait(&ix2);
    assert_eq!(names(&ix2, q("b.txt")).len(), 1);
}

#[test]
fn interrupted_scan_recovers() {
    let t = Tmp::new();
    for i in 0..300 {
        t.file(&format!("d{}/f{i}.txt", i % 10), 1);
    }
    let store = Arc::new(MemStore::new());
    let ix = Indexer::new(store.clone());
    index(&ix, &[&t.root()], false);
    let before = ix.status().unwrap().total_files;
    assert_eq!(before, 300);
    // Un archivo desaparece y otro llega; se interrumpe el nuevo indexado a medias.
    fs::remove_file(t.p("d0/f0.txt")).unwrap();
    t.file("d0/nuevo.txt", 1);
    let id = store.ensure_root(&t.root()).unwrap();
    let gen = store.begin_scan(id).unwrap();
    let ctl = ScanCtl::default();
    let mut seen = 0;
    let out = scan_root(store.as_ref(), id, Path::new(&t.root()), gen, &none(), &ctl, 16, 0, &mut |tk| {
        seen = tk.scanned;
        if tk.scanned >= 32 {
            ctl.cancel.store(true, Ordering::SeqCst);
        }
    })
    .unwrap();
    assert!(out.cancelled && seen >= 32);
    store.interrupt_scan(id).unwrap();
    assert_eq!(store.roots().unwrap()[0].state, RootState::Interrupted);
    assert!(ix.search(&q("f5.txt"), &nocancel()).unwrap().hits.len() == 1, "lo ya indexado sigue disponible mientras tanto");
    assert_eq!(names(&ix, q("f0.txt")).len(), 1, "no se barre nada en un indexado incompleto");
    // Cierre brusco: el estado quedó «Scanning». Al arrancar se completa y se barre lo que ya no existe.
    store.begin_scan(id).unwrap();
    let ix2 = Indexer::new(store.clone());
    index(&ix2, &[&t.root()], false);
    assert_eq!(ix2.status().unwrap().roots[0].state, RootState::Complete);
    assert!(names(&ix2, q("f0.txt")).is_empty());
    assert_eq!(names(&ix2, q("nuevo")).len(), 1);
}

#[test]
fn pause_resume_and_stop() {
    let t = Tmp::new();
    for i in 0..200 {
        t.file(&format!("d{}/f{i}.txt", i % 20), 1);
    }
    let ix = new_ix();
    ix.pause(); // sin indexado en marcha no hace nada
    assert_eq!(ix.status().unwrap().phase, Phase::Idle);
    assert!(ix.start(vec![t.root()], none(), StartOpts { yield_ms: 20, batch: 8, ..opts() }).unwrap());
    assert!(!ix.start(vec![t.root()], none(), opts()).unwrap(), "no arranca dos veces");
    ix.pause();
    std::thread::sleep(Duration::from_millis(150));
    let a = ix.status().unwrap();
    assert_eq!(a.phase, Phase::Paused);
    std::thread::sleep(Duration::from_millis(150));
    let b = ix.status().unwrap();
    assert_eq!(a.scanned, b.scanned, "en pausa no avanza");
    ix.resume();
    wait(&ix);
    assert_eq!(ix.status().unwrap().total_files, 200);
    // Detener a mitad deja la raíz interrumpida y se puede retomar.
    let ix = new_ix();
    assert!(ix.start(vec![t.root()], none(), StartOpts { yield_ms: 30, batch: 4, ..opts() }).unwrap());
    std::thread::sleep(Duration::from_millis(60));
    ix.stop();
    assert_eq!(ix.status().unwrap().roots[0].state, RootState::Interrupted);
    index(&ix, &[&t.root()], false);
    assert_eq!(ix.status().unwrap().total_files, 200);
}

#[test]
fn rebuild_clears_and_reindexes() {
    let t = Tmp::new();
    t.file("a.txt", 1);
    let ix = new_ix();
    index(&ix, &[&t.root()], false);
    ix.store().set_meta("clave", "valor").unwrap();
    t.file("b.txt", 1);
    assert!(ix.rebuild(vec![t.root()], none(), opts()).unwrap());
    wait(&ix);
    assert_eq!(ix.status().unwrap().total_files, 2);
}

#[test]
fn remove_tree_for_incremental_events() {
    let t = Tmp::new();
    t.file("a/b/c.txt", 1);
    t.file("a/bb/d.txt", 1);
    let ix = new_ix();
    index(&ix, &[&t.root()], false);
    let removed = ix.store().remove_tree(&t.p("a/b").to_string_lossy()).unwrap();
    assert_eq!(removed, 2, "la carpeta y su contenido; «bb» no es descendiente de «b»");
    assert_eq!(names(&ix, q("d.txt")).len(), 1);
}

#[test]
fn query_cancellation() {
    let store = Arc::new(MemStore::new());
    let id = store.ensure_root("/r").unwrap();
    let gen = store.begin_scan(id).unwrap();
    store.upsert(id, gen, &(0..20_000).map(|i| synthetic(id, i)).collect::<Vec<_>>()).unwrap();
    let ix = Indexer::new(store);
    let c = Arc::new(AtomicBool::new(true));
    let page = ix.search(&q("archivo"), &c).unwrap();
    assert!(page.cancelled && page.hits.is_empty());
}

fn synthetic(root: i64, i: usize) -> IndexedEntry {
    let name = format!("archivo{i:06}.txt");
    let path = format!("/r/d{}/{name}", i % 100);
    let key = classify::path_key(&path);
    IndexedEntry { root_id: root, depth: classify::depth_of(&key), key, path, name_lc: name.to_lowercase(), name, ext: "txt".into(), is_dir: false, size: i as u64, modified: Some(i as i64), created: None, accessed: None }
}

#[test]
fn large_result_sets_page_without_overlap() {
    let store = Arc::new(MemStore::new());
    let id = store.ensure_root("/r").unwrap();
    let gen = store.begin_scan(id).unwrap();
    store.upsert(id, gen, &(0..60_000).map(|i| synthetic(id, i)).collect::<Vec<_>>()).unwrap();
    store.finish_scan(id, gen, 0, 1).unwrap();
    let ix = Indexer::new(store);
    let mut all = std::collections::HashSet::new();
    let mut offset = 0;
    loop {
        let p = ix.search(&Query { text: "archivo".into(), limit: 5000, offset, sort: SortBy::Name, ..Default::default() }, &nocancel()).unwrap();
        for h in &p.hits {
            assert!(all.insert(h.path.clone()), "sin duplicados entre páginas");
        }
        offset += p.hits.len();
        if !p.more {
            break;
        }
    }
    assert_eq!(all.len(), 60_000);
    let p = ix.search(&Query { text: "archivo".into(), limit: 10_000_000, ..Default::default() }, &nocancel()).unwrap();
    assert_eq!(p.hits.len(), 5000, "el límite se acota");
    assert!(p.more);
    assert_eq!(ix.status().unwrap().total_files, 60_000);
}

#[test]
fn model_serializable_defaults() {
    let s = IndexSettings::default();
    assert!(s.enabled && s.drives.is_none() && s.reconcile_minutes == 360);
    assert!(s.rules().dir_excluded("c:/windows", "windows", 1));
    assert_eq!(Query::default().effective_limit(), 500);
}
