//! Utilidades compartidas: servidor HTTP falso y JSON con la forma de las respuestas de Alpaca.

// Código exclusivo de pruebas: clippy no reconoce estas funciones auxiliares como pruebas.
#![allow(dead_code, clippy::unwrap_used)]

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use tickforge_adapter_alpaca::{
    AlpacaBrokerGateway, AlpacaConfig, AlpacaCredentials, AlpacaMarketData,
};
use tickforge_application::ports::Clock;
use wiremock::MockServer;

pub const KEY_ID: &str = "test-key-id";
pub const SECRET_KEY: &str = "test-secret-key";

pub fn config(server: &MockServer) -> AlpacaConfig {
    AlpacaConfig::paper(AlpacaCredentials::new(
        KEY_ID.to_owned(),
        SECRET_KEY.to_owned(),
    ))
    .with_base_urls(&server.uri(), &server.uri())
}

pub fn gateway(server: &MockServer) -> AlpacaBrokerGateway {
    AlpacaBrokerGateway::new(&config(server)).unwrap()
}

pub struct FixedClock(pub DateTime<Utc>);

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
}

pub fn market_data(server: &MockServer) -> AlpacaMarketData {
    let clock = Arc::new(FixedClock(
        DateTime::parse_from_rfc3339("2024-03-01T20:00:00Z")
            .unwrap()
            .with_timezone(&Utc),
    ));
    AlpacaMarketData::new(&config(server), clock).unwrap()
}

/// Orden con la forma de la Trading API v2 (solo los campos relevantes y algunos extra
/// que el adaptador debe ignorar).
pub fn order_json(status: &str, filled_qty: &str, filled_avg_price: Option<&str>) -> Value {
    json!({
        "id": "61e69015-8549-4bfd-b9c3-01e75843f47d",
        "client_order_id": "tf-000001",
        "created_at": "2024-03-01T15:30:00.123456Z",
        "updated_at": "2024-03-01T15:30:01.223456Z",
        "submitted_at": "2024-03-01T15:30:00.123456Z",
        "filled_at": null,
        "asset_id": "b0b6dd9d-8b9b-48a9-ba46-b9d54906e415",
        "asset_class": "us_equity",
        "symbol": "AAPL",
        "notional": null,
        "qty": "10",
        "filled_qty": filled_qty,
        "filled_avg_price": filled_avg_price,
        "order_class": "",
        "order_type": "limit",
        "type": "limit",
        "side": "buy",
        "time_in_force": "day",
        "limit_price": "180.5",
        "stop_price": null,
        "status": status,
        "extended_hours": false
    })
}

pub fn error_json(code: u64, message: &str) -> Value {
    json!({ "code": code, "message": message })
}
