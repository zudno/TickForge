//! Herramientas de resiliencia compartidas por los adaptadores y la presentación.
//!
//! Son funciones puras (sin `tokio`): calculan cuánto esperar, y quien las usa decide cómo
//! esperar.

mod exponential_backoff;

pub use exponential_backoff::ExponentialBackoff;
