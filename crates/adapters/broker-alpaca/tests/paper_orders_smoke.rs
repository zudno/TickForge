//! Prueba de humo del ciclo de una orden contra **paper trading**.
//!
//! A diferencia de `paper_smoke`, esta prueba **sí escribe** en la cuenta paper: coloca una
//! compra límite de 1 acción muy por debajo del mercado (nunca se ejecuta) y la cancela de
//! inmediato. Nunca corre en CI y se niega a correr fuera del entorno paper:
//!
//! ```text
//! cargo test -p tickforge-adapter-alpaca --test paper_orders_smoke -- --ignored --nocapture
//! ```

// Crate de pruebas: unwrap/expect también en funciones auxiliares sin #[test]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Utc};
use futures::StreamExt;
use rust_decimal::{Decimal, RoundingStrategy};
use rust_decimal_macros::dec;
use tickforge_adapter_alpaca::{
    AlpacaBrokerGateway, AlpacaConfig, AlpacaCredentials, AlpacaEnvironment, AlpacaMarketData,
};
use tickforge_application::ports::{BrokerGateway, Clock, MarketDataFeed, OrderUpdateKind};
use tickforge_domain::shared::{Price, Quantity, Symbol};
use tickforge_domain::trading::{
    ClientOrderId, OrderRequest, OrderSide, OrderStatus, OrderType, TimeInForce,
};

struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        DateTime::from(SystemTime::now())
    }
}

#[tokio::test]
#[ignore = "escribe en la cuenta paper: coloca y cancela una orden límite"]
async fn paper_limit_order_is_placed_streamed_and_canceled() {
    dotenvy::dotenv().ok();
    let config = AlpacaConfig::paper(AlpacaCredentials::new(
        std::env::var("ALPACA_API_KEY_ID").expect("falta ALPACA_API_KEY_ID en .env"),
        std::env::var("ALPACA_API_SECRET_KEY").expect("falta ALPACA_API_SECRET_KEY en .env"),
    ));
    // Salvaguarda: jamás contra la cuenta real
    assert_eq!(config.environment, AlpacaEnvironment::Paper);
    assert!(config.trading_base_url.contains("paper-api"));

    let gateway = AlpacaBrokerGateway::new(&config).unwrap();
    let market_data = AlpacaMarketData::new(&config, Arc::new(SystemClock)).unwrap();
    let aapl = Symbol::new("AAPL").unwrap();

    let clock = gateway.market_clock().await.unwrap();
    println!("mercado abierto: {}", clock.is_open());

    // Límite 20 % por debajo del bid (o del último cierre): no se ejecutará
    let quote = market_data.latest_quote(&aapl).await.unwrap();
    let reference = quote
        .bid()
        .or(quote.ask())
        .map(Price::value)
        .unwrap_or(dec!(100));
    let limit = (reference * dec!(0.8)).round_dp_with_strategy(2, RoundingStrategy::ToZero);
    println!("precio de referencia {reference} → límite {limit}");

    let mut updates = gateway.order_updates().await.unwrap();

    let client_order_id = format!("tf-smoke-{}", SystemClock.now().timestamp_millis());
    let request = OrderRequest::new(
        ClientOrderId::new(&client_order_id).unwrap(),
        aapl.clone(),
        OrderSide::Buy,
        OrderType::Limit {
            limit_price: Price::new(limit).unwrap(),
        },
        Quantity::new(Decimal::ONE).unwrap(),
        TimeInForce::Day,
    )
    .unwrap();

    let order = gateway.submit_order(&request).await.unwrap();
    println!(
        "orden enviada: id {} | estado {} | cliente {}",
        order.id(),
        order.status(),
        order.client_order_id()
    );
    assert_eq!(order.client_order_id().as_str(), client_order_id);

    let open = gateway.open_orders().await.unwrap();
    assert!(open.iter().any(|o| o.id() == order.id()));
    println!(
        "aparece entre las órdenes abiertas ({} en total)",
        open.len()
    );

    // Reintentar con el mismo ClientOrderId devuelve la misma orden (idempotencia real)
    let retried = gateway.submit_order(&request).await.unwrap();
    assert_eq!(retried.id(), order.id());
    println!("reintento idempotente: misma orden {}", retried.id());

    gateway.cancel_order(order.id()).await.unwrap();
    println!("cancelación solicitada");

    // Eventos del flujo hasta ver la cancelación (o 15 s)
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    let mut saw_cancel = false;
    while let Ok(Some(update)) = tokio::time::timeout_at(deadline, updates.next()).await {
        if &update.order_id != order.id() {
            continue;
        }
        println!("  evento: {:?}", update.kind);
        if update.kind == OrderUpdateKind::Canceled {
            saw_cancel = true;
            break;
        }
    }
    assert!(
        saw_cancel,
        "no llegó el evento de cancelación por WebSocket"
    );

    let final_state = gateway.order(order.id()).await.unwrap().unwrap();
    println!("estado final: {}", final_state.status());
    assert_eq!(final_state.status(), OrderStatus::Canceled);
}
