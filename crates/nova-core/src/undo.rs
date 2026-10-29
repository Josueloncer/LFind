//! Historial de operaciones reversibles.
//!
//! Solo se registra lo que se puede deshacer sin perder datos:
//!  * renombrar y mover → se devuelve al lugar original (si sigue libre);
//!  * copiar y crear   → las copias/elementos creados se envían a la Papelera (recuperables).
//! Eliminar permanentemente NO es reversible; enviar a la Papelera se recupera desde la propia Papelera.

use crate::ops::{self, ErrKind, OpError};
use crate::safety;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub enum UndoEntry {
    Rename { from: PathBuf, to: PathBuf },
    Move { pairs: Vec<(PathBuf, PathBuf)> },
    Copy { created: Vec<PathBuf> },
    Create { path: PathBuf },
}

impl UndoEntry {
    pub fn describe(&self) -> String {
        match self {
            UndoEntry::Rename { .. } => "cambio de nombre".into(),
            UndoEntry::Move { pairs } => format!("movimiento de {} elemento(s)", pairs.len()),
            UndoEntry::Copy { created } => format!("copia de {} elemento(s)", created.len()),
            UndoEntry::Create { .. } => "creación de elemento".into(),
        }
    }
}

#[derive(Debug, Default)]
pub struct UndoReport {
    pub restored: u64,
    pub errors: Vec<OpError>,
}

fn fail(r: &mut UndoReport, p: &Path, msg: &str, kind: ErrKind) {
    r.errors.push(OpError { path: p.display().to_string(), message: msg.into(), kind });
}

/// Quita rutas que están dentro de otra ya incluida (basta con enviar la carpeta superior).
fn prune_nested(paths: &[PathBuf]) -> Vec<PathBuf> {
    paths
        .iter()
        .filter(|p| !paths.iter().any(|q| !safety::same_path(p, q) && safety::contains(q, p)))
        .cloned()
        .collect()
}

pub fn apply(entry: &UndoEntry, trash: &dyn Fn(&Path) -> Result<(), String>) -> UndoReport {
    let mut r = UndoReport::default();
    match entry {
        UndoEntry::Rename { from, to } => {
            if fs::symlink_metadata(to).is_err() {
                fail(&mut r, to, "El elemento ya no existe con ese nombre.", ErrKind::NotFound);
            } else if fs::symlink_metadata(from).is_ok() && !safety::same_path(from, to) {
                fail(&mut r, from, "El nombre original ya está ocupado.", ErrKind::Exists);
            } else if let Err(m) = safety::check_destructive(to) {
                fail(&mut r, to, &m, ErrKind::Blocked);
            } else {
                match ops::rename_raw(to, from) {
                    Ok(()) => r.restored += 1,
                    Err(e) => r.errors.push(ops::op_error(to, &e)),
                }
            }
        }
        UndoEntry::Move { pairs } => {
            for (orig, cur) in pairs.iter().rev() {
                if fs::symlink_metadata(cur).is_err() {
                    fail(&mut r, cur, "El elemento ya no está donde se movió.", ErrKind::NotFound);
                } else if fs::symlink_metadata(orig).is_ok() {
                    fail(&mut r, orig, "La ubicación original ya está ocupada.", ErrKind::Exists);
                } else if let Err(m) = safety::check_destructive(cur) {
                    fail(&mut r, cur, &m, ErrKind::Blocked);
                } else {
                    if let Some(parent) = orig.parent() {
                        let _ = fs::create_dir_all(parent);
                    }
                    match ops::move_single(cur, orig) {
                        Ok(()) => r.restored += 1,
                        Err(m) => fail(&mut r, cur, &m, ErrKind::Other),
                    }
                }
            }
        }
        UndoEntry::Copy { created } => trash_all(&prune_nested(created), trash, &mut r),
        UndoEntry::Create { path } => trash_all(std::slice::from_ref(path), trash, &mut r),
    }
    r
}

fn trash_all(paths: &[PathBuf], trash: &dyn Fn(&Path) -> Result<(), String>, r: &mut UndoReport) {
    for p in paths {
        if fs::symlink_metadata(p).is_err() {
            continue; // ya no existe: nada que deshacer
        }
        match trash(p) {
            Ok(()) => r.restored += 1,
            Err(m) => fail(r, p, &m, ErrKind::Other),
        }
    }
}
