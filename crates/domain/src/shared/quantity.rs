use std::fmt;

use rust_decimal::Decimal;

use crate::error::DomainError;

/// Cantidad de acciones, no negativa y con soporte para fracciones.
///
/// Se permite cero porque representa estados válidos (por ejemplo, la cantidad llenada de una
/// orden recién creada). Que una orden tenga cantidad mayor que cero lo valida `OrderRequest`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Quantity(Decimal);

impl Quantity {
    /// Máximo de decimales para acciones fraccionadas (límite de Alpaca).
    pub const MAX_SCALE: u32 = 9;

    pub const ZERO: Quantity = Quantity(Decimal::ZERO);

    pub fn new(value: Decimal) -> Result<Self, DomainError> {
        if value.is_sign_negative() && !value.is_zero() {
            return Err(DomainError::InvalidQuantity(value));
        }

        // `normalize` también convierte -0 en 0
        let normalized = value.normalize();
        if normalized.scale() > Self::MAX_SCALE {
            return Err(DomainError::PrecisionExceeded {
                value,
                max_scale: Self::MAX_SCALE,
            });
        }
        Ok(Self(normalized))
    }

    pub fn value(self) -> Decimal {
        self.0
    }

    pub fn is_zero(self) -> bool {
        self.0.is_zero()
    }

    /// Indica si es un número entero de acciones (sin fracción).
    pub fn is_whole(self) -> bool {
        self.0.fract().is_zero()
    }

    pub fn checked_add(self, other: Quantity) -> Result<Quantity, DomainError> {
        let sum = self
            .0
            .checked_add(other.0)
            .ok_or(DomainError::ArithmeticOverflow("quantity add"))?;
        Self::new(sum)
    }

    /// Resta que falla si el resultado sería negativo (por ejemplo, llenar más de lo ordenado).
    pub fn checked_sub(self, other: Quantity) -> Result<Quantity, DomainError> {
        let difference = self
            .0
            .checked_sub(other.0)
            .ok_or(DomainError::ArithmeticOverflow("quantity sub"))?;
        Self::new(difference)
    }
}

impl fmt::Display for Quantity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl TryFrom<Decimal> for Quantity {
    type Error = DomainError;

    fn try_from(value: Decimal) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal_macros::dec;

    fn qty(value: Decimal) -> Quantity {
        Quantity::new(value).unwrap()
    }

    #[test]
    fn accepts_zero_whole_and_fractional_values() {
        assert!(qty(Decimal::ZERO).is_zero());
        assert!(qty(dec!(10)).is_whole());
        assert!(!qty(dec!(0.5)).is_whole());
    }

    #[test]
    fn negative_zero_becomes_zero() {
        let negative_zero = Decimal::new(0, 2).checked_mul(dec!(-1)).unwrap();
        assert_eq!(qty(negative_zero), Quantity::ZERO);
    }

    #[test]
    fn rejects_negative_values() {
        assert!(matches!(
            Quantity::new(dec!(-1)),
            Err(DomainError::InvalidQuantity(_))
        ));
    }

    #[test]
    fn rejects_excess_precision() {
        assert!(matches!(
            Quantity::new(dec!(0.0000000001)),
            Err(DomainError::PrecisionExceeded { max_scale: 9, .. })
        ));
    }

    #[test]
    fn subtraction_below_zero_fails() {
        let result = qty(dec!(5)).checked_sub(qty(dec!(6)));
        assert!(matches!(result, Err(DomainError::InvalidQuantity(_))));
    }

    fn arb_quantity() -> impl Strategy<Value = Quantity> {
        (0i64..1_000_000_000_000, 0u32..=9).prop_map(|(m, s)| qty(Decimal::new(m, s)))
    }

    proptest! {
        // Sumar y luego restar la misma cantidad regresa al valor original
        #[test]
        fn add_then_sub_roundtrips(a in arb_quantity(), b in arb_quantity()) {
            let sum = a.checked_add(b).unwrap();
            prop_assert_eq!(sum.checked_sub(b).unwrap(), a);
        }

        // La suma es conmutativa
        #[test]
        fn add_is_commutative(a in arb_quantity(), b in arb_quantity()) {
            prop_assert_eq!(a.checked_add(b).unwrap(), b.checked_add(a).unwrap());
        }
    }
}
