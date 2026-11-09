//! Analizador de almacenamiento (Update 10). Independiente del índice de búsqueda: ni lee su base de datos ni escribe en ella.
//!
//! * `walk`: el recorrido, solo metadatos. Se prueba también sin SQLite.
//! * `report`: guarda cada análisis en su propia base de datos y responde las consultas paginadas. También actualiza un informe
//!   copiando lo que no cambió y releyendo solo las carpetas afectadas.
//! * `watch`: anota qué carpetas cambian mientras LFind está abierto.

#[cfg(feature = "sqlite")]
pub mod report;
pub mod walk;
pub mod watch;
