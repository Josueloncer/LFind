//! Motor de búsqueda global: índice persistente de nombres de archivos y carpetas (Update 05, fase 1).
//!
//! Capas (de abajo arriba):
//! * `model`    — tipos compartidos (entradas, consultas, estado). Serializables con `serde` (menos en las pruebas sin dependencias).
//! * `classify` — claves de ruta, extensiones y categorías de archivo.
//! * `exclude`  — reglas de exclusión configurables.
//! * `store`    — contrato `IndexStore` + `MemStore` (referencia semántica y pruebas).
//! * `sqlgen`   — esquema SQL y generador de consultas (puro: sin rusqlite). Se valida contra SQLite real en `scripts/index_xcheck.py`.
//! * `sqlite`   — `SqliteStore` (rusqlite, detrás de la característica `sqlite`): solo traduce `sqlgen` a llamadas.
//! * `scanner`  — recorrido de una unidad/carpeta, por lotes, pausable y cancelable.
//! * `engine`   — `Indexer`: hilo de fondo, estado, pausa/reanudación, reconstrucción, reconciliación y búsqueda.
//!
//! Cfg `nova_noserde`: compila el módulo sin `serde` (solo para pruebas en entornos sin crates.io).

pub mod classify;
pub mod engine;
pub mod exclude;
pub mod model;
pub mod scanner;
pub mod sqlgen;
#[cfg(feature = "sqlite")]
pub mod sqlite;
pub mod store;
#[cfg(test)]
mod tests;

pub use engine::Indexer;
pub use exclude::ExcludeRules;
pub use model::*;
pub use store::{IndexStore, MemStore};
