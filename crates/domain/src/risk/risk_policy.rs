use chrono::{DateTime, Utc};

use crate::error::DomainError;
use crate::market::Quote;
use crate::portfolio::{Account, Position};
use crate::risk::{KillSwitch, RiskLimits, RiskViolation};
use crate::shared::{Money, Quantity};
use crate::trading::{OrderRequest, OrderSide};

/// Todo lo que la política necesita para evaluar una orden, reunido por el caso de uso.
#[derive(Debug, Clone, Copy)]
pub struct RiskContext<'a> {
    pub request: &'a OrderRequest,
    pub account: &'a Account,
    /// Posición actual en el símbolo de la orden, si existe.
    pub position: Option<&'a Position>,
    pub quote: &'a Quote,
    pub now: DateTime<Utc>,
    /// Interruptor de emergencia: si está activo, ninguna orden pasa.
    pub kill_switch: &'a KillSwitch,
}

/// Resultado de la evaluación: aprobada si no hay violaciones.
///
/// Se devuelven **todas** las violaciones (no solo la primera) para que la TUI pueda
/// mostrarle al usuario todo lo que debe corregir de una sola vez.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RiskAssessment {
    violations: Vec<RiskViolation>,
}

impl RiskAssessment {
    pub fn is_approved(&self) -> bool {
        self.violations.is_empty()
    }

    pub fn violations(&self) -> &[RiskViolation] {
        &self.violations
    }

    pub fn into_violations(self) -> Vec<RiskViolation> {
        self.violations
    }
}

/// Política de riesgo pre-trade. Es la única puerta de salida de cualquier orden.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RiskPolicy {
    limits: RiskLimits,
}

impl RiskPolicy {
    pub fn new(limits: RiskLimits) -> Self {
        Self { limits }
    }

    pub fn limits(&self) -> &RiskLimits {
        &self.limits
    }

    /// Evalúa la orden. `Err` indica datos incoherentes (un error de programación o del
    /// adaptador), no una violación de riesgo; las violaciones van en el `RiskAssessment`.
    pub fn evaluate(&self, context: &RiskContext<'_>) -> Result<RiskAssessment, DomainError> {
        let RiskContext {
            request,
            account,
            position,
            quote,
            now,
            kill_switch,
        } = *context;

        ensure_same_symbol(request, quote.symbol())?;
        if let Some(position) = position {
            ensure_same_symbol(request, position.symbol())?;
        }

        let mut violations = Vec::new();

        if let KillSwitch::Engaged { reason, .. } = kill_switch {
            violations.push(RiskViolation::KillSwitchEngaged {
                reason: reason.clone(),
            });
        }
        if account.is_trading_blocked() {
            violations.push(RiskViolation::TradingBlocked);
        }

        let quote_age = quote.age(now);
        if quote_age > self.limits.max_quote_age() {
            violations.push(RiskViolation::stale_quote(
                quote_age,
                self.limits.max_quote_age(),
            ));
        }

        let side = request.side();
        let market_price = quote.executable_price(side);
        let limit_price = request.order_type().limit_price();

        // Sin precio de mercado ni precio límite no se puede estimar el valor de la orden
        let Some(reference_price) = limit_price.or(market_price) else {
            violations.push(RiskViolation::NoExecutablePrice { side });
            return Ok(RiskAssessment { violations });
        };

        let notional = Money::notional(
            reference_price,
            request.quantity(),
            self.limits.max_order_notional().currency(),
        )?;
        if notional.exceeds(self.limits.max_order_notional())? {
            violations.push(RiskViolation::OrderNotionalExceeded {
                notional,
                limit: self.limits.max_order_notional(),
            });
        }

        let held = position.map_or(Quantity::ZERO, Position::quantity);
        match side {
            OrderSide::Buy => {
                if !account.can_afford(notional)? {
                    violations.push(RiskViolation::InsufficientBuyingPower {
                        required: notional,
                        available: account.buying_power(),
                    });
                }

                let projected_quantity = held.checked_add(request.quantity())?;
                let projected =
                    Money::notional(reference_price, projected_quantity, notional.currency())?;
                if projected.exceeds(self.limits.max_position_notional())? {
                    violations.push(RiskViolation::PositionLimitExceeded {
                        projected,
                        limit: self.limits.max_position_notional(),
                    });
                }
            }
            OrderSide::Sell => {
                if request.quantity() > held {
                    violations.push(RiskViolation::InsufficientPosition {
                        requested: request.quantity(),
                        held,
                    });
                }
            }
        }

        // Control fat-finger: un precio límite muy alejado del mercado suele ser un error de dedo
        if let (Some(limit), Some(market)) = (limit_price, market_price) {
            let deviation_pct = limit.deviation_pct(market)?;
            if deviation_pct > self.limits.max_price_deviation_pct() {
                violations.push(RiskViolation::PriceDeviationExceeded {
                    deviation_pct: deviation_pct.round_dp(2),
                    limit_pct: self.limits.max_price_deviation_pct(),
                });
            }
        }

        Ok(RiskAssessment { violations })
    }
}

fn ensure_same_symbol(
    request: &OrderRequest,
    actual: &crate::shared::Symbol,
) -> Result<(), DomainError> {
    if request.symbol() != actual {
        return Err(DomainError::SymbolMismatch {
            expected: request.symbol().clone(),
            actual: actual.clone(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portfolio::AccountBalances;
    use crate::shared::{Price, Symbol};
    use crate::trading::{ClientOrderId, OrderType, TimeInForce};
    use chrono::TimeDelta;
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;

    const NOW: i64 = 1_000;

    fn at(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(seconds, 0).unwrap()
    }

    fn policy() -> RiskPolicy {
        RiskPolicy::new(
            RiskLimits::new(
                Money::usd(dec!(10000)),
                Money::usd(dec!(20000)),
                dec!(5),
                TimeDelta::seconds(5),
            )
            .unwrap(),
        )
    }

    fn account(buying_power: Decimal, blocked: bool) -> Account {
        Account::new(
            AccountBalances {
                equity: Money::usd(dec!(50000)),
                cash: Money::usd(dec!(50000)),
                buying_power: Money::usd(buying_power),
                last_equity: Money::usd(dec!(50000)),
            },
            blocked,
        )
        .unwrap()
    }

    fn quote(symbol: &str, bid: Decimal, ask: Decimal, quoted_at: i64) -> Quote {
        Quote::new(
            Symbol::new(symbol).unwrap(),
            Some(Price::new(bid).unwrap()),
            Quantity::new(dec!(100)).unwrap(),
            Some(Price::new(ask).unwrap()),
            Quantity::new(dec!(100)).unwrap(),
            at(quoted_at),
        )
    }

    fn order(side: OrderSide, order_type: OrderType, quantity: Decimal) -> OrderRequest {
        OrderRequest::new(
            ClientOrderId::new("tf-1").unwrap(),
            Symbol::new("AAPL").unwrap(),
            side,
            order_type,
            Quantity::new(quantity).unwrap(),
            TimeInForce::Day,
        )
        .unwrap()
    }

    fn limit(price: Decimal) -> OrderType {
        OrderType::Limit {
            limit_price: Price::new(price).unwrap(),
        }
    }

    fn evaluate(
        request: &OrderRequest,
        account: &Account,
        position: Option<&Position>,
        quote: &Quote,
    ) -> RiskAssessment {
        policy()
            .evaluate(&RiskContext {
                request,
                account,
                position,
                quote,
                now: at(NOW),
                kill_switch: &KillSwitch::Released,
            })
            .unwrap()
    }

    #[test]
    fn approves_a_reasonable_buy() {
        let request = order(OrderSide::Buy, OrderType::Market, dec!(10));
        let assessment = evaluate(
            &request,
            &account(dec!(50000), false),
            None,
            &quote("AAPL", dec!(189.10), dec!(189.12), NOW),
        );
        assert!(assessment.is_approved(), "{:?}", assessment.violations());
    }

    #[test]
    fn an_engaged_kill_switch_blocks_even_a_perfect_order() {
        let request = order(OrderSide::Buy, OrderType::Market, dec!(1));
        let kill_switch = KillSwitch::engaged(at(NOW), "manual");
        let assessment = policy()
            .evaluate(&RiskContext {
                request: &request,
                account: &account(dec!(50000), false),
                position: None,
                quote: &quote("AAPL", dec!(189.10), dec!(189.12), NOW),
                now: at(NOW),
                kill_switch: &kill_switch,
            })
            .unwrap();

        assert_eq!(
            assessment.violations(),
            [RiskViolation::KillSwitchEngaged {
                reason: "manual".to_owned()
            }]
        );
    }

    #[test]
    fn reports_every_violation_at_once() {
        // Cuenta bloqueada + cotización vieja + orden de 1000 acciones a ~189 USD
        let request = order(OrderSide::Buy, OrderType::Market, dec!(1000));
        let assessment = evaluate(
            &request,
            &account(dec!(1000), true),
            None,
            &quote("AAPL", dec!(189.10), dec!(189.12), NOW - 60),
        );

        let violations = assessment.violations();
        assert!(violations.contains(&RiskViolation::TradingBlocked));
        assert!(
            violations
                .iter()
                .any(|v| matches!(v, RiskViolation::StaleQuote { .. }))
        );
        assert!(
            violations
                .iter()
                .any(|v| matches!(v, RiskViolation::OrderNotionalExceeded { .. }))
        );
        assert!(
            violations
                .iter()
                .any(|v| matches!(v, RiskViolation::InsufficientBuyingPower { .. }))
        );
        assert!(
            violations
                .iter()
                .any(|v| matches!(v, RiskViolation::PositionLimitExceeded { .. }))
        );
    }

    #[test]
    fn position_limit_includes_existing_holdings() {
        // 100 acciones en cartera + 10 nuevas a 189.12 = 20,803.20 > 20,000
        let position = Position::new(
            Symbol::new("AAPL").unwrap(),
            Quantity::new(dec!(100)).unwrap(),
            dec!(150),
        )
        .unwrap();
        let request = order(OrderSide::Buy, OrderType::Market, dec!(10));
        let assessment = evaluate(
            &request,
            &account(dec!(50000), false),
            Some(&position),
            &quote("AAPL", dec!(189.10), dec!(189.12), NOW),
        );

        assert!(matches!(
            assessment.violations(),
            [RiskViolation::PositionLimitExceeded { .. }]
        ));
    }

    #[test]
    fn blocks_short_selling() {
        let request = order(OrderSide::Sell, OrderType::Market, dec!(5));
        let assessment = evaluate(
            &request,
            &account(dec!(50000), false),
            None,
            &quote("AAPL", dec!(189.10), dec!(189.12), NOW),
        );

        assert!(matches!(
            assessment.violations(),
            [RiskViolation::InsufficientPosition { .. }]
        ));
    }

    #[test]
    fn detects_fat_finger_limit_price() {
        // Quiso escribir 189.00 y escribió 1890.00
        let request = order(OrderSide::Buy, limit(dec!(1890)), dec!(1));
        let assessment = evaluate(
            &request,
            &account(dec!(50000), false),
            None,
            &quote("AAPL", dec!(189.10), dec!(189.12), NOW),
        );

        assert!(
            assessment
                .violations()
                .iter()
                .any(|v| matches!(v, RiskViolation::PriceDeviationExceeded { .. }))
        );
    }

    #[test]
    fn limit_order_without_market_price_uses_limit_for_notional() {
        let request = order(OrderSide::Buy, limit(dec!(100)), dec!(10));
        let one_sided = Quote::new(
            Symbol::new("AAPL").unwrap(),
            None,
            Quantity::ZERO,
            None,
            Quantity::ZERO,
            at(NOW),
        );
        let assessment = evaluate(&request, &account(dec!(50000), false), None, &one_sided);
        assert!(assessment.is_approved(), "{:?}", assessment.violations());
    }

    #[test]
    fn market_order_without_market_price_is_blocked() {
        let request = order(OrderSide::Buy, OrderType::Market, dec!(10));
        let no_ask = Quote::new(
            Symbol::new("AAPL").unwrap(),
            Some(Price::new(dec!(100)).unwrap()),
            Quantity::ZERO,
            None,
            Quantity::ZERO,
            at(NOW),
        );
        let assessment = evaluate(&request, &account(dec!(50000), false), None, &no_ask);

        assert_eq!(
            assessment.violations(),
            [RiskViolation::NoExecutablePrice {
                side: OrderSide::Buy
            }]
        );
    }

    #[test]
    fn mismatched_quote_symbol_is_an_error() {
        let request = order(OrderSide::Buy, OrderType::Market, dec!(1));
        let result = policy().evaluate(&RiskContext {
            request: &request,
            account: &account(dec!(50000), false),
            position: None,
            quote: &quote("MSFT", dec!(400), dec!(400.05), NOW),
            now: at(NOW),
            kill_switch: &KillSwitch::Released,
        });

        assert!(matches!(result, Err(DomainError::SymbolMismatch { .. })));
    }
}
