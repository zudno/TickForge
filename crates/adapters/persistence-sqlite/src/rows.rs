//! Conversión entre tipos del dominio y columnas de texto.
//!
//! Se usan los códigos estables del dominio (`as_str`), así la base no depende de cómo se
//! muestran los datos en pantalla.

use std::str::FromStr;

use chrono::{DateTime, SecondsFormat, Utc};
use rust_decimal::Decimal;
use tickforge_domain::shared::{Price, Quantity, Symbol};
use tickforge_domain::trading::{
    ClientOrderId, OrderRequest, OrderSide, OrderStatus, OrderType, TimeInForce,
};

/// Columnas de una solicitud de orden.
pub(crate) struct RequestColumns {
    pub client_order_id: String,
    pub symbol: String,
    pub side: &'static str,
    pub order_type: &'static str,
    pub limit_price: Option<String>,
    pub quantity: String,
    pub time_in_force: &'static str,
}

pub(crate) fn request_columns(request: &OrderRequest) -> RequestColumns {
    RequestColumns {
        client_order_id: request.client_order_id().as_str().to_owned(),
        symbol: request.symbol().as_str().to_owned(),
        side: request.side().as_str(),
        order_type: request.order_type().as_str(),
        limit_price: request
            .order_type()
            .limit_price()
            .map(|price| price.value().to_string()),
        quantity: request.quantity().value().to_string(),
        time_in_force: request.time_in_force().as_str(),
    }
}

/// Reconstruye una solicitud a partir de sus columnas, validando todo con el dominio.
pub(crate) fn request_from_columns(
    client_order_id: &str,
    symbol: &str,
    side: &str,
    order_type: &str,
    limit_price: Option<&str>,
    quantity: &str,
    time_in_force: &str,
) -> Result<OrderRequest, String> {
    let side = match side {
        "buy" => OrderSide::Buy,
        "sell" => OrderSide::Sell,
        other => return Err(format!("lado desconocido '{other}'")),
    };
    let order_type = match (order_type, limit_price) {
        ("market", _) => OrderType::Market,
        ("limit", Some(raw)) => OrderType::Limit {
            limit_price: Price::new(decimal("limit_price", raw)?).map_err(|e| e.to_string())?,
        },
        ("limit", None) => return Err("orden límite sin precio".to_owned()),
        (other, _) => return Err(format!("tipo de orden desconocido '{other}'")),
    };
    let time_in_force = match time_in_force {
        "day" => TimeInForce::Day,
        "gtc" => TimeInForce::Gtc,
        other => return Err(format!("vigencia desconocida '{other}'")),
    };

    OrderRequest::new(
        ClientOrderId::new(client_order_id).map_err(|e| e.to_string())?,
        Symbol::new(symbol).map_err(|e| e.to_string())?,
        side,
        order_type,
        Quantity::new(decimal("quantity", quantity)?).map_err(|e| e.to_string())?,
        time_in_force,
    )
    .map_err(|e| e.to_string())
}

pub(crate) fn status_code(status: OrderStatus) -> &'static str {
    status.as_str()
}

pub(crate) fn timestamp(instant: DateTime<Utc>) -> String {
    instant.to_rfc3339_opts(SecondsFormat::Micros, true)
}

fn decimal(field: &str, raw: &str) -> Result<Decimal, String> {
    Decimal::from_str(raw).map_err(|_| format!("'{field}' no es un decimal válido: '{raw}'"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn requests_round_trip_through_columns() {
        let request = OrderRequest::new(
            ClientOrderId::new("tf-1").unwrap(),
            Symbol::new("BRK.B").unwrap(),
            OrderSide::Sell,
            OrderType::Limit {
                limit_price: Price::new(dec!(412.35)).unwrap(),
            },
            Quantity::new(dec!(0.5)).unwrap(),
            TimeInForce::Gtc,
        )
        .unwrap();

        let columns = request_columns(&request);
        let restored = request_from_columns(
            &columns.client_order_id,
            &columns.symbol,
            columns.side,
            columns.order_type,
            columns.limit_price.as_deref(),
            &columns.quantity,
            columns.time_in_force,
        )
        .unwrap();

        assert_eq!(restored, request);
    }

    #[test]
    fn corrupted_columns_are_reported() {
        let error =
            request_from_columns("tf-1", "AAPL", "hold", "market", None, "1", "day").unwrap_err();
        assert!(error.contains("lado desconocido"));
    }
}
