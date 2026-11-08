//! `SqliteStore`: almacén persistente del índice (rusqlite, SQLite incluido en el binario).
//!
//! Esta capa solo traduce a llamadas de rusqlite las sentencias de `sqlgen`, que se validan contra SQLite real con
//! `scripts/index_xcheck.py`. Dos conexiones sobre el mismo archivo en modo WAL: una de escritura (lotes del indexador, estado) y una de
//! solo lectura (búsquedas), de modo que buscar no espera al indexado ni al revés.
//!
//! NOTA: este archivo requiere `rusqlite` y no pudo compilarse en el entorno donde se escribió (sin acceso a crates.io).

use super::classify::{path_key, subtree_range};
use super::model::*;
use super::sqlgen::{self, P};
use super::store::{IndexStore, MAX_ISSUES};
use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Connection, ErrorCode, OpenFlags};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub struct SqliteStore {
    writer: Mutex<Connection>,
    reader: Mutex<Connection>,
    fts: bool,
}

fn e2s(e: rusqlite::Error) -> String {
    e.to_string()
}

fn is_corrupt(e: &rusqlite::Error) -> bool {
    matches!(e, rusqlite::Error::SqliteFailure(f, _) if matches!(f.code, ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase))
}

impl SqliteStore {
    /// Abre (o crea) el índice. Un archivo corrupto, de otra versión o que no es una base de datos se descarta y se recrea:
    /// el índice siempre se puede reconstruir recorriendo las unidades.
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("No se pudo crear la carpeta del índice: {e}"))?;
        }
        match Self::open_inner(path) {
            Ok(s) => Ok(s),
            Err(e) if is_corrupt(&e) => {
                for ext in ["", "-wal", "-shm"] {
                    let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
                }
                Self::open_inner(path).map_err(e2s)
            }
            Err(e) => Err(e2s(e)),
        }
    }

    fn open_inner(path: &Path) -> rusqlite::Result<Self> {
        let w = Connection::open(path)?;
        w.busy_timeout(Duration::from_secs(5))?;
        w.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA temp_store=MEMORY; PRAGMA cache_size=-32768;")?;
        let v: i64 = w.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if v != 0 && v != sqlgen::SCHEMA_VERSION {
            w.execute_batch("DROP TABLE IF EXISTS entries_fts; DROP TABLE IF EXISTS entries; DROP TABLE IF EXISTS roots; DROP TABLE IF EXISTS issues;")?;
        }
        w.execute_batch(sqlgen::SCHEMA)?;
        // FTS5 con trigramas es una optimización: si este SQLite no la trae, las consultas usan `instr()`.
        let _ = w.execute_batch(sqlgen::FTS_SCHEMA);
        let fts: bool = w.query_row("SELECT count(*) FROM sqlite_master WHERE name='entries_fts'", [], |r| r.get::<_, i64>(0)).map(|n| n > 0).unwrap_or(false);
        w.pragma_update(None, "user_version", sqlgen::SCHEMA_VERSION)?;
        let r = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
        r.busy_timeout(Duration::from_secs(5))?;
        r.execute_batch("PRAGMA cache_size=-65536;")?;
        Ok(SqliteStore { writer: Mutex::new(w), reader: Mutex::new(r), fts })
    }

    pub fn has_fts(&self) -> bool {
        self.fts
    }
}

fn state_of(s: String) -> RootState {
    RootState::parse(&s)
}

impl IndexStore for SqliteStore {
    fn ensure_root(&self, path: &str) -> StoreResult<i64> {
        let c = self.writer.lock().unwrap();
        let k = path_key(path);
        let mut st = c.prepare("SELECT id, path FROM roots").map_err(e2s)?;
        let found = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))).map_err(e2s)?.filter_map(Result::ok).find(|(_, p)| path_key(p) == k);
        drop(st);
        if let Some((id, _)) = found {
            return Ok(id);
        }
        c.execute("INSERT INTO roots(path) VALUES (?1)", params![path]).map_err(e2s)?;
        Ok(c.last_insert_rowid())
    }

    fn roots(&self) -> StoreResult<Vec<RootRecord>> {
        let c = self.writer.lock().unwrap();
        let mut st = c.prepare("SELECT id, path, state, online, gen, files, dirs, skipped, last_complete FROM roots ORDER BY id").map_err(e2s)?;
        let rows = st
            .query_map([], |r| {
                Ok(RootRecord {
                    id: r.get(0)?,
                    path: r.get(1)?,
                    state: state_of(r.get(2)?),
                    online: r.get::<_, i64>(3)? != 0,
                    gen: r.get(4)?,
                    files: r.get::<_, i64>(5)?.max(0) as u64,
                    dirs: r.get::<_, i64>(6)?.max(0) as u64,
                    skipped: r.get::<_, i64>(7)?.max(0) as u64,
                    last_complete: r.get(8)?,
                })
            })
            .map_err(e2s)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(e2s)
    }

    fn begin_scan(&self, root_id: i64) -> StoreResult<i64> {
        let c = self.writer.lock().unwrap();
        c.execute("UPDATE roots SET gen = gen + 1, state = 'scanning' WHERE id = ?1", params![root_id]).map_err(e2s)?;
        c.query_row("SELECT gen FROM roots WHERE id = ?1", params![root_id], |r| r.get(0)).map_err(e2s)
    }

    fn upsert(&self, root_id: i64, gen: i64, batch: &[IndexedEntry]) -> StoreResult<()> {
        let mut c = self.writer.lock().unwrap();
        let tx = c.transaction().map_err(e2s)?;
        {
            let mut st = tx.prepare_cached(sqlgen::UPSERT).map_err(e2s)?;
            for e in batch {
                st.execute(params![
                    root_id,
                    e.key,
                    e.path,
                    e.name,
                    e.name_lc,
                    e.ext,
                    e.is_dir as i64,
                    e.size.min(i64::MAX as u64) as i64,
                    e.modified,
                    e.created,
                    e.accessed,
                    e.depth as i64,
                    gen
                ])
                .map_err(e2s)?;
            }
        }
        tx.commit().map_err(e2s)
    }

    fn finish_scan(&self, root_id: i64, gen: i64, skipped: u64, now_ms: i64) -> StoreResult<u64> {
        let mut c = self.writer.lock().unwrap();
        let tx = c.transaction().map_err(e2s)?;
        let removed = tx.execute(sqlgen::SWEEP, params![root_id, gen]).map_err(e2s)? as u64;
        let (files, dirs): (i64, i64) = tx
            .query_row("SELECT COALESCE(SUM(kind = 0), 0), COALESCE(SUM(kind = 1), 0) FROM entries WHERE root_id = ?1", params![root_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(e2s)?;
        tx.execute("UPDATE roots SET state='complete', files=?2, dirs=?3, skipped=?4, last_complete=?5 WHERE id=?1", params![root_id, files, dirs, skipped.min(i64::MAX as u64) as i64, now_ms]).map_err(e2s)?;
        tx.commit().map_err(e2s)?;
        Ok(removed)
    }

    fn interrupt_scan(&self, root_id: i64) -> StoreResult<()> {
        let c = self.writer.lock().unwrap();
        c.execute("UPDATE roots SET state='interrupted' WHERE id=?1", params![root_id]).map_err(e2s)?;
        Ok(())
    }

    fn set_online(&self, root_id: i64, online: bool) -> StoreResult<()> {
        let c = self.writer.lock().unwrap();
        c.execute("UPDATE roots SET online=?2 WHERE id=?1", params![root_id, online as i64]).map_err(e2s)?;
        Ok(())
    }

    fn remove_root(&self, root_id: i64) -> StoreResult<()> {
        let mut c = self.writer.lock().unwrap();
        let tx = c.transaction().map_err(e2s)?;
        tx.execute("DELETE FROM entries WHERE root_id=?1", params![root_id]).map_err(e2s)?;
        tx.execute("DELETE FROM issues WHERE root_id=?1", params![root_id]).map_err(e2s)?;
        tx.execute("DELETE FROM roots WHERE id=?1", params![root_id]).map_err(e2s)?;
        tx.commit().map_err(e2s)
    }

    fn clear_all(&self) -> StoreResult<()> {
        // Borrar fila a fila con los disparadores de FTS tardaría minutos con millones de filas: se recrean las tablas (la meta se conserva).
        let c = self.writer.lock().unwrap();
        c.execute_batch(
            "BEGIN;
             DROP TRIGGER IF EXISTS entries_ai; DROP TRIGGER IF EXISTS entries_ad; DROP TRIGGER IF EXISTS entries_au;
             DROP TABLE IF EXISTS entries_fts; DROP TABLE IF EXISTS entries; DELETE FROM roots; DELETE FROM issues;
             COMMIT;",
        )
        .map_err(e2s)?;
        c.execute_batch(sqlgen::SCHEMA).map_err(e2s)?;
        if self.fts {
            c.execute_batch(sqlgen::FTS_SCHEMA).map_err(e2s)?;
        }
        Ok(())
    }

    fn remove_tree(&self, path: &str) -> StoreResult<u64> {
        let c = self.writer.lock().unwrap();
        let (a, z) = subtree_range(path);
        Ok(c.execute(sqlgen::REMOVE_TREE, params![path_key(path), a, z]).map_err(e2s)? as u64)
    }

    fn log_issue(&self, root_id: i64, path: &str, message: &str) -> StoreResult<()> {
        let c = self.writer.lock().unwrap();
        c.execute("INSERT INTO issues(root_id, path, message) VALUES (?1, ?2, ?3)", params![root_id, path, message]).map_err(e2s)?;
        c.execute(
            "DELETE FROM issues WHERE root_id=?1 AND id <= COALESCE((SELECT id FROM issues WHERE root_id=?1 ORDER BY id DESC LIMIT 1 OFFSET ?2), 0)",
            params![root_id, MAX_ISSUES as i64],
        )
        .map_err(e2s)?;
        Ok(())
    }

    fn issues(&self, limit: usize) -> StoreResult<Vec<ScanIssue>> {
        let c = self.writer.lock().unwrap();
        let mut st = c.prepare("SELECT path, message FROM issues ORDER BY id DESC LIMIT ?1").map_err(e2s)?;
        let rows = st.query_map(params![limit as i64], |r| Ok(ScanIssue { path: r.get(0)?, message: r.get(1)? })).map_err(e2s)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(e2s)
    }

    fn query(&self, q: &Query, cancel: &Arc<AtomicBool>) -> StoreResult<QueryPage> {
        let t0 = Instant::now();
        let built = sqlgen::build_query(q, self.fts);
        let limit = q.effective_limit();
        let c = self.reader.lock().unwrap();
        let flag = cancel.clone();
        // SQLite llama a esto cada 2000 instrucciones de su máquina virtual: devolver `true` aborta la consulta en curso.
        c.progress_handler(2000, Some(move || flag.load(Ordering::Relaxed)));
        let res: rusqlite::Result<Vec<Hit>> = (|| {
            let mut st = c.prepare_cached(&built.sql)?;
            let vals: Vec<Value> = built.params.iter().map(|p| match p {
                P::I(i) => Value::Integer(*i),
                P::T(s) => Value::Text(s.clone()),
            }).collect();
            let rows = st.query_map(params_from_iter(vals), |r| {
                Ok(Hit {
                    path: r.get(0)?,
                    name: r.get(1)?,
                    ext: r.get(2)?,
                    is_dir: r.get::<_, i64>(3)? == 1,
                    size: r.get::<_, i64>(4)?.max(0) as u64,
                    modified: r.get(5)?,
                    created: r.get(6)?,
                })
            })?;
            rows.collect()
        })();
        c.progress_handler(2000, None::<fn() -> bool>);
        let elapsed_ms = t0.elapsed().as_millis() as u64;
        match res {
            Ok(mut hits) => {
                let more = hits.len() > limit;
                hits.truncate(limit);
                Ok(QueryPage { hits, more, cancelled: false, elapsed_ms })
            }
            Err(rusqlite::Error::SqliteFailure(f, _)) if f.code == ErrorCode::OperationInterrupted => Ok(QueryPage { hits: vec![], more: false, cancelled: true, elapsed_ms }),
            Err(e) => Err(e2s(e)),
        }
    }

    fn counts(&self) -> StoreResult<(u64, u64)> {
        let c = self.writer.lock().unwrap();
        c.query_row("SELECT COALESCE(SUM(files),0), COALESCE(SUM(dirs),0) FROM roots WHERE online=1", [], |r| Ok((r.get::<_, i64>(0)?.max(0) as u64, r.get::<_, i64>(1)?.max(0) as u64))).map_err(e2s)
    }

    fn get_meta(&self, key: &str) -> StoreResult<Option<String>> {
        let c = self.writer.lock().unwrap();
        match c.query_row("SELECT value FROM meta WHERE key=?1", params![key], |r| r.get::<_, String>(0)) {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e2s(e)),
        }
    }

    fn set_meta(&self, key: &str, value: &str) -> StoreResult<()> {
        let c = self.writer.lock().unwrap();
        c.execute("INSERT INTO meta(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key, value]).map_err(e2s)?;
        Ok(())
    }
}
