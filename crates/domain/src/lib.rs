//! Núcleo del dominio de TickForge.
//!
//! Contiene entidades, value objects, máquinas de estado y reglas de negocio puras.
//! Este crate NO debe depender de tokio, reqwest, serde_json ni de ningún otro crate de I/O.

pub mod error;
pub mod shared;

pub use error::DomainError;
