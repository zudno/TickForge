use tickforge_domain::risk::RiskViolation;
use tickforge_domain::trading::{ClientOrderId, Order, OrderEvent, OrderRequest};

/// Publica eventos de la aplicación para que otros componentes reaccionen sin acoplarse
/// (la TUI, el diario, y en el futuro los modelos de ML).
///
/// Publicar nunca falla ni bloquea: si nadie escucha, el evento se descarta.
pub trait EventPublisher: Send + Sync {
    fn publish(&self, event: ApplicationEvent);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplicationEvent {
    /// El broker aceptó una orden nueva.
    OrderSubmitted(Order),
    /// La política de riesgo bloqueó una orden antes de enviarla.
    OrderRejectedByRisk {
        request: OrderRequest,
        violations: Vec<RiskViolation>,
    },
    /// El envío al broker falló.
    OrderSubmissionFailed {
        request: OrderRequest,
        reason: String,
    },
    /// Cambio de estado de una orden (cancelación solicitada, ejecución, rechazo, etc.).
    Order(OrderEvent),
    /// No se pudo escribir en el diario después de que el broker aceptó la orden.
    /// La orden está viva; la reconciliación al iniciar corregirá el diario.
    JournalWriteFailed {
        client_order_id: ClientOrderId,
        reason: String,
    },
}
