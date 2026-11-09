use serde::Deserialize;
use serde_json::{Number, Value};

// --- WebSocket de trading (`trade_updates`) ---

/// Envoltura de todo mensaje del stream de trading: `{"stream": "...", "data": {...}}`.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TradingStreamMessageWire {
    pub stream: String,
    #[serde(default)]
    pub data: Value,
}

/// `data` de un mensaje `trade_updates`.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TradeUpdateWire {
    pub event: String,
    pub execution_id: Option<String>,
    pub timestamp: Option<String>,
    /// Precio de la ejecución (en `fill` y `partial_fill`); texto o número según la versión.
    pub price: Option<Value>,
    /// Cantidad de esta ejecución (no la acumulada).
    pub qty: Option<Value>,
    pub order: TradeUpdateOrderWire,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TradeUpdateOrderWire {
    pub id: String,
    pub updated_at: Option<String>,
}

// --- WebSocket de datos de mercado ---
// Cada mensaje es un arreglo de objetos con un campo `T` que indica el tipo.

/// `{"T": "q", ...}`
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct StreamQuoteWire {
    #[serde(rename = "S")]
    pub symbol: String,
    pub t: String,
    pub bp: Number,
    pub bs: Number,
    pub ap: Number,
    #[serde(rename = "as")]
    pub ask_size: Number,
}

/// `{"T": "t", ...}`
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct StreamTradeWire {
    #[serde(rename = "S")]
    pub symbol: String,
    pub t: String,
    pub p: Number,
    pub s: Number,
}

/// `{"T": "b", ...}`: vela de un minuto.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct StreamBarWire {
    #[serde(rename = "S")]
    pub symbol: String,
    pub t: String,
    pub o: Number,
    pub h: Number,
    pub l: Number,
    pub c: Number,
    pub v: Number,
}
