use std::fmt;

use rust_decimal::Decimal;

use crate::error::DomainError;

/// Precio unitario de un instrumento, siempre estrictamente positivo.
///
/// Se guarda normalizado (sin ceros a la derecha), así `1.50` y `1.5` son el mismo valor.
/// La validación del tick mínimo según el rango de precio (Reg NMS 612) pertenece a la
/// validación de órdenes, no a este value object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Price(Decimal);

impl Price {
    /// Máximo de decimales: las acciones bajo 1 USD cotizan en incrementos de 0.0001.
    pub const MAX_SCALE: u32 = 4;

    pub fn new(value: Decimal) -> Result<Self, DomainError> {
        if value <= Decimal::ZERO {
            return Err(DomainError::InvalidPrice(value));
        }

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

    /// Desviación porcentual absoluta respecto a un precio de referencia.
    ///
    /// Se usa en el control fat-finger: `|self - reference| / reference * 100`.
    pub fn deviation_pct(self, reference: Price) -> Result<Decimal, DomainError> {
        const OPERATION: &str = "price deviation";

        // `reference` es estrictamente positivo por construcción, así que no hay división entre cero
        self.0
            .checked_sub(reference.0)
            .and_then(|diff| diff.checked_div(reference.0))
            .and_then(|ratio| ratio.checked_mul(Decimal::ONE_HUNDRED))
            .map(|pct| pct.abs())
            .ok_or(DomainError::ArithmeticOverflow(OPERATION))
    }
}

impl fmt::Display for Price {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl TryFrom<Decimal> for Price {
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

    #[test]
    fn accepts_positive_values() {
        assert_eq!(Price::new(dec!(189.25)).unwrap().value(), dec!(189.25));
        assert_eq!(Price::new(dec!(0.0001)).unwrap().value(), dec!(0.0001));
    }

    #[test]
    fn rejects_zero_and_negative_values() {
        assert_eq!(
            Price::new(Decimal::ZERO),
            Err(DomainError::InvalidPrice(Decimal::ZERO))
        );
        assert!(matches!(
            Price::new(dec!(-1)),
            Err(DomainError::InvalidPrice(_))
        ));
    }

    #[test]
    fn rejects_excess_precision() {
        assert!(matches!(
            Price::new(dec!(10.00001)),
            Err(DomainError::PrecisionExceeded { max_scale: 4, .. })
        ));
    }

    #[test]
    fn trailing_zeros_do_not_count_as_precision() {
        let price = Price::new(dec!(10.500000)).unwrap();
        assert_eq!(price, Price::new(dec!(10.5)).unwrap());
        assert_eq!(price.value().scale(), 1);
    }

    #[test]
    fn computes_absolute_deviation() {
        let reference = Price::new(dec!(100)).unwrap();
        let above = Price::new(dec!(105)).unwrap();
        let below = Price::new(dec!(95)).unwrap();

        assert_eq!(above.deviation_pct(reference).unwrap(), dec!(5));
        assert_eq!(below.deviation_pct(reference).unwrap(), dec!(5));
    }

    proptest! {
        // La desviación de un precio contra sí mismo siempre es cero
        #[test]
        fn self_deviation_is_zero(mantissa in 1i64..10_000_000_000, scale in 0u32..=4) {
            let price = Price::new(Decimal::new(mantissa, scale)).unwrap();
            prop_assert_eq!(price.deviation_pct(price).unwrap(), Decimal::ZERO);
        }
    }
}
