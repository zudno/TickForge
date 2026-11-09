use std::fmt;

use chrono::{DateTime, Utc};

use crate::error::DomainError;
use crate::shared::{Price, Quantity};
use crate::trading::identifier::{is_visible_ascii, sanitize};

/// Identificador de una ejecución, asignado por el broker.
///
/// Permite descartar ejecuciones duplicadas (por ejemplo, el mismo evento recibido dos veces
/// tras una reconexión del WebSocket).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FillId(String);

impl FillId {
    pub const MAX_LEN: usize = 128;

    pub fn new(raw: &str) -> Result<Self, DomainError> {
        sanitize(raw, Self::MAX_LEN, is_visible_ascii)
            .map(Self)
            .ok_or_else(|| DomainError::InvalidFillId(raw.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FillId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Ejecución (total o parcial) de una orden.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fill {
    id: FillId,
    quantity: Quantity,
    price: Price,
    executed_at: DateTime<Utc>,
}

impl Fill {
    pub fn new(
        id: FillId,
        quantity: Quantity,
        price: Price,
        executed_at: DateTime<Utc>,
    ) -> Result<Self, DomainError> {
        // Una ejecución sin acciones no tiene sentido económico
        if quantity.is_zero() {
            return Err(DomainError::NonPositiveQuantity(quantity.value()));
        }
        Ok(Self {
            id,
            quantity,
            price,
            executed_at,
        })
    }

    pub fn id(&self) -> &FillId {
        &self.id
    }

    pub fn quantity(&self) -> Quantity {
        self.quantity
    }

    pub fn price(&self) -> Price {
        self.price
    }

    pub fn executed_at(&self) -> DateTime<Utc> {
        self.executed_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;

    #[test]
    fn rejects_zero_quantity() {
        let result = Fill::new(
            FillId::new("exec-1").unwrap(),
            Quantity::ZERO,
            Price::new(dec!(10)).unwrap(),
            DateTime::UNIX_EPOCH,
        );
        assert_eq!(result, Err(DomainError::NonPositiveQuantity(Decimal::ZERO)));
    }
}
