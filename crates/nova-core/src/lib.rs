//! Núcleo de LFind (antes NOVA Explorer).
//!
//! No depende de Tauri: todo lo que hay aquí se puede probar con `cargo test`
//! y reutilizar desde una CLI, un servicio de indexación o tests de benchmark.

pub mod archive;
pub mod b64;
pub mod clipboard;
#[cfg(any(windows, nova_winck))]
pub mod clipboard_win;
pub mod dir;
pub mod icons;
#[cfg(any(windows, nova_winck))]
pub mod icons_win;
pub mod names;
pub mod newmenu;
pub mod ops;
pub mod pathbar;
pub mod places;
pub mod preview;
pub mod prefs;
pub mod props;
pub mod safety;
pub mod search;
pub mod sendto;
pub mod shell;
pub mod sys;
pub mod thumbs;
#[cfg(any(windows, nova_winck))]
pub mod thumbs_win;
pub mod undo;
pub mod volumes;
