//! Adaptador de Alpaca (<https://alpaca.markets>).
//!
//! - `AlpacaBrokerGateway` implementa `BrokerGateway` sobre la Trading API v2.
//! - `AlpacaMarketData` implementa `MarketDataFeed` sobre la Market Data API v2.
//!
//! Los modelos de la API (`wire`) nunca salen de este crate: `mapping` los traduce al dominio.
//! Por defecto solo se construye contra el entorno de **paper trading**; la cuenta real exige
//! una `LiveTradingAuthorization` y los clientes validan la configuración al construirse.

mod config;
mod error;
mod gateway;
mod http;
mod live_authorization;
mod mapping;
mod market_data;
mod rate_limiter;
mod websocket;
mod wire;

pub use config::{AlpacaConfig, AlpacaCredentials, AlpacaEnvironment, DataFeed};
pub use error::AlpacaSetupError;
pub use gateway::AlpacaBrokerGateway;
pub use live_authorization::LiveTradingAuthorization;
pub use market_data::AlpacaMarketData;
