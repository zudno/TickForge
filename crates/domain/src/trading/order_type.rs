use std::fmt;

use crate::shared::Price;

/// Tipo de orden soportado en el MVP.
///
/// Los tipos avanzados (stop, stop-limit, trailing, bracket) se agregarán como nuevas variantes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OrderType {
    /// Se ejecuta al mejor precio disponible.
    Market,
    /// Se ejecuta solo a `limit_price` o mejor.
    Limit { limit_price: Price },
}

impl OrderType {
    pub fn limit_price(self) -> Option<Price> {
        match self {
            Self::Market => None,
            Self::Limit { limit_price } => Some(limit_price),
        }
    }

    /// Código estable para logs y persistencia.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Market => "market",
            Self::Limit { .. } => "limit",
        }
    }
}

impl fmt::Display for OrderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Market => f.write_str("market"),
            Self::Limit { limit_price } => write!(f, "limit @ {limit_price}"),
        }
    }
}
