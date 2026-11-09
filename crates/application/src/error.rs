//! Errores de la capa de aplicación.

use thiserror::Error;
use tickforge_domain::DomainError;
use tickforge_domain::risk::RiskViolation;
use tickforge_domain::trading::OrderId;

use crate::ports::{BrokerError, JournalError, MarketDataError};

/// Error de un caso de uso. Agrupa los errores de dominio y de cada puerto para que la TUI
/// pueda decidir cómo mostrarlos.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ApplicationError {
    #[error(transparent)]
    Domain(#[from] DomainError),

    /// La política de riesgo bloqueó la orden; contiene todas las violaciones encontradas.
    #[error("orden bloqueada por la política de riesgo ({} violaciones)", .0.len())]
    RiskRejected(Vec<RiskViolation>),

    #[error("no existe la orden {0}")]
    OrderNotFound(OrderId),

    #[error(transparent)]
    Broker(#[from] BrokerError),

    #[error(transparent)]
    MarketData(#[from] MarketDataError),

    #[error(transparent)]
    Journal(#[from] JournalError),
}
