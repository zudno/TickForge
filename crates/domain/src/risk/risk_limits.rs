use chrono::TimeDelta;
use rust_decimal::Decimal;

use crate::error::DomainError;
use crate::portfolio::Account;
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
    /// Pérdida del día (equity contra el cierre anterior) a partir de la cual no se aceptan
    /// órdenes nuevas. `None`: sin límite.
    max_daily_loss: Option<Money>,
    /// Tope de órdenes enviadas en los últimos 60 segundos. `None`: sin tope.
    max_orders_per_minute: Option<u32>,
    /// Tope de órdenes enviadas en las últimas 24 horas. `None`: sin tope.
    max_orders_per_day: Option<u32>,
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
            max_daily_loss: None,
            max_orders_per_minute: None,
            max_orders_per_day: None,
        })
    }

    /// Fija la pérdida máxima del día (monto positivo, en la moneda de los demás límites).
    pub fn with_max_daily_loss(mut self, max_daily_loss: Money) -> Result<Self, DomainError> {
        if max_daily_loss.currency() != self.max_order_notional.currency() {
            return Err(DomainError::CurrencyMismatch {
                left: self.max_order_notional.currency(),
                right: max_daily_loss.currency(),
            });
        }
        if max_daily_loss.amount() <= Decimal::ZERO {
            return Err(DomainError::InvalidRiskLimits(
                "la pérdida máxima del día debe ser mayor que cero",
            ));
        }
        self.max_daily_loss = Some(max_daily_loss);
        Ok(self)
    }

    /// Fija el tope de órdenes por minuto y por día (ventanas móviles de 60 s y 24 h).
    pub fn with_order_rate_limits(
        mut self,
        per_minute: u32,
        per_day: u32,
    ) -> Result<Self, DomainError> {
        if per_minute == 0 || per_day == 0 {
            return Err(DomainError::InvalidRiskLimits(
                "los topes de órdenes deben ser mayores que cero",
            ));
        }
        if per_minute > per_day {
            return Err(DomainError::InvalidRiskLimits(
                "el tope de órdenes por minuto no puede superar al tope por día",
            ));
        }
        self.max_orders_per_minute = Some(per_minute);
        self.max_orders_per_day = Some(per_day);
        Ok(self)
    }

    pub fn max_daily_loss(&self) -> Option<Money> {
        self.max_daily_loss
    }

    pub fn max_orders_per_minute(&self) -> Option<u32> {
        self.max_orders_per_minute
    }

    pub fn max_orders_per_day(&self) -> Option<u32> {
        self.max_orders_per_day
    }

    /// Indica si hay algún tope de órdenes (y por lo tanto hay que contar las recientes).
    pub fn limits_order_rate(&self) -> bool {
        self.max_orders_per_minute.is_some() || self.max_orders_per_day.is_some()
    }

    /// Pérdida del día si alcanzó o superó el máximo; `None` si no hay límite o no se
    /// alcanzó. La usan la política (para bloquear órdenes) y la vigilancia que activa el
    /// kill switch, así ambas deciden exactamente igual.
    pub fn daily_loss_breach(&self, account: &Account) -> Result<Option<Money>, DomainError> {
        let Some(limit) = self.max_daily_loss else {
            return Ok(None);
        };
        let pnl = account.day_pnl()?;
        if pnl.currency() != limit.currency() {
            return Err(DomainError::CurrencyMismatch {
                left: limit.currency(),
                right: pnl.currency(),
            });
        }
        let loss = Money::new(-pnl.amount(), pnl.currency());
        Ok((loss.amount() >= limit.amount()).then_some(loss))
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

    fn base() -> RiskLimits {
        RiskLimits::new(
            Money::usd(Decimal::from(250)),
            Money::usd(Decimal::from(1000)),
            Decimal::from(3),
            TimeDelta::seconds(10),
        )
        .unwrap()
    }

    fn account(equity: i64, last_equity: i64) -> Account {
        Account::new(
            crate::portfolio::AccountBalances {
                equity: Money::usd(Decimal::from(equity)),
                cash: Money::usd(Decimal::from(equity)),
                buying_power: Money::usd(Decimal::from(equity)),
                last_equity: Money::usd(Decimal::from(last_equity)),
            },
            false,
        )
        .unwrap()
    }

    #[test]
    fn daily_loss_and_order_caps_are_optional_and_validated() {
        let limits = base();
        assert_eq!(limits.max_daily_loss(), None);
        assert!(!limits.limits_order_rate());

        assert!(
            base()
                .with_max_daily_loss(Money::usd(Decimal::ZERO))
                .is_err()
        );
        assert!(base().with_order_rate_limits(0, 10).is_err());
        assert!(base().with_order_rate_limits(10, 5).is_err());

        let strict = base()
            .with_max_daily_loss(Money::usd(Decimal::from(50)))
            .unwrap()
            .with_order_rate_limits(5, 30)
            .unwrap();
        assert_eq!(strict.max_daily_loss(), Some(Money::usd(Decimal::from(50))));
        assert_eq!(strict.max_orders_per_minute(), Some(5));
        assert_eq!(strict.max_orders_per_day(), Some(30));
        assert!(strict.limits_order_rate());
    }

    #[test]
    fn daily_loss_breach_starts_exactly_at_the_limit() {
        let limits = base()
            .with_max_daily_loss(Money::usd(Decimal::from(50)))
            .unwrap();

        assert_eq!(
            limits.daily_loss_breach(&account(1000, 1049)).unwrap(),
            None
        );
        assert_eq!(
            limits.daily_loss_breach(&account(1000, 1050)).unwrap(),
            Some(Money::usd(Decimal::from(50)))
        );
        assert_eq!(
            limits.daily_loss_breach(&account(1000, 1200)).unwrap(),
            Some(Money::usd(Decimal::from(200)))
        );
        // Una ganancia nunca es una pérdida
        assert_eq!(
            limits.daily_loss_breach(&account(2000, 1000)).unwrap(),
            None
        );
        // Sin límite configurado no hay nada que vigilar
        assert_eq!(
            base().daily_loss_breach(&account(0, 100_000)).unwrap(),
            None
        );
    }
}
