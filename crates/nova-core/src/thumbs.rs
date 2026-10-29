//! (Fase futura) Miniaturas bajo demanda.
//!
//! Contrato previsto, aún sin implementar:
//! * Se solicitan solo para filas visibles; se cancelan al salir de pantalla.
//! * Caché en disco con límite de tamaño y expulsión LRU.
//! * Cola con prioridad y concurrencia limitada.
