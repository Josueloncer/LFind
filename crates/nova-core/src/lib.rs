//! Núcleo de LFind (antes NOVA Explorer).
//!
//! No depende de Tauri: todo lo que hay aquí se puede probar con `cargo test`
//! y reutilizar desde una CLI, un servicio de indexación o tests de benchmark.

pub mod analyzer;
pub mod archive;
pub mod b64;
pub mod backgrounds;
pub mod clipboard;
pub mod cloud;
#[cfg(any(windows, nova_winck))]
pub mod clipboard_win;
pub mod diagnostics;
pub mod dir;
pub mod icons;
pub mod index;
#[cfg(any(windows, nova_winck))]
pub mod icons_win;
pub mod lnk;
pub mod names;
pub mod newmenu;
pub mod ops;
pub mod pathbar;
pub mod places;
pub mod preview;
pub mod prefs;
pub mod props;
pub mod recycle;
pub mod safety;
pub mod search;
pub mod sendto;
pub mod shell;
pub mod sys;
#[cfg(feature = "sqlite")]
pub mod duplicates;
pub mod fonts;
pub mod inspector;
pub mod shellapps;
pub mod terminal;
pub mod thumbs;
pub mod winshell;
#[cfg(any(windows, nova_winck))]
pub mod thumbs_win;
pub mod undo;
pub mod viewer;
pub mod volumes;
