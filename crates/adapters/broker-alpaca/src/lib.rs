//! Adaptador de Alpaca (<https://alpaca.markets>).
//!
//! - `AlpacaBrokerGateway` implementa `BrokerGateway` sobre la Trading API v2.
//! - `AlpacaMarketData` implementa `MarketDataFeed` sobre la Market Data API v2.
//!
//! Los modelos de la API (`wire`) nunca salen de este crate: `mapping` los traduce al dominio.
//! Por defecto solo se construye contra el entorno de **paper trading**.

mod config;
mod error;
mod gateway;
mod http;
mod mapping;
mod market_data;
mod rate_limiter;
mod websocket;
mod wire;

pub use config::{AlpacaConfig, AlpacaCredentials, AlpacaEnvironment, DataFeed};
pub use error::AlpacaSetupError;
pub use gateway::AlpacaBrokerGateway;
pub use market_data::AlpacaMarketData;
