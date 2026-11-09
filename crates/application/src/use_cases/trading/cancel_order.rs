use tickforge_domain::trading::OrderId;

use crate::error::ApplicationError;
use crate::ports::{ApplicationEvent, JournalEntry};
use crate::use_cases::trading::TradingPorts;

/// Caso de uso: solicitar la cancelación de una orden abierta.
///
/// La confirmación (`Canceled`, o `Filled` si la ejecución ganó la carrera) llega después
/// por el flujo de actualizaciones del broker.
pub struct CancelOrderUseCase {
    ports: TradingPorts,
}

impl CancelOrderUseCase {
    pub fn new(ports: TradingPorts) -> Self {
        Self { ports }
    }

    pub async fn execute(&self, order_id: &OrderId) -> Result<(), ApplicationError> {
        let ports = &self.ports;

        let mut order = ports
            .broker
            .order(order_id)
            .await?
            .ok_or_else(|| ApplicationError::OrderNotFound(order_id.clone()))?;

        // El agregado decide si la cancelación es válida en su estado actual
        let now = ports.clock.now();
        let event = order.request_cancel(now)?;

        ports.broker.cancel_order(order_id).await?;

        ports
            .append_best_effort(
                JournalEntry::CancelRequested {
                    order_id: order_id.clone(),
                    at: now,
                },
                order.client_order_id(),
            )
            .await;
        ports.events.publish(ApplicationEvent::Order(event));

        Ok(())
    }
}
