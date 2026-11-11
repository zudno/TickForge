//! Casos de uso de trading: colocar y cancelar órdenes.

mod cancel_order;
mod place_order;
mod watch_order_updates;

use std::sync::Arc;

use tickforge_domain::trading::ClientOrderId;

use crate::ports::{
    ApplicationEvent, BrokerGateway, ClientOrderIdGenerator, Clock, EventPublisher, JournalEntry,
    MarketDataFeed, OrderJournal,
};

pub use cancel_order::CancelOrderUseCase;
pub use place_order::{OrderPreview, PlaceOrderCommand, PlaceOrderUseCase};
pub use watch_order_updates::WatchOrderUpdatesUseCase;

/// Puertos que comparten los casos de uso de trading.
///
/// Agruparlos evita constructores con demasiados parámetros y deja en un solo lugar
/// (el composition root) la decisión de qué implementación usar.
#[derive(Clone)]
pub struct TradingPorts {
    pub broker: Arc<dyn BrokerGateway>,
    pub market_data: Arc<dyn MarketDataFeed>,
    pub journal: Arc<dyn OrderJournal>,
    pub events: Arc<dyn EventPublisher>,
    pub clock: Arc<dyn Clock>,
    pub client_order_ids: Arc<dyn ClientOrderIdGenerator>,
}

impl TradingPorts {
    /// Escribe en el diario sin abortar la operación: si falla, lo notifica como evento.
    ///
    /// Se usa cuando la orden ya está en el broker (o el resultado ya está decidido) y
    /// devolver un error engañaría al usuario sobre el estado real de la orden.
    async fn append_best_effort(&self, entry: JournalEntry, client_order_id: &ClientOrderId) {
        if let Err(error) = self.journal.append(entry).await {
            self.events.publish(ApplicationEvent::JournalWriteFailed {
                client_order_id: client_order_id.clone(),
                reason: error.to_string(),
            });
        }
    }
}
