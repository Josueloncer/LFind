//! (Fase futura) Búsqueda e indexación incremental.
//!
//! Contrato previsto, aún sin implementar:
//! * Búsqueda por nombre en streaming con cancelación (misma mecánica que `dir`).
//! * Índice incremental persistente, alimentado por el journal NTFS (USN) cuando
//!   esté disponible, con recorrido normal como alternativa.
//! * Nunca bloquea el hilo de la UI y consume un presupuesto de CPU acotado.
