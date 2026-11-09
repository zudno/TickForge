//! Prueba de humo contra la cuenta real de **paper trading** (solo lectura, no envía órdenes).
//!
//! Está ignorada por defecto porque necesita red y llaves. Para ejecutarla, con las llaves de
//! paper en el archivo `.env` de la raíz del repositorio:
//!
//! ```text
//! cargo test -p tickforge-adapter-alpaca --test paper_smoke -- --ignored --nocapture
//! ```

// Crate de pruebas: unwrap/expect también en funciones auxiliares sin #[test]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use tickforge_adapter_alpaca::{
    AlpacaBrokerGateway, AlpacaConfig, AlpacaCredentials, AlpacaEnvironment, AlpacaMarketData,
};
use tickforge_application::ports::{BrokerGateway, Clock, MarketDataFeed};
use tickforge_domain::market::Timeframe;
use tickforge_domain::shared::Symbol;

struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        DateTime::from(SystemTime::now())
    }
}

fn paper_config() -> AlpacaConfig {
    // Busca `.env` en el directorio actual y sus padres
    dotenvy::dotenv().ok();
    let key_id = std::env::var("ALPACA_API_KEY_ID").expect("falta ALPACA_API_KEY_ID en .env");
    let secret_key =
        std::env::var("ALPACA_API_SECRET_KEY").expect("falta ALPACA_API_SECRET_KEY en .env");

    let config = AlpacaConfig::paper(AlpacaCredentials::new(key_id, secret_key));
    // Salvaguarda: esta prueba jamás debe apuntar a la cuenta real
    assert_eq!(config.environment, AlpacaEnvironment::Paper);
    config
}

#[tokio::test]
#[ignore = "requiere red y llaves de paper trading en .env"]
async fn paper_account_and_market_data_are_reachable() {
    let config = paper_config();
    let gateway = AlpacaBrokerGateway::new(&config).unwrap();
    let market_data = AlpacaMarketData::new(&config, Arc::new(SystemClock)).unwrap();
    let aapl = Symbol::new("AAPL").unwrap();

    let account = gateway.account().await.unwrap();
    println!(
        "cuenta: equity {} | efectivo {} | poder de compra {} | bloqueada: {}",
        account.equity(),
        account.cash(),
        account.buying_power(),
        account.is_trading_blocked()
    );

    let clock = gateway.market_clock().await.unwrap();
    println!(
        "mercado abierto: {} | próximo cambio en {} min",
        clock.is_open(),
        clock.time_until_next_change().num_minutes()
    );

    let positions = gateway.positions().await.unwrap();
    let open_orders = gateway.open_orders().await.unwrap();
    println!(
        "posiciones: {} | órdenes abiertas: {}",
        positions.len(),
        open_orders.len()
    );

    let quote = market_data.latest_quote(&aapl).await.unwrap();
    println!(
        "AAPL bid {:?} / ask {:?} @ {}",
        quote.bid().map(|p| p.to_string()),
        quote.ask().map(|p| p.to_string()),
        quote.quoted_at()
    );

    let bars = market_data.bars(&aapl, Timeframe::Day1, 5).await.unwrap();
    println!("velas diarias de AAPL recibidas: {}", bars.len());
    assert!(!bars.is_empty());
}
