use rust_decimal::Decimal;

use crate::error::DomainError;
use crate::shared::{Currency, Money, Price, Quantity, Symbol};
use crate::trading::{ClientOrderId, OrderSide, OrderType, TimeInForce};

/// Intención validada de colocar una orden, antes de enviarla al broker.
///
/// Es lo que construye la TUI (o el motor de decisiones de ML) y lo que evalúa la política
/// de riesgo. Solo se convierte en `Order` cuando el broker le asigna un `OrderId`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderRequest {
    client_order_id: ClientOrderId,
    symbol: Symbol,
    side: OrderSide,
    order_type: OrderType,
    quantity: Quantity,
    time_in_force: TimeInForce,
}

impl OrderRequest {
    pub fn new(
        client_order_id: ClientOrderId,
        symbol: Symbol,
        side: OrderSide,
        order_type: OrderType,
        quantity: Quantity,
        time_in_force: TimeInForce,
    ) -> Result<Self, DomainError> {
        // Una orden siempre debe pedir al menos una fracción de acción
        if quantity.is_zero() {
            return Err(DomainError::NonPositiveQuantity(quantity.value()));
        }
        if let Some(limit_price) = order_type.limit_price() {
            ensure_valid_tick(limit_price)?;
        }

        Ok(Self {
            client_order_id,
            symbol,
            side,
            order_type,
            quantity,
            time_in_force,
        })
    }

    pub fn client_order_id(&self) -> &ClientOrderId {
        &self.client_order_id
    }

    pub fn symbol(&self) -> &Symbol {
        &self.symbol
    }

    pub fn side(&self) -> OrderSide {
        self.side
    }

    pub fn order_type(&self) -> OrderType {
        self.order_type
    }

    pub fn quantity(&self) -> Quantity {
        self.quantity
    }

    pub fn time_in_force(&self) -> TimeInForce {
        self.time_in_force
    }

    /// Precio con el que se estima el costo: el límite si existe; si no, el de mercado.
    pub fn reference_price(&self, market_price: Price) -> Price {
        self.order_type.limit_price().unwrap_or(market_price)
    }

    /// Valor nocional estimado de la orden, usado por la política de riesgo y la vista previa.
    pub fn estimated_notional(&self, market_price: Price) -> Result<Money, DomainError> {
        Money::notional(
            self.reference_price(market_price),
            self.quantity,
            Currency::Usd,
        )
    }
}

/// Incremento mínimo de precio según la Regla 612 de Reg NMS:
/// 0.01 USD para precios desde 1 USD y 0.0001 USD por debajo.
///
/// Nota: la enmienda de 2024 que introduce incrementos de 0.005 USD para algunos valores
/// no está modelada; esas órdenes se rechazan aquí de forma conservadora.
fn minimum_tick(price: Price) -> Decimal {
    if price.value() >= Decimal::ONE {
        Decimal::new(1, 2)
    } else {
        Decimal::new(1, 4)
    }
}

fn ensure_valid_tick(price: Price) -> Result<(), DomainError> {
    let tick = minimum_tick(price);
    let remainder = price
        .value()
        .checked_rem(tick)
        .ok_or(DomainError::ArithmeticOverflow("tick size check"))?;

    if !remainder.is_zero() {
        return Err(DomainError::InvalidTickSize {
            price: price.value(),
            tick,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn request(order_type: OrderType, quantity: Decimal) -> Result<OrderRequest, DomainError> {
        OrderRequest::new(
            ClientOrderId::new("tf-1").unwrap(),
            Symbol::new("AAPL").unwrap(),
            OrderSide::Buy,
            order_type,
            Quantity::new(quantity).unwrap(),
            TimeInForce::Day,
        )
    }

    fn limit(price: Decimal) -> OrderType {
        OrderType::Limit {
            limit_price: Price::new(price).unwrap(),
        }
    }

    #[test]
    fn rejects_zero_quantity() {
        assert!(matches!(
            request(OrderType::Market, Decimal::ZERO),
            Err(DomainError::NonPositiveQuantity(_))
        ));
    }

    #[test]
    fn accepts_valid_ticks() {
        assert!(request(limit(dec!(189.25)), dec!(1)).is_ok());
        assert!(request(limit(dec!(1)), dec!(1)).is_ok());
        assert!(request(limit(dec!(0.5123)), dec!(1)).is_ok());
    }

    #[test]
    fn rejects_sub_penny_limit_above_one_dollar() {
        assert_eq!(
            request(limit(dec!(10.005)), dec!(1)),
            Err(DomainError::InvalidTickSize {
                price: dec!(10.005),
                tick: dec!(0.01)
            })
        );
    }

    #[test]
    fn estimated_notional_uses_limit_price_when_present() {
        let market_price = Price::new(dec!(200)).unwrap();

        let limit_order = request(limit(dec!(190)), dec!(10)).unwrap();
        let market_order = request(OrderType::Market, dec!(10)).unwrap();

        assert_eq!(
            limit_order.estimated_notional(market_price).unwrap(),
            Money::usd(dec!(1900))
        );
        assert_eq!(
            market_order.estimated_notional(market_price).unwrap(),
            Money::usd(dec!(2000))
        );
    }
}
