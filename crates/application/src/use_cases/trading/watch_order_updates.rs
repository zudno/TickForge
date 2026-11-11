use std::sync::Arc;

use futures::stream::BoxStream;

use crate::error::ApplicationError;
use crate::ports::{BrokerGateway, OrderUpdate};

/// Caso de uso: escuchar en tiempo real los cambios de las órdenes (aceptación, ejecuciones,
/// cancelaciones, rechazos) para refrescar la interfaz al instante.
pub struct WatchOrderUpdatesUseCase {
    broker: Arc<dyn BrokerGateway>,
}

impl WatchOrderUpdatesUseCase {
    pub fn new(broker: Arc<dyn BrokerGateway>) -> Self {
        Self { broker }
    }

    pub async fn stream(&self) -> Result<BoxStream<'static, OrderUpdate>, ApplicationError> {
        Ok(self.broker.order_updates().await?)
    }
}
