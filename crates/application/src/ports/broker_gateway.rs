use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::stream::BoxStream;
use thiserror::Error;
use tickforge_domain::DomainError;
use tickforge_domain::market::MarketClock;
use tickforge_domain::portfolio::{Account, Position};
use tickforge_domain::shared::Symbol;
use tickforge_domain::trading::{Fill, Order, OrderEvent, OrderId, OrderRequest};

/// Puerto de ejecución: lo implementa cada broker (Alpaca, IBKR, simulador).
#[async_trait]
pub trait BrokerGateway: Send + Sync {
    async fn account(&self) -> Result<Account, BrokerError>;

    async fn positions(&self) -> Result<Vec<Position>, BrokerError>;

    /// Posición en un símbolo; `None` si no hay posición abierta.
    async fn position(&self, symbol: &Symbol) -> Result<Option<Position>, BrokerError>;

    async fn open_orders(&self) -> Result<Vec<Order>, BrokerError>;

    /// Orden por id; `None` si el broker no la conoce.
    async fn order(&self, order_id: &OrderId) -> Result<Option<Order>, BrokerError>;

    /// Envía la orden. El `ClientOrderId` de la solicitud garantiza que un reintento
    /// no cree una orden duplicada.
    async fn submit_order(&self, request: &OrderRequest) -> Result<Order, BrokerError>;

    /// Solicita la cancelación; la confirmación llega después por `order_updates`.
    async fn cancel_order(&self, order_id: &OrderId) -> Result<(), BrokerError>;

    /// Flujo de actualizaciones de órdenes en tiempo real.
    async fn order_updates(&self) -> Result<BoxStream<'static, OrderUpdate>, BrokerError>;

    async fn market_clock(&self) -> Result<MarketClock, BrokerError>;
}

/// Actualización de una orden reportada por el broker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderUpdate {
    pub order_id: OrderId,
    pub kind: OrderUpdateKind,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderUpdateKind {
    Accepted,
    /// Ejecución parcial o total; el agregado decide cuál según la cantidad pendiente.
    Filled(Fill),
    Canceled,
    CancelRejected,
    Rejected {
        reason: String,
    },
    Expired,
}

impl OrderUpdate {
    /// Traduce la actualización del broker en el comando correspondiente del agregado.
    ///
    /// Devuelve `Ok(None)` si la actualización ya estaba aplicada (ejecución duplicada).
    pub fn apply_to(&self, order: &mut Order) -> Result<Option<OrderEvent>, DomainError> {
        let at = self.occurred_at;
        match &self.kind {
            OrderUpdateKind::Accepted => order.mark_accepted(at).map(Some),
            OrderUpdateKind::Filled(fill) => order.apply_fill(fill.clone()),
            OrderUpdateKind::Canceled => order.mark_canceled(at).map(Some),
            OrderUpdateKind::CancelRejected => order.mark_cancel_rejected(at).map(Some),
            OrderUpdateKind::Rejected { reason } => {
                order.mark_rejected(reason.clone(), at).map(Some)
            }
            OrderUpdateKind::Expired => order.mark_expired(at).map(Some),
        }
    }
}

/// Error de comunicación o de negocio devuelto por el broker.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BrokerError {
    #[error("el broker no está disponible: {0}")]
    Unavailable(String),

    #[error("el broker rechazó las credenciales")]
    Unauthorized,

    #[error("se alcanzó el límite de peticiones del broker")]
    RateLimited,

    #[error("el broker rechazó la operación: {reason}")]
    Rejected { reason: String },

    #[error("el broker no encontró la orden {0}")]
    OrderNotFound(OrderId),

    #[error("respuesta inválida del broker: {0}")]
    InvalidResponse(String),
}

impl BrokerError {
    /// Errores transitorios que pueden reintentarse con backoff.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Unavailable(_) | Self::RateLimited)
    }
}
