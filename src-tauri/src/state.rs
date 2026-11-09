use nova_core::ops::Decision;
use nova_core::undo::UndoEntry;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

/// Trabajos en curso (lecturas de carpeta, copias, estadísticas…), cancelables por id.
#[derive(Default)]
pub struct Jobs {
    pub next_id: AtomicU64,
    pub flags: Mutex<HashMap<u64, Arc<AtomicBool>>>,
    /// Pausa de las operaciones de archivo que la admiten.
    pub pauses: Mutex<HashMap<u64, Arc<AtomicBool>>>,
    /// Canal por el que la interfaz responde a un conflicto («¿reemplazar?») de un trabajo.
    pub answers: Mutex<HashMap<u64, Sender<(Decision, bool)>>>,
}

pub const MAX_UNDO: usize = 30;

#[derive(Default)]
pub struct AppState {
    pub jobs: Arc<Jobs>,
    /// `true` si el sistema aceptó el efecto Mica en la ventana.
    pub vibrancy: AtomicBool,
    /// Pila de operaciones reversibles. Vive en el backend: la interfaz solo pide «deshacer».
    pub undo: Arc<Mutex<Vec<UndoEntry>>>,
}
