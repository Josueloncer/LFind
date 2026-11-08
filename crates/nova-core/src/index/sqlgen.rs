//! Esquema SQL y generador de consultas del índice. Código puro (sin rusqlite) para poder probarlo en cualquier
//! entorno y validarlo contra SQLite real (`scripts/index_xcheck.py` ejecuta exactamente estas sentencias).

use super::classify::{category_exts, subtree_range};
use super::model::{KindFilter, Query, SortBy};

pub const SCHEMA_VERSION: i64 = 1;

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS roots (
  id INTEGER PRIMARY KEY,
  path TEXT NOT NULL UNIQUE,
  state TEXT NOT NULL DEFAULT 'pending',
  gen INTEGER NOT NULL DEFAULT 0,
  online INTEGER NOT NULL DEFAULT 1,
  files INTEGER NOT NULL DEFAULT 0,
  dirs INTEGER NOT NULL DEFAULT 0,
  skipped INTEGER NOT NULL DEFAULT 0,
  last_complete INTEGER
);
CREATE TABLE IF NOT EXISTS entries (
  id INTEGER PRIMARY KEY,
  root_id INTEGER NOT NULL,
  key TEXT NOT NULL UNIQUE,
  path TEXT NOT NULL,
  name TEXT NOT NULL,
  name_lc TEXT NOT NULL,
  ext TEXT NOT NULL DEFAULT '',
  kind INTEGER NOT NULL,
  size INTEGER NOT NULL DEFAULT 0,
  modified INTEGER,
  created INTEGER,
  accessed INTEGER,
  depth INTEGER NOT NULL,
  gen INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS ix_entries_root_gen ON entries(root_id, gen);
CREATE INDEX IF NOT EXISTS ix_entries_ext ON entries(ext, size);
CREATE INDEX IF NOT EXISTS ix_entries_modified ON entries(modified);
CREATE TABLE IF NOT EXISTS issues (
  id INTEGER PRIMARY KEY,
  root_id INTEGER NOT NULL,
  path TEXT NOT NULL,
  message TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
";

/// Búsqueda por subcadena (trigramas). Si el SQLite en uso no la trae, el almacén cae a `instr()`.
pub const FTS_SCHEMA: &str = "
CREATE VIRTUAL TABLE IF NOT EXISTS entries_fts USING fts5(name_lc, content='entries', content_rowid='id', tokenize='trigram');
CREATE TRIGGER IF NOT EXISTS entries_ai AFTER INSERT ON entries BEGIN
  INSERT INTO entries_fts(rowid, name_lc) VALUES (new.id, new.name_lc);
END;
CREATE TRIGGER IF NOT EXISTS entries_ad AFTER DELETE ON entries BEGIN
  INSERT INTO entries_fts(entries_fts, rowid, name_lc) VALUES ('delete', old.id, old.name_lc);
END;
CREATE TRIGGER IF NOT EXISTS entries_au AFTER UPDATE OF name_lc ON entries WHEN old.name_lc IS NOT new.name_lc BEGIN
  INSERT INTO entries_fts(entries_fts, rowid, name_lc) VALUES ('delete', old.id, old.name_lc);
  INSERT INTO entries_fts(rowid, name_lc) VALUES (new.id, new.name_lc);
END;
";

/// Una fila por entrada; si la clave ya existe se actualiza (sin duplicados) y se marca con la generación del indexado.
pub const UPSERT: &str = "
INSERT INTO entries (root_id, key, path, name, name_lc, ext, kind, size, modified, created, accessed, depth, gen)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
ON CONFLICT(key) DO UPDATE SET
  root_id=excluded.root_id, path=excluded.path, name=excluded.name, name_lc=excluded.name_lc, ext=excluded.ext,
  kind=excluded.kind, size=excluded.size, modified=excluded.modified, created=excluded.created,
  accessed=excluded.accessed, depth=excluded.depth, gen=excluded.gen";

/// Barrido: lo que no se vio en el indexado completo ya no existe (borrado, movido o renombrado).
pub const SWEEP: &str = "DELETE FROM entries WHERE root_id=?1 AND gen<>?2";
/// Quita una ruta y todo lo que cuelga de ella.
pub const REMOVE_TREE: &str = "DELETE FROM entries WHERE key=?1 OR (key>=?2 AND key<?3)";

#[derive(Clone, Debug, PartialEq)]
pub enum P {
    I(i64),
    T(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Built {
    pub sql: String,
    pub params: Vec<P>,
}

/// Lo que la consulta significa una vez normalizada (compartido por `MemStore` y el generador SQL).
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub tokens: Vec<String>,
    pub exact: bool,
    pub exts: Vec<String>,
    pub kind: KindFilter,
    pub sort: SortBy,
    pub desc: bool,
    pub limit: usize,
    pub offset: usize,
    pub min_size: Option<i64>,
    pub max_size: Option<i64>,
}

pub fn plan(q: &Query) -> Plan {
    let text = q.text.trim().to_lowercase();
    let tokens: Vec<String> = if q.exact {
        if text.is_empty() { vec![] } else { vec![text] }
    } else {
        text.split_whitespace().map(|s| s.to_string()).collect()
    };
    let mut exts: Vec<String> = q.exts.iter().map(|e| e.trim().trim_start_matches('.').to_lowercase()).filter(|e| !e.is_empty()).collect();
    let mut kind = q.kind;
    if let Some(c) = q.category.as_deref() {
        if c == "folder" {
            if kind == KindFilter::Any {
                kind = KindFilter::Dir;
            }
        } else {
            exts.extend(category_exts(c).iter().map(|s| s.to_string()));
        }
    }
    exts.sort();
    exts.dedup();
    let clamp = |v: u64| v.min(i64::MAX as u64) as i64;
    let (min_size, max_size) = (q.min_size.map(clamp), q.max_size.map(clamp));
    let (mut sort, mut desc) = (q.sort, q.desc);
    if sort == SortBy::Relevance && tokens.is_empty() {
        // Sin texto no hay relevancia que calcular: lo más útil es lo más grande (si se filtra por tamaño) o lo más reciente.
        sort = if min_size.is_some() || max_size.is_some() { SortBy::Size } else { SortBy::Modified };
        desc = true;
    }
    Plan { tokens, exact: q.exact, exts, kind, sort, desc, limit: q.effective_limit(), offset: q.offset, min_size, max_size }
}

struct B {
    sql: String,
    params: Vec<P>,
}
impl B {
    fn p(&mut self, v: P) -> usize {
        self.params.push(v);
        self.params.len()
    }
}

fn fts_phrase(tok: &str) -> String {
    format!("\"{}\"", tok.replace('"', "\"\""))
}

/// Genera la consulta. `fts`: ¿existe `entries_fts`? Pide `limit + 1` filas para saber si hay más.
pub fn build_query(q: &Query, fts: bool) -> Built {
    let pl = plan(q);
    let mut b = B { sql: String::new(), params: vec![] };
    let mut conds: Vec<String> = vec![];

    // Texto del nombre.
    if pl.exact {
        if let Some(t) = pl.tokens.first() {
            let i = b.p(P::T(t.clone()));
            conds.push(format!("(e.name_lc = ?{i} OR (e.ext <> '' AND e.name_lc = ?{i} || '.' || e.ext))"));
            if fts && t.chars().count() >= 3 {
                let j = b.p(P::T(fts_phrase(t)));
                conds.push(format!("e.id IN (SELECT rowid FROM entries_fts WHERE entries_fts MATCH ?{j})"));
            }
        }
    } else {
        let mut phrases: Vec<String> = vec![];
        for t in &pl.tokens {
            if fts && t.chars().count() >= 3 {
                phrases.push(fts_phrase(t));
            } else {
                let i = b.p(P::T(t.clone()));
                conds.push(format!("instr(e.name_lc, ?{i}) > 0"));
            }
        }
        if !phrases.is_empty() {
            let j = b.p(P::T(phrases.join(" ")));
            conds.push(format!("e.id IN (SELECT rowid FROM entries_fts WHERE entries_fts MATCH ?{j})"));
        }
    }

    if !pl.exts.is_empty() {
        let ph: Vec<String> = pl.exts.iter().map(|e| format!("?{}", b.p(P::T(e.clone())))).collect();
        conds.push(format!("e.ext IN ({})", ph.join(",")));
    }
    match pl.kind {
        KindFilter::File => conds.push("e.kind = 0".into()),
        KindFilter::Dir => conds.push("e.kind = 1".into()),
        KindFilter::Any => {}
    }
    if let Some(v) = pl.min_size {
        let i = b.p(P::I(v));
        conds.push(format!("e.size >= ?{i}"));
    }
    if let Some(v) = pl.max_size {
        let i = b.p(P::I(v));
        conds.push(format!("e.size <= ?{i}"));
    }
    for (col, r) in [("modified", &q.modified), ("created", &q.created), ("accessed", &q.accessed)] {
        if let Some(v) = r.after {
            let i = b.p(P::I(v));
            conds.push(format!("e.{col} >= ?{i}"));
        }
        if let Some(v) = r.before {
            let i = b.p(P::I(v));
            conds.push(format!("e.{col} <= ?{i}"));
        }
    }
    if let Some(ids) = &q.root_ids {
        if ids.is_empty() {
            conds.push("0".into());
        } else {
            let list: Vec<String> = ids.iter().map(|i| i.to_string()).collect();
            conds.push(format!("e.root_id IN ({})", list.join(",")));
        }
    }
    if let Some(f) = q.folder.as_deref().filter(|f| !f.trim().is_empty()) {
        let (a, z) = subtree_range(f);
        let i = b.p(P::T(a));
        let j = b.p(P::T(z));
        conds.push(format!("e.key >= ?{i} AND e.key < ?{j}"));
    }

    b.sql.push_str("SELECT e.path, e.name, e.ext, e.kind, e.size, e.modified, e.created FROM entries e");
    if !conds.is_empty() {
        b.sql.push_str(" WHERE ");
        b.sql.push_str(&conds.join(" AND "));
    }

    // Orden.
    let dir = if pl.desc { "DESC" } else { "ASC" };
    let order = match pl.sort {
        SortBy::Relevance => {
            let t = pl.tokens[0].clone();
            let i = b.p(P::T(t));
            format!(
                "CASE WHEN e.name_lc = ?{i} OR (e.ext <> '' AND e.name_lc = ?{i} || '.' || e.ext) THEN 0 \
                 WHEN instr(e.name_lc, ?{i}) = 1 THEN 1 \
                 WHEN instr(e.name_lc, ' ' || ?{i}) > 0 OR instr(e.name_lc, '_' || ?{i}) > 0 OR instr(e.name_lc, '-' || ?{i}) > 0 OR instr(e.name_lc, '.' || ?{i}) > 0 THEN 2 \
                 ELSE 3 END, length(e.name_lc), e.depth, e.name_lc, e.key"
            )
        }
        SortBy::Name => format!("e.name_lc {dir}, e.key"),
        SortBy::Size => format!("e.size {dir}, e.key"),
        SortBy::Modified => format!("e.modified {dir}, e.key"),
    };
    b.sql.push_str(" ORDER BY ");
    b.sql.push_str(&order);
    let (l, o) = (b.p(P::I(pl.limit as i64 + 1)), b.p(P::I(pl.offset as i64)));
    b.sql.push_str(&format!(" LIMIT ?{l} OFFSET ?{o}"));
    Built { sql: b.sql, params: b.params }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_defaults_and_sort() {
        let q = Query { min_size: Some(1 << 30), ..Default::default() };
        let p = plan(&q);
        assert_eq!((p.sort, p.desc), (SortBy::Size, true));
        let p = plan(&Query::default());
        assert_eq!((p.sort, p.desc, p.limit), (SortBy::Modified, true, 500));
        let p = plan(&Query { text: "  Foto  VACACIONES ".into(), ..Default::default() });
        assert_eq!(p.tokens, vec!["foto", "vacaciones"]);
        assert_eq!(p.sort, SortBy::Relevance);
        let p = plan(&Query { exts: vec![".PNG".into()], category: Some("pdf".into()), ..Default::default() });
        assert_eq!(p.exts, vec!["pdf", "png"], "ordenadas y sin duplicados");
        let p = plan(&Query { category: Some("folder".into()), ..Default::default() });
        assert_eq!(p.kind, KindFilter::Dir);
    }

    #[test]
    fn sql_uses_fts_only_for_three_chars_or_more() {
        let b = build_query(&Query { text: "ab foto".into(), ..Default::default() }, true);
        assert!(b.sql.contains("instr(e.name_lc, ?1) > 0"));
        assert!(b.sql.contains("entries_fts MATCH ?2"));
        assert_eq!(b.params[0], P::T("ab".into()));
        assert_eq!(b.params[1], P::T("\"foto\"".into()));
        let b = build_query(&Query { text: "foto".into(), ..Default::default() }, false);
        assert!(!b.sql.contains("entries_fts"));
        assert!(b.sql.contains("LIMIT") && matches!(b.params.last(), Some(P::I(0))));
    }

    #[test]
    fn sql_quotes_are_escaped_and_params_bound() {
        let b = build_query(&Query { text: "a\"b';--".into(), ..Default::default() }, true);
        assert!(!b.sql.contains("';--"), "el texto nunca va dentro del SQL");
        assert!(b.params.iter().any(|p| *p == P::T("\"a\"\"b';--\"".into())));
    }

    #[test]
    fn empty_roots_match_nothing() {
        let b = build_query(&Query { root_ids: Some(vec![]), ..Default::default() }, true);
        assert!(b.sql.contains("WHERE 0"));
    }
}
