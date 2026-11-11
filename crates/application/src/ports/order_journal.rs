use async_trait::async_trait;
use chrono::{DateTime, Utc};
use thiserror::Error;
use tickforge_domain::risk::RiskViolation;
use tickforge_domain::trading::{ClientOrderId, Order, OrderId, OrderRequest, OrderStatus};

/// Diario de auditoría de órdenes (solo se agrega, nunca se modifica).
///
/// Registra la intención antes de enviar, para poder reconciliar con el broker si la
/// aplicación se cierra (o pierde la red) a mitad de un envío.
#[async_trait]
pub trait OrderJournal: Send + Sync {
    async fn append(&self, entry: JournalEntry) -> Result<(), JournalError>;

    /// Intenciones sin resultado confirmado: no hay `Submitted` ni `Reconciled` para su
    /// `ClientOrderId`. Incluye las que tienen `SubmissionFailed`, porque un error de red
    /// puede ocurrir **después** de que el broker recibió la orden.
    async fn pending_intents(&self) -> Result<Vec<OrderRequest>, JournalError>;
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
    /// Una intención pendiente se comparó con el broker al iniciar.
    Reconciled {
        client_order_id: ClientOrderId,
        outcome: ReconcileOutcome,
        at: DateTime<Utc>,
    },
}

impl JournalEntry {
    /// `ClientOrderId` al que se refiere la entrada, si aplica.
    pub fn client_order_id(&self) -> Option<&ClientOrderId> {
        match self {
            Self::Intent { request, .. }
            | Self::RiskRejected { request, .. }
            | Self::SubmissionFailed { request, .. } => Some(request.client_order_id()),
            Self::Submitted { order, .. } => Some(order.client_order_id()),
            Self::Reconciled {
                client_order_id, ..
            } => Some(client_order_id),
            Self::CancelRequested { .. } => None,
        }
    }

    /// Indica si la entrada cierra una intención (ya se sabe qué pasó con ella).
    pub fn resolves_intent(&self) -> bool {
        matches!(self, Self::Submitted { .. } | Self::Reconciled { .. })
    }
}

/// Resultado de reconciliar una intención pendiente.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconcileOutcome {
    /// El broker sí tiene la orden: el envío llegó aunque la aplicación no lo registró.
    FoundAtBroker {
        order_id: OrderId,
        status: OrderStatus,
    },
    /// El broker no la conoce: la orden nunca llegó.
    NotFoundAtBroker,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum JournalError {
    #[error("no se pudo acceder al diario de órdenes: {0}")]
    Storage(String),

    #[error("el diario de órdenes contiene datos inválidos: {0}")]
    Corrupted(String),
}
