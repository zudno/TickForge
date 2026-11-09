use chrono::{DateTime, TimeDelta, Utc};
use rust_decimal::Decimal;

use crate::error::DomainError;
use crate::shared::{Price, Quantity, Symbol};
use crate::trading::OrderSide;

/// Mejor oferta de compra (bid) y de venta (ask) de un instrumento (NBBO o del feed disponible).
///
/// `bid` y `ask` son opcionales porque fuera de horario o en instrumentos poco líquidos un lado
/// puede no tener ofertas. Se aceptan cotizaciones cruzadas (`bid > ask`), que ocurren
/// momentáneamente en los feeds; `is_crossed` permite detectarlas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quote {
    symbol: Symbol,
    bid: Option<Price>,
    bid_size: Quantity,
    ask: Option<Price>,
    ask_size: Quantity,
    quoted_at: DateTime<Utc>,
}

impl Quote {
    pub fn new(
        symbol: Symbol,
        bid: Option<Price>,
        bid_size: Quantity,
        ask: Option<Price>,
        ask_size: Quantity,
        quoted_at: DateTime<Utc>,
    ) -> Self {
        Self {
            symbol,
            bid,
            bid_size,
            ask,
            ask_size,
            quoted_at,
        }
    }

    pub fn symbol(&self) -> &Symbol {
        &self.symbol
    }

    pub fn bid(&self) -> Option<Price> {
        self.bid
    }

    pub fn bid_size(&self) -> Quantity {
        self.bid_size
    }

    pub fn ask(&self) -> Option<Price> {
        self.ask
    }

    pub fn ask_size(&self) -> Quantity {
        self.ask_size
    }

    pub fn quoted_at(&self) -> DateTime<Utc> {
        self.quoted_at
    }

    /// Precio al que se ejecutaría una orden a mercado: el ask para comprar, el bid para vender.
    pub fn executable_price(&self, side: OrderSide) -> Option<Price> {
        match side {
            OrderSide::Buy => self.ask,
            OrderSide::Sell => self.bid,
        }
    }

    /// Precio medio entre bid y ask (puede tener más decimales que un `Price`).
    pub fn mid_price(&self) -> Result<Option<Decimal>, DomainError> {
        let (Some(bid), Some(ask)) = (self.bid, self.ask) else {
            return Ok(None);
        };
        bid.value()
            .checked_add(ask.value())
            .and_then(|sum| sum.checked_div(Decimal::TWO))
            .map(Some)
            .ok_or(DomainError::ArithmeticOverflow("mid price"))
    }

    /// Diferencial `ask - bid`; negativo si la cotización está cruzada.
    pub fn spread(&self) -> Result<Option<Decimal>, DomainError> {
        let (Some(bid), Some(ask)) = (self.bid, self.ask) else {
            return Ok(None);
        };
        ask.value()
            .checked_sub(bid.value())
            .map(Some)
            .ok_or(DomainError::ArithmeticOverflow("spread"))
    }

    pub fn is_crossed(&self) -> bool {
        matches!((self.bid, self.ask), (Some(bid), Some(ask)) if bid > ask)
    }

    /// Antigüedad de la cotización respecto a `now`; nunca negativa (tolera desfase de relojes).
    pub fn age(&self, now: DateTime<Utc>) -> TimeDelta {
        now.signed_duration_since(self.quoted_at)
            .max(TimeDelta::zero())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn quote(bid: Option<Decimal>, ask: Option<Decimal>) -> Quote {
        Quote::new(
            Symbol::new("AAPL").unwrap(),
            bid.map(|value| Price::new(value).unwrap()),
            Quantity::new(dec!(100)).unwrap(),
            ask.map(|value| Price::new(value).unwrap()),
            Quantity::new(dec!(200)).unwrap(),
            DateTime::from_timestamp(1_000, 0).unwrap(),
        )
    }

    #[test]
    fn executable_price_depends_on_side() {
        let quote = quote(Some(dec!(189.10)), Some(dec!(189.12)));
        assert_eq!(
            quote.executable_price(OrderSide::Buy).unwrap().value(),
            dec!(189.12)
        );
        assert_eq!(
            quote.executable_price(OrderSide::Sell).unwrap().value(),
            dec!(189.10)
        );
    }

    #[test]
    fn computes_mid_and_spread() {
        let quote = quote(Some(dec!(10.00)), Some(dec!(10.01)));
        assert_eq!(quote.mid_price().unwrap(), Some(dec!(10.005)));
        assert_eq!(quote.spread().unwrap(), Some(dec!(0.01)));
        assert!(!quote.is_crossed());
    }

    #[test]
    fn one_sided_quote_has_no_mid() {
        let quote = quote(None, Some(dec!(10)));
        assert_eq!(quote.mid_price().unwrap(), None);
        assert_eq!(quote.executable_price(OrderSide::Sell), None);
    }

    #[test]
    fn detects_crossed_quotes() {
        assert!(quote(Some(dec!(10.02)), Some(dec!(10.01))).is_crossed());
    }

    #[test]
    fn age_is_never_negative() {
        let quote = quote(Some(dec!(1)), Some(dec!(1)));
        let earlier = DateTime::from_timestamp(900, 0).unwrap();
        let later = DateTime::from_timestamp(1_005, 0).unwrap();

        assert_eq!(quote.age(earlier), TimeDelta::zero());
        assert_eq!(quote.age(later), TimeDelta::seconds(5));
    }
}
