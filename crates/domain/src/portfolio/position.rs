use rust_decimal::Decimal;

use crate::error::DomainError;
use crate::shared::{Currency, Money, Price, Quantity, Symbol};

/// Posición larga en un instrumento (las ventas en corto quedan fuera del MVP).
///
/// Guarda el costo total (`cost_basis`) en lugar del precio promedio para no acumular
/// errores de redondeo; el precio promedio se deriva con precisión completa.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Position {
    symbol: Symbol,
    quantity: Quantity,
    cost_basis: Money,
}

impl Position {
    /// Reconstruye una posición a partir de lo que reporta el broker.
    pub fn new(
        symbol: Symbol,
        quantity: Quantity,
        average_entry_price: Decimal,
    ) -> Result<Self, DomainError> {
        if quantity.is_zero() {
            return Err(DomainError::NonPositiveQuantity(quantity.value()));
        }
        if average_entry_price <= Decimal::ZERO {
            return Err(DomainError::InvalidPrice(average_entry_price));
        }
        let cost_basis = average_entry_price
            .checked_mul(quantity.value())
            .map(Money::usd)
            .ok_or(DomainError::ArithmeticOverflow("position cost basis"))?;

        Ok(Self {
            symbol,
            quantity,
            cost_basis,
        })
    }

    /// Abre una posición con la primera compra.
    pub fn open(symbol: Symbol, quantity: Quantity, price: Price) -> Result<Self, DomainError> {
        if quantity.is_zero() {
            return Err(DomainError::NonPositiveQuantity(quantity.value()));
        }
        Ok(Self {
            symbol,
            quantity,
            cost_basis: Money::notional(price, quantity, Currency::Usd)?,
        })
    }

    pub fn symbol(&self) -> &Symbol {
        &self.symbol
    }

    pub fn quantity(&self) -> Quantity {
        self.quantity
    }

    pub fn cost_basis(&self) -> Money {
        self.cost_basis
    }

    /// Una posición sin acciones ya está cerrada.
    pub fn is_flat(&self) -> bool {
        self.quantity.is_zero()
    }

    /// Precio promedio de entrada; `None` si la posición está cerrada.
    pub fn average_entry_price(&self) -> Option<Decimal> {
        if self.is_flat() {
            return None;
        }
        self.cost_basis.amount().checked_div(self.quantity.value())
    }

    pub fn market_value(&self, price: Price) -> Result<Money, DomainError> {
        Money::notional(price, self.quantity, self.cost_basis.currency())
    }

    pub fn unrealized_pnl(&self, price: Price) -> Result<Money, DomainError> {
        self.market_value(price)?.checked_sub(self.cost_basis)
    }

    /// P&L no realizado en porcentaje sobre el costo; `None` si la posición está cerrada.
    pub fn unrealized_pnl_pct(&self, price: Price) -> Result<Option<Decimal>, DomainError> {
        let cost = self.cost_basis.amount();
        if cost <= Decimal::ZERO {
            return Ok(None);
        }
        self.unrealized_pnl(price)?
            .amount()
            .checked_div(cost)
            .and_then(|ratio| ratio.checked_mul(Decimal::ONE_HUNDRED))
            .map(Some)
            .ok_or(DomainError::ArithmeticOverflow("unrealized pnl pct"))
    }

    /// Aumenta la posición con una compra ejecutada.
    pub fn apply_buy(&mut self, quantity: Quantity, price: Price) -> Result<(), DomainError> {
        let added_cost = Money::notional(price, quantity, self.cost_basis.currency())?;
        let new_quantity = self.quantity.checked_add(quantity)?;
        let new_cost = self.cost_basis.checked_add(added_cost)?;

        self.quantity = new_quantity;
        self.cost_basis = new_cost;
        Ok(())
    }

    /// Reduce la posición con una venta ejecutada y devuelve el P&L realizado.
    ///
    /// El costo que sale de la posición se calcula al precio promedio (método de costo promedio).
    pub fn apply_sell(&mut self, quantity: Quantity, price: Price) -> Result<Money, DomainError> {
        if quantity > self.quantity {
            return Err(DomainError::InsufficientPosition {
                requested: quantity,
                available: self.quantity,
            });
        }

        let currency = self.cost_basis.currency();
        let new_quantity = self.quantity.checked_sub(quantity)?;
        let removed_cost = if new_quantity.is_zero() {
            // Al cerrar por completo sale todo el costo, sin residuos de redondeo
            self.cost_basis
        } else {
            self.cost_basis
                .amount()
                .checked_mul(quantity.value())
                .and_then(|value| value.checked_div(self.quantity.value()))
                .map(|amount| Money::new(amount, currency))
                .ok_or(DomainError::ArithmeticOverflow("position removed cost"))?
        };
        let proceeds = Money::notional(price, quantity, currency)?;
        let realized_pnl = proceeds.checked_sub(removed_cost)?;
        let new_cost = self.cost_basis.checked_sub(removed_cost)?;

        self.quantity = new_quantity;
        self.cost_basis = new_cost;
        Ok(realized_pnl)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal_macros::dec;

    fn symbol() -> Symbol {
        Symbol::new("AAPL").unwrap()
    }

    fn qty(value: Decimal) -> Quantity {
        Quantity::new(value).unwrap()
    }

    fn price(value: Decimal) -> Price {
        Price::new(value).unwrap()
    }

    #[test]
    fn buys_update_weighted_average() {
        let mut position = Position::open(symbol(), qty(dec!(10)), price(dec!(100))).unwrap();
        position.apply_buy(qty(dec!(10)), price(dec!(110))).unwrap();

        assert_eq!(position.quantity(), qty(dec!(20)));
        assert_eq!(position.average_entry_price(), Some(dec!(105)));
    }

    #[test]
    fn computes_unrealized_pnl() {
        let position = Position::new(symbol(), qty(dec!(10)), dec!(100)).unwrap();

        assert_eq!(
            position.market_value(price(dec!(120))).unwrap(),
            Money::usd(dec!(1200))
        );
        assert_eq!(
            position.unrealized_pnl(price(dec!(120))).unwrap(),
            Money::usd(dec!(200))
        );
        assert_eq!(
            position.unrealized_pnl_pct(price(dec!(90))).unwrap(),
            Some(dec!(-10))
        );
    }

    #[test]
    fn partial_sell_realizes_pnl_at_average_cost() {
        let mut position = Position::new(symbol(), qty(dec!(10)), dec!(100)).unwrap();

        let realized = position.apply_sell(qty(dec!(4)), price(dec!(110))).unwrap();
        assert_eq!(realized, Money::usd(dec!(40)));
        assert_eq!(position.quantity(), qty(dec!(6)));
        assert_eq!(position.average_entry_price(), Some(dec!(100)));
    }

    #[test]
    fn full_sell_flattens_position() {
        let mut position = Position::new(symbol(), qty(dec!(3)), dec!(33.3333)).unwrap();

        position.apply_sell(qty(dec!(3)), price(dec!(30))).unwrap();
        assert!(position.is_flat());
        assert!(position.cost_basis().amount().is_zero());
        assert_eq!(position.average_entry_price(), None);
    }

    #[test]
    fn cannot_sell_more_than_held() {
        let mut position = Position::new(symbol(), qty(dec!(5)), dec!(100)).unwrap();
        let before = position.clone();

        assert!(matches!(
            position.apply_sell(qty(dec!(6)), price(dec!(100))),
            Err(DomainError::InsufficientPosition { .. })
        ));
        assert_eq!(position, before);
    }

    #[test]
    fn rejects_invalid_broker_data() {
        assert!(Position::new(symbol(), Quantity::ZERO, dec!(100)).is_err());
        assert!(Position::new(symbol(), qty(dec!(1)), dec!(0)).is_err());
    }

    proptest! {
        // Comprar y luego vender todo al mismo precio deja la posición plana y sin P&L
        #[test]
        fn round_trip_at_same_price_has_zero_pnl(
            first in 1i64..10_000,
            second in 1i64..10_000,
            cents in 1i64..1_000_000,
        ) {
            let fill_price = price(Decimal::new(cents, 2));
            let mut position = Position::open(symbol(), qty(Decimal::from(first)), fill_price).unwrap();
            position.apply_buy(qty(Decimal::from(second)), fill_price).unwrap();

            let realized = position.apply_sell(position.quantity(), fill_price).unwrap();
            prop_assert!(position.is_flat());
            prop_assert!(realized.amount().is_zero());
        }
    }
}
