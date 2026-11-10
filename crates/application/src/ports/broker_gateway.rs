use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::stream::BoxStream;
use thiserror::Error;
use tickforge_domain::DomainError;
use tickforge_domain::market::MarketClock;
use tickforge_domain::portfolio::{Account, Position};
use tickforge_domain::shared::Symbol;
use tickforge_domain::trading::{
    ClientOrderId, Fill, Order, OrderEvent, OrderId, OrderRequest, OrderStatus,
};

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

    /// Orden por el id que asignó TickForge; `None` si nunca llegó al broker.
    /// Es la base de la reconciliación al iniciar.
    async fn order_by_client_order_id(
        &self,
        client_order_id: &ClientOrderId,
    ) -> Result<Option<Order>, BrokerError>;

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
    /// Es idempotente: devuelve `Ok(None)` si la orden ya refleja la actualización. Esto pasa
    /// con frecuencia porque el estado llega por dos vías (la respuesta REST y el WebSocket)
    /// y los eventos pueden repetirse tras una reconexión.
    ///
    /// Limitación conocida: una orden reconstruida desde REST (`Order::restore`) no conoce los
    /// ids de sus ejecuciones previas; la reconciliación de la fase 6 evita aplicarlas dos veces.
    pub fn apply_to(&self, order: &mut Order) -> Result<Option<OrderEvent>, DomainError> {
        let at = self.occurred_at;
        let status = order.status();

        match &self.kind {
            OrderUpdateKind::Accepted if status != OrderStatus::PendingNew => Ok(None),
            OrderUpdateKind::Accepted => order.mark_accepted(at).map(Some),

            OrderUpdateKind::Filled(_) if status == OrderStatus::Filled => Ok(None),
            OrderUpdateKind::Filled(fill) => order.apply_fill(fill.clone()),

            OrderUpdateKind::Canceled if status == OrderStatus::Canceled => Ok(None),
            OrderUpdateKind::Canceled => order.mark_canceled(at).map(Some),

            // Solo hay algo que revertir si la cancelación sigue pendiente
            OrderUpdateKind::CancelRejected if status != OrderStatus::PendingCancel => Ok(None),
            OrderUpdateKind::CancelRejected => order.mark_cancel_rejected(at).map(Some),

            OrderUpdateKind::Rejected { .. } if status == OrderStatus::Rejected => Ok(None),
            OrderUpdateKind::Rejected { reason } => {
                order.mark_rejected(reason.clone(), at).map(Some)
            }

            OrderUpdateKind::Expired if status == OrderStatus::Expired => Ok(None),
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

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use tickforge_domain::shared::{Price, Quantity};
    use tickforge_domain::trading::{ClientOrderId, FillId, OrderSide, OrderType, TimeInForce};

    fn at(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(seconds, 0).unwrap()
    }

    fn order() -> Order {
        let request = OrderRequest::new(
            ClientOrderId::new("tf-1").unwrap(),
            Symbol::new("AAPL").unwrap(),
            OrderSide::Buy,
            OrderType::Market,
            Quantity::new(dec!(2)).unwrap(),
            TimeInForce::Day,
        )
        .unwrap();
        Order::new(OrderId::new("ord-1").unwrap(), request, at(0))
    }

    fn update(kind: OrderUpdateKind) -> OrderUpdate {
        OrderUpdate {
            order_id: OrderId::new("ord-1").unwrap(),
            kind,
            occurred_at: at(5),
        }
    }

    fn fill(id: &str, quantity: rust_decimal::Decimal) -> OrderUpdateKind {
        OrderUpdateKind::Filled(
            Fill::new(
                FillId::new(id).unwrap(),
                Quantity::new(quantity).unwrap(),
                Price::new(dec!(100)).unwrap(),
                at(5),
            )
            .unwrap(),
        )
    }

    #[test]
    fn repeated_lifecycle_updates_are_no_ops() {
        let mut order = order();

        assert!(
            update(OrderUpdateKind::Accepted)
                .apply_to(&mut order)
                .unwrap()
                .is_some()
        );
        // Segundo "new" (por ejemplo, REST y WebSocket a la vez)
        assert_eq!(
            update(OrderUpdateKind::Accepted).apply_to(&mut order),
            Ok(None)
        );

        assert!(
            update(OrderUpdateKind::Canceled)
                .apply_to(&mut order)
                .unwrap()
                .is_some()
        );
        assert_eq!(
            update(OrderUpdateKind::Canceled).apply_to(&mut order),
            Ok(None)
        );
        assert_eq!(order.status(), OrderStatus::Canceled);
    }

    #[test]
    fn fills_after_completion_are_ignored() {
        let mut order = order();
        update(fill("e1", dec!(2))).apply_to(&mut order).unwrap();
        assert_eq!(order.status(), OrderStatus::Filled);

        // Una ejecución con otro id sobre una orden ya completa no es un error
        assert_eq!(update(fill("e2", dec!(1))).apply_to(&mut order), Ok(None));
    }

    #[test]
    fn cancel_rejection_without_pending_cancel_is_a_no_op() {
        let mut order = order();
        update(OrderUpdateKind::Accepted)
            .apply_to(&mut order)
            .unwrap();

        assert_eq!(
            update(OrderUpdateKind::CancelRejected).apply_to(&mut order),
            Ok(None)
        );
        assert_eq!(order.status(), OrderStatus::New);
    }

    #[test]
    fn real_conflicts_are_still_errors() {
        // Una orden cancelada no puede pasar a expirada
        let mut order = order();
        update(OrderUpdateKind::Canceled)
            .apply_to(&mut order)
            .unwrap();
        assert!(
            update(OrderUpdateKind::Expired)
                .apply_to(&mut order)
                .is_err()
        );
    }
}
