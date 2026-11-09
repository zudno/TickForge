use rust_decimal::Decimal;

use crate::error::DomainError;
use crate::shared::Money;

/// Saldos de la cuenta tal como los reporta el broker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccountBalances {
    /// Valor total de la cuenta: efectivo + valor de mercado de las posiciones.
    pub equity: Money,
    /// Efectivo disponible; puede ser negativo en cuentas de margen.
    pub cash: Money,
    /// Monto máximo que se puede comprar ahora mismo.
    pub buying_power: Money,
    /// Equity al cierre de la sesión anterior; base del P&L del día.
    pub last_equity: Money,
}

/// Cuenta de corretaje del usuario.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Account {
    balances: AccountBalances,
    trading_blocked: bool,
}

impl Account {
    pub fn new(balances: AccountBalances, trading_blocked: bool) -> Result<Self, DomainError> {
        let AccountBalances {
            equity,
            cash,
            buying_power,
            last_equity,
        } = balances;

        // Todos los saldos deben estar en la misma moneda
        for other in [cash, buying_power, last_equity] {
            if other.currency() != equity.currency() {
                return Err(DomainError::CurrencyMismatch {
                    left: equity.currency(),
                    right: other.currency(),
                });
            }
        }
        if buying_power.is_negative() {
            return Err(DomainError::InvalidAccount(
                "el poder de compra no puede ser negativo",
            ));
        }

        Ok(Self {
            balances,
            trading_blocked,
        })
    }

    pub fn equity(&self) -> Money {
        self.balances.equity
    }

    pub fn cash(&self) -> Money {
        self.balances.cash
    }

    pub fn buying_power(&self) -> Money {
        self.balances.buying_power
    }

    pub fn last_equity(&self) -> Money {
        self.balances.last_equity
    }

    /// El broker bloqueó la operación (por ejemplo, por una restricción regulatoria).
    pub fn is_trading_blocked(&self) -> bool {
        self.trading_blocked
    }

    /// Ganancia o pérdida del día: `equity - last_equity`.
    pub fn day_pnl(&self) -> Result<Money, DomainError> {
        self.balances.equity.checked_sub(self.balances.last_equity)
    }

    /// P&L del día en porcentaje; `None` si no hay equity previo con el cual comparar.
    pub fn day_pnl_pct(&self) -> Result<Option<Decimal>, DomainError> {
        let base = self.balances.last_equity.amount();
        if base <= Decimal::ZERO {
            return Ok(None);
        }
        self.day_pnl()?
            .amount()
            .checked_div(base)
            .and_then(|ratio| ratio.checked_mul(Decimal::ONE_HUNDRED))
            .map(Some)
            .ok_or(DomainError::ArithmeticOverflow("day pnl pct"))
    }

    /// Indica si el poder de compra alcanza para cubrir `amount`.
    pub fn can_afford(&self, amount: Money) -> Result<bool, DomainError> {
        Ok(!amount.exceeds(self.balances.buying_power)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn balances(equity: Decimal, last_equity: Decimal, buying_power: Decimal) -> AccountBalances {
        AccountBalances {
            equity: Money::usd(equity),
            cash: Money::usd(dec!(1000)),
            buying_power: Money::usd(buying_power),
            last_equity: Money::usd(last_equity),
        }
    }

    #[test]
    fn computes_day_pnl() {
        let account = Account::new(balances(dec!(10500), dec!(10000), dec!(5000)), false).unwrap();

        assert_eq!(account.day_pnl().unwrap(), Money::usd(dec!(500)));
        assert_eq!(account.day_pnl_pct().unwrap(), Some(dec!(5)));
    }

    #[test]
    fn day_pnl_pct_is_none_without_previous_equity() {
        let account = Account::new(balances(dec!(100), dec!(0), dec!(100)), false).unwrap();
        assert_eq!(account.day_pnl_pct().unwrap(), None);
    }

    #[test]
    fn rejects_negative_buying_power() {
        assert!(matches!(
            Account::new(balances(dec!(100), dec!(100), dec!(-1)), false),
            Err(DomainError::InvalidAccount(_))
        ));
    }

    #[test]
    fn can_afford_up_to_buying_power() {
        let account = Account::new(balances(dec!(100), dec!(100), dec!(500)), false).unwrap();

        assert!(account.can_afford(Money::usd(dec!(500))).unwrap());
        assert!(!account.can_afford(Money::usd(dec!(500.01))).unwrap());
    }
}
