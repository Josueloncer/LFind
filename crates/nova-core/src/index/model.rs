//! Tipos compartidos del índice.

#[cfg(not(nova_noserde))]
use serde::{Deserialize, Serialize};

pub type StoreResult<T> = Result<T, String>;

#[derive(Clone, Debug, PartialEq)]
pub struct IndexedEntry {
    pub root_id: i64,
    /// Clave única: ruta en minúsculas con `/` como separador (ver `classify::path_key`).
    pub key: String,
    /// Ruta tal como la devuelve el sistema (se muestra y se abre).
    pub path: String,
    pub name: String,
    pub name_lc: String,
    /// Extensión en minúsculas sin punto ("" para carpetas y archivos sin extensión).
    pub ext: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<i64>,
    pub created: Option<i64>,
    pub accessed: Option<i64>,
    pub depth: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(not(nova_noserde), derive(Serialize, Deserialize))]
#[cfg_attr(not(nova_noserde), serde(rename_all = "lowercase"))]
pub enum KindFilter {
    #[default]
    Any,
    File,
    Dir,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(not(nova_noserde), derive(Serialize, Deserialize))]
#[cfg_attr(not(nova_noserde), serde(rename_all = "lowercase"))]
pub enum SortBy {
    #[default]
    Relevance,
    Name,
    Size,
    Modified,
}

/// Rango de fechas en milisegundos desde epoch Unix (ambos extremos incluidos).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(not(nova_noserde), derive(Serialize, Deserialize))]
#[cfg_attr(not(nova_noserde), serde(rename_all = "camelCase", default))]
pub struct DateRange {
    pub after: Option<i64>,
    pub before: Option<i64>,
}

impl DateRange {
    pub fn contains(&self, v: Option<i64>) -> bool {
        if self.after.is_none() && self.before.is_none() {
            return true;
        }
        match v {
            None => false,
            Some(v) => self.after.map_or(true, |a| v >= a) && self.before.map_or(true, |b| v <= b),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(not(nova_noserde), derive(Serialize, Deserialize))]
#[cfg_attr(not(nova_noserde), serde(rename_all = "camelCase", default))]
pub struct Query {
    /// Texto del nombre. Varias palabras = todas deben aparecer (en cualquier orden).
    pub text: String,
    /// `true`: el nombre completo (con o sin extensión) debe ser igual al texto.
    pub exact: bool,
    pub exts: Vec<String>,
    /// image | video | audio | archive | code | doc | sheet | slides | pdf | design | exec | folder
    pub category: Option<String>,
    pub kind: KindFilter,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    pub modified: DateRange,
    pub created: DateRange,
    pub accessed: DateRange,
    /// Raíces indexadas elegidas por el usuario (p. ej. "C:\\"). Vacío = todas las disponibles.
    pub drives: Vec<String>,
    /// Solo dentro de esta carpeta (incluye subcarpetas).
    pub folder: Option<String>,
    pub sort: SortBy,
    pub desc: bool,
    pub limit: usize,
    pub offset: usize,
    /// Lo rellena el motor a partir de `drives` y de las unidades conectadas; la interfaz no lo envía.
    #[cfg_attr(not(nova_noserde), serde(skip))]
    pub root_ids: Option<Vec<i64>>,
}

pub const DEFAULT_LIMIT: usize = 500;
pub const MAX_LIMIT: usize = 5000;

impl Query {
    pub fn effective_limit(&self) -> usize {
        if self.limit == 0 { DEFAULT_LIMIT } else { self.limit.min(MAX_LIMIT) }
    }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(not(nova_noserde), derive(Serialize))]
#[cfg_attr(not(nova_noserde), serde(rename_all = "camelCase"))]
pub struct Hit {
    pub path: String,
    pub name: String,
    pub ext: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<i64>,
    pub created: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(not(nova_noserde), derive(Serialize))]
#[cfg_attr(not(nova_noserde), serde(rename_all = "camelCase"))]
pub struct QueryPage {
    pub hits: Vec<Hit>,
    /// Hay más resultados después de esta página (se pide otra con `offset`).
    pub more: bool,
    pub cancelled: bool,
    pub elapsed_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(nova_noserde), derive(Serialize))]
#[cfg_attr(not(nova_noserde), serde(rename_all = "lowercase"))]
pub enum RootState {
    /// Configurada pero nunca indexada.
    Pending,
    Scanning,
    Complete,
    /// Un indexado se detuvo (cierre de la app, cancelación): se retoma en el siguiente arranque.
    Interrupted,
}

impl RootState {
    pub fn as_str(self) -> &'static str {
        match self {
            RootState::Pending => "pending",
            RootState::Scanning => "scanning",
            RootState::Complete => "complete",
            RootState::Interrupted => "interrupted",
        }
    }
    pub fn parse(s: &str) -> RootState {
        match s {
            "scanning" => RootState::Scanning,
            "complete" => RootState::Complete,
            "interrupted" => RootState::Interrupted,
            _ => RootState::Pending,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(not(nova_noserde), derive(Serialize))]
#[cfg_attr(not(nova_noserde), serde(rename_all = "camelCase"))]
pub struct RootRecord {
    pub id: i64,
    pub path: String,
    pub state: RootState,
    pub online: bool,
    pub gen: i64,
    pub files: u64,
    pub dirs: u64,
    pub skipped: u64,
    /// Milisegundos epoch del último indexado completo.
    pub last_complete: Option<i64>,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(not(nova_noserde), derive(Serialize))]
#[cfg_attr(not(nova_noserde), serde(rename_all = "camelCase"))]
pub struct ScanIssue {
    pub path: String,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(nova_noserde), derive(Serialize))]
#[cfg_attr(not(nova_noserde), serde(rename_all = "lowercase"))]
pub enum Phase {
    Idle,
    Scanning,
    Paused,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(not(nova_noserde), derive(Serialize))]
#[cfg_attr(not(nova_noserde), serde(rename_all = "camelCase"))]
pub struct IndexStatus {
    pub phase: Phase,
    pub current_root: Option<String>,
    /// Entradas vistas en el indexado en curso.
    pub scanned: u64,
    /// 0..1 aproximado; `None` si no hay estimación (primer indexado de la unidad).
    pub progress: Option<f32>,
    pub roots: Vec<RootRecord>,
    pub total_files: u64,
    pub total_dirs: u64,
    pub skipped: u64,
    pub issues: Vec<ScanIssue>,
    pub last_update: Option<i64>,
}

/// Configuración persistente del índice.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(not(nova_noserde), derive(Serialize, Deserialize))]
#[cfg_attr(not(nova_noserde), serde(rename_all = "camelCase", default))]
pub struct IndexSettings {
    pub enabled: bool,
    /// `None`: todas las unidades fijas. `Some(lista)`: solo esas raíces.
    pub drives: Option<Vec<String>>,
    pub excluded_paths: Vec<String>,
    pub excluded_names: Vec<String>,
    pub excluded_root_dirs: Vec<String>,
    pub excluded_exts: Vec<String>,
    /// Cada cuántos minutos se reconcilia una unidad ya indexada (0 = solo al arrancar).
    pub reconcile_minutes: u32,
}

impl Default for IndexSettings {
    fn default() -> Self {
        let d = crate::index::exclude::ExcludeRules::windows_defaults();
        IndexSettings {
            enabled: true,
            drives: None,
            excluded_paths: d.paths,
            excluded_names: d.names,
            excluded_root_dirs: d.root_dirs,
            excluded_exts: d.exts,
            reconcile_minutes: 360,
        }
    }
}

impl IndexSettings {
    pub fn rules(&self) -> crate::index::exclude::ExcludeRules {
        crate::index::exclude::ExcludeRules::new(
            self.excluded_paths.clone(),
            self.excluded_names.clone(),
            self.excluded_root_dirs.clone(),
            self.excluded_exts.clone(),
        )
    }
}
