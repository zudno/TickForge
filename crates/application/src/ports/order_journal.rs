use async_trait::async_trait;
use chrono::{DateTime, Utc};
use thiserror::Error;
use tickforge_domain::risk::RiskViolation;
use tickforge_domain::trading::{Order, OrderId, OrderRequest};

/// Diario de auditoría de órdenes (solo se agrega, nunca se modifica).
///
/// Registra la intención antes de enviar, para poder reconciliar con el broker si la
/// aplicación se cierra a mitad de un envío.
#[async_trait]
pub trait OrderJournal: Send + Sync {
    async fn append(&self, entry: JournalEntry) -> Result<(), JournalError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JournalEntry {
    /// La orden pasó el riesgo y está a punto de enviarse.
    Intent {
        request: OrderRequest,
        at: DateTime<Utc>,
    },
    /// La política de riesgo bloqueó la orden.
    RiskRejected {
        request: OrderRequest,
        violations: Vec<RiskViolation>,
        at: DateTime<Utc>,
    },
    /// El broker aceptó la orden y le asignó un id.
    Submitted { order: Order, at: DateTime<Utc> },
    /// El envío falló (error de red o rechazo del broker).
    SubmissionFailed {
        request: OrderRequest,
        reason: String,
        at: DateTime<Utc>,
    },
    /// Se pidió la cancelación al broker.
    CancelRequested {
        order_id: OrderId,
        at: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum JournalError {
    #[error("no se pudo escribir en el diario de órdenes: {0}")]
    Storage(String),
}
