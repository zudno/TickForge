use tickforge_domain::market::MarketClock;
use tickforge_domain::portfolio::{Account, AccountBalances, Position};
use tickforge_domain::shared::{Money, Price, Quantity, Symbol};
use tickforge_domain::trading::{
    ClientOrderId, Order, OrderId, OrderRequest, OrderSide, OrderSnapshot, OrderStatus, OrderType,
    TimeInForce,
};

use crate::mapping::primitives::{MappingError, decimal, timestamp};
use crate::wire::{AccountWire, ClockWire, NewOrderBodyWire, OrderWire, PositionWire};

pub(crate) fn account(wire: AccountWire) -> Result<Account, MappingError> {
    if wire.currency != "USD" {
        return Err(MappingError(format!(
            "moneda de cuenta no soportada: '{}'",
            wire.currency
        )));
    }
    let money = |field: &str, raw: &str| decimal(field, raw).map(Money::usd);

    let balances = AccountBalances {
        equity: money("equity", &wire.equity)?,
        cash: money("cash", &wire.cash)?,
        buying_power: money("buying_power", &wire.buying_power)?,
        last_equity: money("last_equity", &wire.last_equity)?,
    };
    // Cualquiera de los dos bloqueos impide operar
    let blocked = wire.trading_blocked || wire.account_blocked;
    let account = Account::new(balances, blocked)?;
    Ok(match wire.account_number {
        Some(number) => account.with_number(number),
        None => account,
    })
}

pub(crate) fn position(wire: PositionWire) -> Result<Position, MappingError> {
    if wire.side != "long" {
        return Err(MappingError(format!(
            "posición en {} con lado '{}': las posiciones en corto no están soportadas",
            wire.symbol, wire.side
        )));
    }
    Ok(Position::new(
        Symbol::new(&wire.symbol)?,
        Quantity::new(decimal("qty", &wire.qty)?)?,
        decimal("avg_entry_price", &wire.avg_entry_price)?,
    )?)
}

pub(crate) fn order(wire: OrderWire) -> Result<Order, MappingError> {
    let raw_quantity = wire.qty.as_deref().ok_or_else(|| {
        MappingError(format!(
            "la orden {} es por monto (notional), no soportada",
            wire.id
        ))
    })?;
    let quantity = Quantity::new(decimal("qty", raw_quantity)?)?;

    let side = match wire.side.as_str() {
        "buy" => OrderSide::Buy,
        "sell" => OrderSide::Sell,
        other => {
            return Err(MappingError(format!(
                "lado de orden desconocido: '{other}'"
            )));
        }
    };
    let order_type = match wire.order_type.as_str() {
        "market" => OrderType::Market,
        "limit" => {
            let raw = wire
                .limit_price
                .as_deref()
                .ok_or_else(|| MappingError("orden límite sin 'limit_price'".to_owned()))?;
            OrderType::Limit {
                limit_price: Price::new(decimal("limit_price", raw)?)?,
            }
        }
        other => {
            return Err(MappingError(format!(
                "tipo de orden no soportado por el MVP: '{other}' (orden {})",
                wire.id
            )));
        }
    };
    let time_in_force = match wire.time_in_force.as_str() {
        "day" => TimeInForce::Day,
        "gtc" => TimeInForce::Gtc,
        other => {
            return Err(MappingError(format!(
                "vigencia no soportada por el MVP: '{other}'"
            )));
        }
    };

    let request = OrderRequest::new(
        ClientOrderId::new(&wire.client_order_id)?,
        Symbol::new(&wire.symbol)?,
        side,
        order_type,
        quantity,
        time_in_force,
    )?;

    let filled_quantity = Quantity::new(decimal("filled_qty", &wire.filled_qty)?)?;
    let average_fill_price = wire
        .filled_avg_price
        .as_deref()
        .map(|raw| decimal("filled_avg_price", raw))
        .transpose()?;
    let status = order_status(&wire.status, filled_quantity, quantity)?;

    let created_at = timestamp("created_at", &wire.created_at)?;
    let submitted_at =
        optional_timestamp("submitted_at", wire.submitted_at.as_deref())?.unwrap_or(created_at);
    let updated_at =
        optional_timestamp("updated_at", wire.updated_at.as_deref())?.unwrap_or(submitted_at);

    Ok(Order::restore(OrderSnapshot {
        id: OrderId::new(&wire.id)?,
        request,
        status,
        filled_quantity,
        average_fill_price,
        reject_reason: None,
        submitted_at,
        updated_at,
    })?)
}

/// Traduce los estados de Alpaca a la máquina de estados del dominio.
///
/// Para los estados "vivos" se deriva el estado de la cantidad llenada, porque Alpaca puede
/// reportar `new` y una ejecución en respuestas que se cruzan.
fn order_status(
    raw: &str,
    filled: Quantity,
    quantity: Quantity,
) -> Result<OrderStatus, MappingError> {
    let fully_filled = filled == quantity;
    let live = if filled.is_zero() {
        OrderStatus::New
    } else if fully_filled {
        OrderStatus::Filled
    } else {
        OrderStatus::PartiallyFilled
    };

    let status = match raw {
        // Recibida por Alpaca pero aún no enviada al mercado
        "pending_new" | "accepted" | "accepted_for_bidding" if filled.is_zero() => {
            OrderStatus::PendingNew
        }
        "pending_new" | "accepted" | "accepted_for_bidding" => live,
        // Estados en los que la orden sigue viva o terminó el día sin cerrarse
        "new" | "partially_filled" | "done_for_day" | "pending_replace" | "held" | "suspended"
        | "stopped" | "calculated" => live,
        "filled" => OrderStatus::Filled,
        "pending_cancel" if fully_filled => OrderStatus::Filled,
        "pending_cancel" => OrderStatus::PendingCancel,
        // Una orden reemplazada queda cerrada; la nueva llega como otra orden
        "canceled" | "replaced" => OrderStatus::Canceled,
        "expired" => OrderStatus::Expired,
        "rejected" => OrderStatus::Rejected,
        other => {
            return Err(MappingError(format!(
                "estado de orden desconocido: '{other}'"
            )));
        }
    };
    Ok(status)
}

fn optional_timestamp(
    field: &str,
    raw: Option<&str>,
) -> Result<Option<chrono::DateTime<chrono::Utc>>, MappingError> {
    raw.map(|value| timestamp(field, value)).transpose()
}

pub(crate) fn new_order_body(request: &OrderRequest) -> NewOrderBodyWire {
    NewOrderBodyWire {
        symbol: request.symbol().as_str().to_owned(),
        qty: request.quantity().to_string(),
        side: match request.side() {
            OrderSide::Buy => "buy",
            OrderSide::Sell => "sell",
        },
        order_type: match request.order_type() {
            OrderType::Market => "market",
            OrderType::Limit { .. } => "limit",
        },
        time_in_force: match request.time_in_force() {
            TimeInForce::Day => "day",
            TimeInForce::Gtc => "gtc",
        },
        limit_price: request
            .order_type()
            .limit_price()
            .map(|price| price.to_string()),
        client_order_id: request.client_order_id().as_str().to_owned(),
    }
}

pub(crate) fn clock(wire: ClockWire) -> Result<MarketClock, MappingError> {
    Ok(MarketClock::new(
        timestamp("timestamp", &wire.timestamp)?,
        wire.is_open,
        timestamp("next_open", &wire.next_open)?,
        timestamp("next_close", &wire.next_close)?,
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;

    fn qty(value: Decimal) -> Quantity {
        Quantity::new(value).unwrap()
    }

    #[test]
    fn live_statuses_follow_filled_quantity() {
        let total = qty(dec!(10));
        assert_eq!(
            order_status("new", qty(dec!(0)), total).unwrap(),
            OrderStatus::New
        );
        assert_eq!(
            order_status("new", qty(dec!(4)), total).unwrap(),
            OrderStatus::PartiallyFilled
        );
        assert_eq!(
            order_status("partially_filled", qty(dec!(10)), total).unwrap(),
            OrderStatus::Filled
        );
        assert_eq!(
            order_status("accepted", qty(dec!(0)), total).unwrap(),
            OrderStatus::PendingNew
        );
    }

    #[test]
    fn pending_cancel_with_full_fill_is_filled() {
        let total = qty(dec!(10));
        assert_eq!(
            order_status("pending_cancel", total, total).unwrap(),
            OrderStatus::Filled
        );
    }

    #[test]
    fn unknown_status_is_an_error() {
        assert!(order_status("teleported", qty(dec!(0)), qty(dec!(1))).is_err());
    }
}
