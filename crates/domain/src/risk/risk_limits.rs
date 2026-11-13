use chrono::TimeDelta;
use rust_decimal::Decimal;

use crate::error::DomainError;
use crate::shared::Money;

/// Límites configurables que aplica la política de riesgo.
///
/// Se cargan desde la configuración; el modo autónomo de ML podrá usar límites más estrictos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RiskLimits {
    max_order_notional: Money,
    max_position_notional: Money,
    max_price_deviation_pct: Decimal,
    max_quote_age: TimeDelta,
    /// Las órdenes a mercado solo se aceptan con el mercado abierto (sesión regular). Fuera de
    /// ella la liquidez es baja y el precio de ejecución puede alejarse mucho de la cotización.
    market_orders_require_open_market: bool,
}

impl RiskLimits {
    pub fn new(
        max_order_notional: Money,
        max_position_notional: Money,
        max_price_deviation_pct: Decimal,
        max_quote_age: TimeDelta,
    ) -> Result<Self, DomainError> {
        if max_order_notional.currency() != max_position_notional.currency() {
            return Err(DomainError::CurrencyMismatch {
                left: max_order_notional.currency(),
                right: max_position_notional.currency(),
            });
        }

        let positive_amounts = max_order_notional.amount() > Decimal::ZERO
            && max_position_notional.amount() > Decimal::ZERO;
        if !positive_amounts {
            return Err(DomainError::InvalidRiskLimits(
                "los montos máximos deben ser mayores que cero",
            ));
        }
        if max_price_deviation_pct <= Decimal::ZERO {
            return Err(DomainError::InvalidRiskLimits(
                "la desviación máxima de precio debe ser mayor que cero",
            ));
        }
        if max_quote_age <= TimeDelta::zero() {
            return Err(DomainError::InvalidRiskLimits(
                "la antigüedad máxima de la cotización debe ser mayor que cero",
            ));
        }

        Ok(Self {
            max_order_notional,
            max_position_notional,
            max_price_deviation_pct,
            max_quote_age,
            market_orders_require_open_market: false,
        })
    }

    /// Exige que el mercado esté abierto para aceptar órdenes a mercado (se usa en modo real).
    pub fn restrict_market_orders_to_open_market(mut self) -> Self {
        self.market_orders_require_open_market = true;
        self
    }

    pub fn market_orders_require_open_market(&self) -> bool {
        self.market_orders_require_open_market
    }

    /// Valor máximo de una sola orden.
    pub fn max_order_notional(&self) -> Money {
        self.max_order_notional
    }

    /// Valor máximo de la posición en un símbolo después de ejecutar la orden.
    pub fn max_position_notional(&self) -> Money {
        self.max_position_notional
    }

    /// Desviación máxima (en %) del precio límite respecto al precio de mercado (fat-finger).
    pub fn max_price_deviation_pct(&self) -> Decimal {
        self.max_price_deviation_pct
    }

    /// Antigüedad máxima de la cotización usada para evaluar la orden.
    pub fn max_quote_age(&self) -> TimeDelta {
        self.max_quote_age
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn rejects_non_positive_limits() {
        let valid = (
            Money::usd(dec!(1000)),
            Money::usd(dec!(5000)),
            dec!(5),
            TimeDelta::seconds(5),
        );
        assert!(RiskLimits::new(valid.0, valid.1, valid.2, valid.3).is_ok());

        assert!(RiskLimits::new(Money::usd(dec!(0)), valid.1, valid.2, valid.3).is_err());
        assert!(RiskLimits::new(valid.0, valid.1, dec!(0), valid.3).is_err());
        assert!(RiskLimits::new(valid.0, valid.1, valid.2, TimeDelta::zero()).is_err());
    }
}
