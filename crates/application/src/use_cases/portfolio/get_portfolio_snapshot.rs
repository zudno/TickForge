use std::sync::Arc;

use tickforge_domain::portfolio::{Account, Position};
use tickforge_domain::trading::Order;

use crate::error::ApplicationError;
use crate::ports::BrokerGateway;

/// Foto del portafolio para los paneles de la TUI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortfolioSnapshot {
    pub account: Account,
    pub positions: Vec<Position>,
    pub open_orders: Vec<Order>,
}

/// Caso de uso: obtener cuenta, posiciones y órdenes abiertas en una sola consulta paralela.
pub struct GetPortfolioSnapshotUseCase {
    broker: Arc<dyn BrokerGateway>,
}

impl GetPortfolioSnapshotUseCase {
    pub fn new(broker: Arc<dyn BrokerGateway>) -> Self {
        Self { broker }
    }

    pub async fn execute(&self) -> Result<PortfolioSnapshot, ApplicationError> {
        let (account, positions, open_orders) = futures::try_join!(
            self.broker.account(),
            self.broker.positions(),
            self.broker.open_orders(),
        )?;

        Ok(PortfolioSnapshot {
            account,
            positions,
            open_orders,
        })
    }
}
