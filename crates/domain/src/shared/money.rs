use std::fmt;

use rust_decimal::{Decimal, RoundingStrategy};

use crate::error::DomainError;
use crate::shared::{Price, Quantity};

/// Moneda de un monto. Por ahora el sistema solo opera el mercado de EE. UU.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Currency {
    Usd,
}

impl Currency {
    /// Código ISO 4217.
    pub fn code(self) -> &'static str {
        match self {
            Self::Usd => "USD",
        }
    }

    /// Número de decimales de la unidad mínima (centavos).
    pub fn minor_units(self) -> u32 {
        match self {
            Self::Usd => 2,
        }
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

/// Monto monetario con su moneda.
///
/// A diferencia de `Price`, puede ser negativo (P&L, flujos de salida). Los cálculos intermedios
/// conservan toda la precisión; solo se redondea al mostrar o al reportar con `rounded`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Money {
    amount: Decimal,
    currency: Currency,
}

impl Money {
    pub fn new(amount: Decimal, currency: Currency) -> Self {
        Self { amount, currency }
    }

    pub fn usd(amount: Decimal) -> Self {
        Self::new(amount, Currency::Usd)
    }

    pub fn zero(currency: Currency) -> Self {
        Self::new(Decimal::ZERO, currency)
    }

    /// Valor nocional de una operación: `price * quantity`.
    pub fn notional(
        price: Price,
        quantity: Quantity,
        currency: Currency,
    ) -> Result<Self, DomainError> {
        price
            .value()
            .checked_mul(quantity.value())
            .map(|amount| Self::new(amount, currency))
            .ok_or(DomainError::ArithmeticOverflow("notional"))
    }

    pub fn amount(self) -> Decimal {
        self.amount
    }

    pub fn currency(self) -> Currency {
        self.currency
    }

    pub fn is_negative(self) -> bool {
        self.amount.is_sign_negative() && !self.amount.is_zero()
    }

    pub fn abs(self) -> Self {
        Self::new(self.amount.abs(), self.currency)
    }

    pub fn checked_add(self, other: Money) -> Result<Money, DomainError> {
        self.ensure_same_currency(other)?;
        self.amount
            .checked_add(other.amount)
            .map(|amount| Self::new(amount, self.currency))
            .ok_or(DomainError::ArithmeticOverflow("money add"))
    }

    pub fn checked_sub(self, other: Money) -> Result<Money, DomainError> {
        self.ensure_same_currency(other)?;
        self.amount
            .checked_sub(other.amount)
            .map(|amount| Self::new(amount, self.currency))
            .ok_or(DomainError::ArithmeticOverflow("money sub"))
    }

    /// Compara montos de la misma moneda; falla si las monedas difieren.
    pub fn exceeds(self, other: Money) -> Result<bool, DomainError> {
        self.ensure_same_currency(other)?;
        Ok(self.amount > other.amount)
    }

    /// Redondea a la unidad mínima de la moneda con redondeo bancario (mitad al par),
    /// que evita el sesgo acumulado del redondeo hacia arriba.
    pub fn rounded(self) -> Self {
        let amount = self.amount.round_dp_with_strategy(
            self.currency.minor_units(),
            RoundingStrategy::MidpointNearestEven,
        );
        Self::new(amount, self.currency)
    }

    fn ensure_same_currency(self, other: Money) -> Result<(), DomainError> {
        if self.currency != other.currency {
            return Err(DomainError::CurrencyMismatch {
                left: self.currency,
                right: other.currency,
            });
        }
        Ok(())
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let precision = self.currency.minor_units() as usize;
        let rounded = self.rounded().amount;
        // Evita mostrar "-0.00" cuando un monto negativo diminuto se redondea a cero
        let amount = if rounded.is_zero() {
            Decimal::ZERO
        } else {
            rounded
        };
        write!(f, "{amount:.precision$} {}", self.currency)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal_macros::dec;

    #[test]
    fn computes_notional_value() {
        let price = Price::new(dec!(189.25)).unwrap();
        let quantity = Quantity::new(dec!(10)).unwrap();

        let notional = Money::notional(price, quantity, Currency::Usd).unwrap();
        assert_eq!(notional, Money::usd(dec!(1892.50)));
    }

    #[test]
    fn fractional_notional_keeps_full_precision() {
        let price = Price::new(dec!(100.01)).unwrap();
        let quantity = Quantity::new(dec!(0.333)).unwrap();

        let notional = Money::notional(price, quantity, Currency::Usd).unwrap();
        assert_eq!(notional.amount(), dec!(33.30333));
        assert_eq!(notional.rounded().amount(), dec!(33.30));
    }

    #[test]
    fn rounds_half_to_even() {
        assert_eq!(Money::usd(dec!(0.125)).rounded().amount(), dec!(0.12));
        assert_eq!(Money::usd(dec!(0.135)).rounded().amount(), dec!(0.14));
    }

    #[test]
    fn subtraction_can_go_negative() {
        let pnl = Money::usd(dec!(10))
            .checked_sub(Money::usd(dec!(25)))
            .unwrap();
        assert!(pnl.is_negative());
        assert_eq!(pnl.abs(), Money::usd(dec!(15)));
    }

    #[test]
    fn displays_with_currency_and_minor_units() {
        assert_eq!(Money::usd(dec!(1234.5)).to_string(), "1234.50 USD");
        assert_eq!(Money::usd(dec!(-0.005)).to_string(), "0.00 USD");
    }

    #[test]
    fn comparison_respects_currency() {
        assert!(Money::usd(dec!(10)).exceeds(Money::usd(dec!(5))).unwrap());
        assert!(!Money::usd(dec!(5)).exceeds(Money::usd(dec!(5))).unwrap());
    }

    fn arb_money() -> impl Strategy<Value = Money> {
        (-1_000_000_000_000i64..1_000_000_000_000, 0u32..=6)
            .prop_map(|(m, s)| Money::usd(Decimal::new(m, s)))
    }

    proptest! {
        // La suma de montos es conmutativa
        #[test]
        fn add_is_commutative(a in arb_money(), b in arb_money()) {
            prop_assert_eq!(a.checked_add(b).unwrap(), b.checked_add(a).unwrap());
        }

        // Sumar y restar el mismo monto regresa al original
        #[test]
        fn add_then_sub_roundtrips(a in arb_money(), b in arb_money()) {
            prop_assert_eq!(a.checked_add(b).unwrap().checked_sub(b).unwrap(), a);
        }
    }
}
