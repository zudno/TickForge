use std::sync::Arc;

use tickforge_domain::market::MarketClock;

use crate::error::ApplicationError;
use crate::ports::BrokerGateway;

/// Caso de uso: consultar si el mercado está abierto y cuándo cambia de estado.
pub struct GetMarketClockUseCase {
    broker: Arc<dyn BrokerGateway>,
}

impl GetMarketClockUseCase {
    pub fn new(broker: Arc<dyn BrokerGateway>) -> Self {
        Self { broker }
    }

    pub async fn execute(&self) -> Result<MarketClock, ApplicationError> {
        Ok(self.broker.market_clock().await?)
    }
}
