//! Contrato del almacén del índice y su implementación en memoria (referencia semántica y pruebas).

use super::model::*;
use super::sqlgen::{self, Plan};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub const MAX_ISSUES: usize = 200;

pub trait IndexStore: Send + Sync {
    /// Devuelve el id de la raíz (la crea si no existe). `path` se compara sin distinguir mayúsculas.
    fn ensure_root(&self, path: &str) -> StoreResult<i64>;
    fn roots(&self) -> StoreResult<Vec<RootRecord>>;
    /// Empieza un indexado: nueva generación y estado `Scanning`. Devuelve la generación.
    fn begin_scan(&self, root_id: i64) -> StoreResult<i64>;
    /// Escribe un lote en una sola transacción (inserta o actualiza por clave: sin duplicados).
    fn upsert(&self, root_id: i64, gen: i64, batch: &[IndexedEntry]) -> StoreResult<()>;
    /// Termina un indexado completo: borra lo no visto, guarda contadores y marca `Complete`. Devuelve cuántas entradas quitó.
    fn finish_scan(&self, root_id: i64, gen: i64, skipped: u64, now_ms: i64) -> StoreResult<u64>;
    /// Marca un indexado como detenido (se retoma luego). No borra nada.
    fn interrupt_scan(&self, root_id: i64) -> StoreResult<()>;
    fn set_online(&self, root_id: i64, online: bool) -> StoreResult<()>;
    fn remove_root(&self, root_id: i64) -> StoreResult<()>;
    fn clear_all(&self) -> StoreResult<()>;
    /// Quita una ruta y sus descendientes (cambios incrementales). Devuelve cuántas entradas quitó.
    fn remove_tree(&self, path: &str) -> StoreResult<u64>;
    fn log_issue(&self, root_id: i64, path: &str, message: &str) -> StoreResult<()>;
    fn issues(&self, limit: usize) -> StoreResult<Vec<ScanIssue>>;
    fn query(&self, q: &Query, cancel: &Arc<AtomicBool>) -> StoreResult<QueryPage>;
    /// (archivos, carpetas) de las raíces conectadas.
    fn counts(&self) -> StoreResult<(u64, u64)>;
    fn get_meta(&self, key: &str) -> StoreResult<Option<String>>;
    fn set_meta(&self, key: &str, value: &str) -> StoreResult<()>;
}

struct Row {
    e: IndexedEntry,
    gen: i64,
}

#[derive(Default)]
struct Inner {
    roots: Vec<RootRecord>,
    next_root: i64,
    rows: HashMap<String, Row>,
    issues: Vec<(i64, ScanIssue)>,
    meta: HashMap<String, String>,
}

#[derive(Default)]
pub struct MemStore {
    inner: Mutex<Inner>,
}

impl MemStore {
    pub fn new() -> Self {
        Self::default()
    }
}

fn rank(name_lc: &str, ext: &str, t: &str) -> u8 {
    if name_lc == t || (!ext.is_empty() && name_lc.len() == t.len() + 1 + ext.len() && name_lc.starts_with(t) && name_lc[t.len()..].starts_with('.') && name_lc.ends_with(ext)) {
        0
    } else if name_lc.starts_with(t) {
        1
    } else if [" ", "_", "-", "."].iter().any(|p| name_lc.contains(&format!("{p}{t}"))) {
        2
    } else {
        3
    }
}

fn matches(e: &IndexedEntry, pl: &Plan, q: &Query, folder: &Option<(String, String)>) -> bool {
    if let Some(ids) = &q.root_ids {
        if !ids.contains(&e.root_id) {
            return false;
        }
    }
    if pl.exact {
        if let Some(t) = pl.tokens.first() {
            let ok = e.name_lc == *t || (!e.ext.is_empty() && e.name_lc == format!("{t}.{}", e.ext));
            if !ok {
                return false;
            }
        }
    } else if !pl.tokens.iter().all(|t| e.name_lc.contains(t.as_str())) {
        return false;
    }
    if !pl.exts.is_empty() && !pl.exts.iter().any(|x| *x == e.ext) {
        return false;
    }
    match pl.kind {
        KindFilter::File if e.is_dir => return false,
        KindFilter::Dir if !e.is_dir => return false,
        _ => {}
    }
    let size = e.size.min(i64::MAX as u64) as i64;
    if pl.min_size.map_or(false, |m| size < m) || pl.max_size.map_or(false, |m| size > m) {
        return false;
    }
    if !q.modified.contains(e.modified) || !q.created.contains(e.created) || !q.accessed.contains(e.accessed) {
        return false;
    }
    if let Some((a, z)) = folder {
        if !(e.key.as_str() >= a.as_str() && e.key.as_str() < z.as_str()) {
            return false;
        }
    }
    true
}

impl IndexStore for MemStore {
    fn ensure_root(&self, path: &str) -> StoreResult<i64> {
        let mut g = self.inner.lock().unwrap();
        let k = super::classify::path_key(path);
        if let Some(r) = g.roots.iter().find(|r| super::classify::path_key(&r.path) == k) {
            return Ok(r.id);
        }
        g.next_root += 1;
        let id = g.next_root;
        g.roots.push(RootRecord { id, path: path.to_string(), state: RootState::Pending, online: true, gen: 0, files: 0, dirs: 0, skipped: 0, last_complete: None });
        Ok(id)
    }
    fn roots(&self) -> StoreResult<Vec<RootRecord>> {
        Ok(self.inner.lock().unwrap().roots.clone())
    }
    fn begin_scan(&self, root_id: i64) -> StoreResult<i64> {
        let mut g = self.inner.lock().unwrap();
        let r = g.roots.iter_mut().find(|r| r.id == root_id).ok_or("raíz desconocida")?;
        r.gen += 1;
        r.state = RootState::Scanning;
        Ok(r.gen)
    }
    fn upsert(&self, root_id: i64, gen: i64, batch: &[IndexedEntry]) -> StoreResult<()> {
        let mut g = self.inner.lock().unwrap();
        for e in batch {
            let mut e = e.clone();
            e.root_id = root_id;
            g.rows.insert(e.key.clone(), Row { e, gen });
        }
        Ok(())
    }
    fn finish_scan(&self, root_id: i64, gen: i64, skipped: u64, now_ms: i64) -> StoreResult<u64> {
        let mut g = self.inner.lock().unwrap();
        let before = g.rows.len();
        g.rows.retain(|_, r| !(r.e.root_id == root_id && r.gen != gen));
        let removed = (before - g.rows.len()) as u64;
        let (files, dirs) = g.rows.values().filter(|r| r.e.root_id == root_id).fold((0u64, 0u64), |(f, d), r| if r.e.is_dir { (f, d + 1) } else { (f + 1, d) });
        let r = g.roots.iter_mut().find(|r| r.id == root_id).ok_or("raíz desconocida")?;
        r.state = RootState::Complete;
        r.files = files;
        r.dirs = dirs;
        r.skipped = skipped;
        r.last_complete = Some(now_ms);
        Ok(removed)
    }
    fn interrupt_scan(&self, root_id: i64) -> StoreResult<()> {
        let mut g = self.inner.lock().unwrap();
        if let Some(r) = g.roots.iter_mut().find(|r| r.id == root_id) {
            r.state = RootState::Interrupted;
        }
        Ok(())
    }
    fn set_online(&self, root_id: i64, online: bool) -> StoreResult<()> {
        let mut g = self.inner.lock().unwrap();
        if let Some(r) = g.roots.iter_mut().find(|r| r.id == root_id) {
            r.online = online;
        }
        Ok(())
    }
    fn remove_root(&self, root_id: i64) -> StoreResult<()> {
        let mut g = self.inner.lock().unwrap();
        g.rows.retain(|_, r| r.e.root_id != root_id);
        g.roots.retain(|r| r.id != root_id);
        g.issues.retain(|(id, _)| *id != root_id);
        Ok(())
    }
    fn clear_all(&self) -> StoreResult<()> {
        let mut g = self.inner.lock().unwrap();
        g.rows.clear();
        g.roots.clear();
        g.issues.clear();
        Ok(())
    }
    fn remove_tree(&self, path: &str) -> StoreResult<u64> {
        let mut g = self.inner.lock().unwrap();
        let k = super::classify::path_key(path);
        let (a, z) = super::classify::subtree_range(path);
        let before = g.rows.len();
        g.rows.retain(|key, _| !(*key == k || (key.as_str() >= a.as_str() && key.as_str() < z.as_str())));
        Ok((before - g.rows.len()) as u64)
    }
    fn log_issue(&self, root_id: i64, path: &str, message: &str) -> StoreResult<()> {
        let mut g = self.inner.lock().unwrap();
        g.issues.push((root_id, ScanIssue { path: path.to_string(), message: message.to_string() }));
        let n = g.issues.iter().filter(|(id, _)| *id == root_id).count();
        if n > MAX_ISSUES {
            if let Some(i) = g.issues.iter().position(|(id, _)| *id == root_id) {
                g.issues.remove(i);
            }
        }
        Ok(())
    }
    fn issues(&self, limit: usize) -> StoreResult<Vec<ScanIssue>> {
        let g = self.inner.lock().unwrap();
        Ok(g.issues.iter().rev().take(limit).map(|(_, i)| i.clone()).collect())
    }
    fn query(&self, q: &Query, cancel: &Arc<AtomicBool>) -> StoreResult<QueryPage> {
        let t0 = Instant::now();
        let pl = sqlgen::plan(q);
        let folder = q.folder.as_deref().filter(|f| !f.trim().is_empty()).map(super::classify::subtree_range);
        let g = self.inner.lock().unwrap();
        let mut found: Vec<&IndexedEntry> = vec![];
        for (i, r) in g.rows.values().enumerate() {
            if i % 4096 == 0 && cancel.load(Ordering::Relaxed) {
                return Ok(QueryPage { hits: vec![], more: false, cancelled: true, elapsed_ms: t0.elapsed().as_millis() as u64 });
            }
            if matches(&r.e, &pl, q, &folder) {
                found.push(&r.e);
            }
        }
        let first = pl.tokens.first().cloned().unwrap_or_default();
        found.sort_by(|a, b| {
            use std::cmp::Ordering::*;
            let dir = |o: std::cmp::Ordering| if pl.desc { o.reverse() } else { o };
            let o_rev = |o: std::cmp::Ordering| o.reverse();
            let main = match pl.sort {
                SortBy::Relevance => rank(&a.name_lc, &a.ext, &first)
                    .cmp(&rank(&b.name_lc, &b.ext, &first))
                    .then(a.name_lc.chars().count().cmp(&b.name_lc.chars().count()))
                    .then(a.depth.cmp(&b.depth))
                    .then(a.name_lc.cmp(&b.name_lc)),
                SortBy::Name => dir(a.name_lc.cmp(&b.name_lc)),
                SortBy::Size => dir(a.size.min(i64::MAX as u64).cmp(&b.size.min(i64::MAX as u64))),
                // Sin fecha, siempre al final (igual que `sqlgen`): en ascendente hay que pedirlo.
                SortBy::Modified if !pl.desc => a.modified.is_none().cmp(&b.modified.is_none()).then(a.modified.cmp(&b.modified)),
                SortBy::Modified => o_rev(a.modified.cmp(&b.modified)),
            };
            match main {
                Equal => a.key.cmp(&b.key),
                o => o,
            }
        });
        let more = found.len() > pl.offset + pl.limit;
        let hits = found
            .into_iter()
            .skip(pl.offset)
            .take(pl.limit)
            .map(|e| Hit { path: e.path.clone(), name: e.name.clone(), ext: e.ext.clone(), is_dir: e.is_dir, size: e.size, modified: e.modified, created: e.created })
            .collect();
        Ok(QueryPage { hits, more, cancelled: false, elapsed_ms: t0.elapsed().as_millis() as u64 })
    }
    fn counts(&self) -> StoreResult<(u64, u64)> {
        // Igual que SqliteStore: suma de los contadores de las raíces conectadas, que se fijan al terminar un indexado.
        let g = self.inner.lock().unwrap();
        Ok(g.roots.iter().filter(|r| r.online).fold((0, 0), |(f, d), r| (f + r.files, d + r.dirs)))
    }
    fn get_meta(&self, key: &str) -> StoreResult<Option<String>> {
        Ok(self.inner.lock().unwrap().meta.get(key).cloned())
    }
    fn set_meta(&self, key: &str, value: &str) -> StoreResult<()> {
        self.inner.lock().unwrap().meta.insert(key.to_string(), value.to_string());
        Ok(())
    }
}
