use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde_json::Value;
use tickforge_application::ports::{MarketEvent, OrderUpdate, OrderUpdateKind};
use tickforge_domain::market::{Bar, Ohlc, Quote, Timeframe, TradeTick};
use tickforge_domain::shared::{Price, Quantity, Symbol};
use tickforge_domain::trading::{Fill, FillId, OrderId};

use crate::mapping::market::optional_price;
use crate::mapping::primitives::{MappingError, decimal, number, timestamp};
use crate::wire::{StreamBarWire, StreamQuoteWire, StreamTradeWire, TradeUpdateWire};

// --- Stream de trading ---

/// Traduce un evento `trade_updates` a una actualización del puerto.
///
/// Devuelve `Ok(None)` para eventos que no cambian el modelo del dominio
/// (`pending_new`, `accepted`, `done_for_day`, `pending_cancel`, etc.).
pub(crate) fn trade_update(wire: TradeUpdateWire) -> Result<Option<OrderUpdate>, MappingError> {
    let order_id = OrderId::new(&wire.order.id)?;
    let raw_time = wire
        .timestamp
        .as_deref()
        .or(wire.order.updated_at.as_deref())
        .ok_or_else(|| MappingError("evento de orden sin marca de tiempo".to_owned()))?;
    let occurred_at = timestamp("timestamp", raw_time)?;

    let kind = match wire.event.as_str() {
        "new" => OrderUpdateKind::Accepted,
        "fill" | "partial_fill" => OrderUpdateKind::Filled(fill(&wire, occurred_at)?),
        // Una orden reemplazada queda cerrada; la nueva llega como otra orden
        "canceled" | "replaced" => OrderUpdateKind::Canceled,
        "expired" => OrderUpdateKind::Expired,
        "rejected" => OrderUpdateKind::Rejected {
            reason: "rechazada por el broker".to_owned(),
        },
        "order_cancel_rejected" => OrderUpdateKind::CancelRejected,
        _ => return Ok(None),
    };
    Ok(Some(OrderUpdate {
        order_id,
        kind,
        occurred_at,
    }))
}

fn fill(wire: &TradeUpdateWire, executed_at: DateTime<Utc>) -> Result<Fill, MappingError> {
    let missing = |field: &str| MappingError(format!("ejecución sin el campo '{field}'"));

    let execution_id = wire
        .execution_id
        .as_deref()
        .ok_or_else(|| missing("execution_id"))?;
    let price = wire.price.as_ref().ok_or_else(|| missing("price"))?;
    let quantity = wire.qty.as_ref().ok_or_else(|| missing("qty"))?;

    Ok(Fill::new(
        FillId::new(execution_id)?,
        Quantity::new(decimal_value("qty", quantity)?)?,
        Price::new(decimal_value("price", price)?)?,
        executed_at,
    )?)
}

/// Acepta el valor como texto o como número JSON.
fn decimal_value(field: &str, value: &Value) -> Result<Decimal, MappingError> {
    match value {
        Value::String(raw) => decimal(field, raw),
        Value::Number(raw) => number(field, raw),
        other => Err(MappingError(format!(
            "el campo '{field}' no es numérico: {other}"
        ))),
    }
}

// --- Stream de datos de mercado ---

/// Elemento de un mensaje del WebSocket de datos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DataFrameItem {
    Success(String),
    Error {
        code: i64,
        message: String,
    },
    Subscribed,
    Event(MarketEvent),
    /// Tipo que no usamos o dato que el dominio no acepta.
    Ignored,
}

/// Interpreta un mensaje completo (un arreglo JSON) del WebSocket de datos.
pub(crate) fn data_frame(payload: &[u8]) -> Vec<DataFrameItem> {
    let Ok(items) = serde_json::from_slice::<Vec<Value>>(payload) else {
        return vec![DataFrameItem::Ignored];
    };
    items.into_iter().map(data_item).collect()
}

fn data_item(item: Value) -> DataFrameItem {
    let kind = item.get("T").and_then(Value::as_str).unwrap_or_default();
    let text = |field: &str| {
        item.get(field)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };

    match kind {
        "success" => DataFrameItem::Success(text("msg")),
        "error" => DataFrameItem::Error {
            code: item.get("code").and_then(Value::as_i64).unwrap_or_default(),
            message: text("msg"),
        },
        "subscription" => DataFrameItem::Subscribed,
        "q" => event(serde_json::from_value(item).ok().map(stream_quote)),
        "t" => event(serde_json::from_value(item).ok().map(stream_trade)),
        "b" => event(serde_json::from_value(item).ok().map(stream_bar)),
        _ => DataFrameItem::Ignored,
    }
}

fn event(mapped: Option<Result<MarketEvent, MappingError>>) -> DataFrameItem {
    match mapped {
        Some(Ok(event)) => DataFrameItem::Event(event),
        _ => DataFrameItem::Ignored,
    }
}

fn stream_quote(wire: StreamQuoteWire) -> Result<MarketEvent, MappingError> {
    Ok(MarketEvent::Quote(Quote::new(
        Symbol::new(&wire.symbol)?,
        optional_price("bp", &wire.bp)?,
        Quantity::new(number("bs", &wire.bs)?)?,
        optional_price("ap", &wire.ap)?,
        Quantity::new(number("as", &wire.ask_size)?)?,
        timestamp("t", &wire.t)?,
    )))
}

fn stream_trade(wire: StreamTradeWire) -> Result<MarketEvent, MappingError> {
    Ok(MarketEvent::Trade(TradeTick::new(
        Symbol::new(&wire.symbol)?,
        Price::new(number("p", &wire.p)?)?,
        Quantity::new(number("s", &wire.s)?)?,
        timestamp("t", &wire.t)?,
    )))
}

fn stream_bar(wire: StreamBarWire) -> Result<MarketEvent, MappingError> {
    let price = |field: &str, value| -> Result<Price, MappingError> {
        Ok(Price::new(number(field, value)?)?)
    };
    Ok(MarketEvent::Bar(Bar::new(
        Symbol::new(&wire.symbol)?,
        Timeframe::Minute1,
        timestamp("t", &wire.t)?,
        Ohlc {
            open: price("o", &wire.o)?,
            high: price("h", &wire.h)?,
            low: price("l", &wire.l)?,
            close: price("c", &wire.c)?,
        },
        Quantity::new(number("v", &wire.v)?)?,
    )?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use serde_json::json;

    fn update(event: &str, extra: Value) -> TradeUpdateWire {
        let mut data = json!({
            "event": event,
            "timestamp": "2024-03-01T15:30:02.5Z",
            "order": { "id": "ord-1", "updated_at": "2024-03-01T15:30:02Z" }
        });
        if let (Some(target), Value::Object(fields)) = (data.as_object_mut(), extra) {
            target.extend(fields);
        }
        serde_json::from_value(data).unwrap()
    }

    #[test]
    fn fill_events_carry_the_execution() {
        let wire = update(
            "partial_fill",
            json!({ "execution_id": "exec-9", "price": "179.08", "qty": "3", "position_qty": "3" }),
        );

        let mapped = trade_update(wire).unwrap().unwrap();
        let OrderUpdateKind::Filled(fill) = mapped.kind else {
            panic!("se esperaba una ejecución");
        };
        assert_eq!(fill.id().as_str(), "exec-9");
        assert_eq!(fill.price().value(), dec!(179.08));
        assert_eq!(fill.quantity(), Quantity::new(dec!(3)).unwrap());
    }

    #[test]
    fn lifecycle_events_are_mapped_and_noise_is_ignored() {
        let kind = |event: &str| {
            trade_update(update(event, json!({})))
                .unwrap()
                .map(|u| u.kind)
        };

        assert_eq!(kind("new"), Some(OrderUpdateKind::Accepted));
        assert_eq!(kind("canceled"), Some(OrderUpdateKind::Canceled));
        assert_eq!(kind("expired"), Some(OrderUpdateKind::Expired));
        assert_eq!(
            kind("order_cancel_rejected"),
            Some(OrderUpdateKind::CancelRejected)
        );
        assert_eq!(kind("pending_new"), None);
        assert_eq!(kind("done_for_day"), None);
    }

    #[test]
    fn fill_without_price_is_an_error() {
        let wire = update("fill", json!({ "execution_id": "exec-1", "qty": "1" }));
        assert!(trade_update(wire).is_err());
    }

    #[test]
    fn data_frames_mix_control_and_market_messages() {
        let payload = json!([
            { "T": "success", "msg": "authenticated" },
            { "T": "q", "S": "AAPL", "bp": 179.1, "bs": 1, "ap": 179.12, "as": 2, "t": "2024-03-01T15:30:00Z" },
            { "T": "t", "S": "AAPL", "p": 179.11, "s": 50, "t": "2024-03-01T15:30:00.5Z", "i": 1, "x": "V" },
            { "T": "d", "S": "AAPL" },
            { "T": "error", "code": 406, "msg": "connection limit exceeded" }
        ])
        .to_string();

        let items = data_frame(payload.as_bytes());

        assert_eq!(items[0], DataFrameItem::Success("authenticated".to_owned()));
        assert!(matches!(
            &items[1],
            DataFrameItem::Event(MarketEvent::Quote(_))
        ));
        assert!(matches!(
            &items[2],
            DataFrameItem::Event(MarketEvent::Trade(_))
        ));
        assert_eq!(items[3], DataFrameItem::Ignored);
        assert_eq!(
            items[4],
            DataFrameItem::Error {
                code: 406,
                message: "connection limit exceeded".to_owned()
            }
        );
    }

    #[test]
    fn invalid_market_data_is_ignored_not_fatal() {
        // Precio negativo: el dominio lo rechaza y el elemento se descarta
        let payload =
            json!([{ "T": "t", "S": "AAPL", "p": -1, "s": 1, "t": "2024-03-01T15:30:00Z" }])
                .to_string();
        assert_eq!(data_frame(payload.as_bytes()), vec![DataFrameItem::Ignored]);
    }
}
