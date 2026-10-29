//! Núcleo de NOVA Explorer.
//!
//! No depende de Tauri: todo lo que hay aquí se puede probar con `cargo test`
//! y reutilizar desde una CLI, un servicio de indexación o tests de benchmark.

pub mod dir;
pub mod names;
pub mod ops;
pub mod places;
pub mod prefs;
pub mod props;
pub mod safety;
pub mod search;
pub mod shell;
pub mod sys;
pub mod thumbs;
pub mod undo;
pub mod volumes;
