//! Capa de aplicación de TickForge.
//!
//! Define los puertos (traits) que implementan los adaptadores y los casos de uso
//! que orquestan el dominio. Solo depende de `tickforge-domain`.

pub mod error;
pub mod ports;
pub mod use_cases;

pub use error::ApplicationError;
