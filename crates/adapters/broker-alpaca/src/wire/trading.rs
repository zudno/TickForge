use serde::{Deserialize, Serialize};

/// `GET /v2/account`
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct AccountWire {
    pub currency: String,
    pub cash: String,
    pub equity: String,
    pub last_equity: String,
    pub buying_power: String,
    #[serde(default)]
    pub trading_blocked: bool,
    #[serde(default)]
    pub account_blocked: bool,
}

/// Elemento de `GET /v2/positions` y respuesta de `GET /v2/positions/{symbol}`.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct PositionWire {
    pub symbol: String,
    pub qty: String,
    pub avg_entry_price: String,
    pub side: String,
}

/// Orden tal como la devuelve la Trading API.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct OrderWire {
    pub id: String,
    pub client_order_id: String,
    pub created_at: String,
    pub updated_at: Option<String>,
    pub submitted_at: Option<String>,
    pub symbol: String,
    /// `null` en órdenes por monto (`notional`), que el MVP no soporta.
    pub qty: Option<String>,
    pub filled_qty: String,
    pub filled_avg_price: Option<String>,
    #[serde(rename = "type")]
    pub order_type: String,
    pub side: String,
    pub time_in_force: String,
    pub limit_price: Option<String>,
    pub status: String,
}

/// Cuerpo de `POST /v2/orders`.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct NewOrderBodyWire {
    pub symbol: String,
    pub qty: String,
    pub side: &'static str,
    #[serde(rename = "type")]
    pub order_type: &'static str,
    pub time_in_force: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_price: Option<String>,
    pub client_order_id: String,
}

/// `GET /v2/clock`
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ClockWire {
    pub timestamp: String,
    pub is_open: bool,
    pub next_open: String,
    pub next_close: String,
}

/// Cuerpo de error común: `{"code": 40310000, "message": "insufficient buying power"}`.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ErrorBodyWire {
    pub message: Option<String>,
}
