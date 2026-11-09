use chrono::{DateTime, Utc};

use crate::shared::{Price, Quantity, Symbol};

/// Operación individual ejecutada en el mercado (último precio negociado).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradeTick {
    symbol: Symbol,
    price: Price,
    size: Quantity,
    executed_at: DateTime<Utc>,
}

impl TradeTick {
    pub fn new(symbol: Symbol, price: Price, size: Quantity, executed_at: DateTime<Utc>) -> Self {
        Self {
            symbol,
            price,
            size,
            executed_at,
        }
    }

    pub fn symbol(&self) -> &Symbol {
        &self.symbol
    }

    pub fn price(&self) -> Price {
        self.price
    }

    pub fn size(&self) -> Quantity {
        self.size
    }

    pub fn executed_at(&self) -> DateTime<Utc> {
        self.executed_at
    }
}
