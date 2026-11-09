//! Analizador de almacenamiento (Update 10). Independiente del índice de búsqueda: ni lee su base de datos ni escribe en ella.
//!
//! * `walk`: el recorrido, solo metadatos. Se prueba también sin SQLite.
//! * `report`: guarda cada análisis en su propia base de datos y responde las consultas paginadas.

#[cfg(feature = "sqlite")]
pub mod report;
pub mod walk;
