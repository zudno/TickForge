use chrono::{DateTime, Utc};

use crate::trading::{Fill, OrderId};

/// Evento de dominio emitido por el agregado `Order` en cada cambio de estado.
///
/// La capa de aplicación lo publica en el bus de eventos para que la TUI, el diario de
/// órdenes y (en el futuro) los modelos de ML reaccionen sin acoplarse entre sí.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderEvent {
    pub order_id: OrderId,
    pub kind: OrderEventKind,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderEventKind {
    Accepted,
    PartiallyFilled { fill: Fill },
    Filled { fill: Fill },
    CancelRequested,
    CancelRejected,
    Canceled,
    Rejected { reason: String },
    Expired,
}
