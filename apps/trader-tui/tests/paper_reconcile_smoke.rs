//! Prueba de humo de la reconciliación contra **paper trading** y SQLite en disco.
//!
//! Simula la caída más delicada: la aplicación registra la intención, el broker recibe la
//! orden y la aplicación se cierra antes de registrar el resultado. Al "reiniciar", la
//! reconciliación debe encontrar la orden en Alpaca por su `client_order_id`.
//!
//! Escribe en la cuenta paper con las mismas garantías que `paper_orders_smoke`: compra
//! límite de 1 acción muy por debajo del mercado, cancelada al final.
//!
//! ```text
//! cargo test -p tickforge --test paper_reconcile_smoke -- --ignored --nocapture
//! ```

// Crate de pruebas: unwrap/expect también en funciones auxiliares sin #[test]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use rust_decimal::{Decimal, RoundingStrategy};
use tickforge_adapter_alpaca::{
    AlpacaBrokerGateway, AlpacaConfig, AlpacaCredentials, AlpacaEnvironment, AlpacaMarketData,
};
use tickforge_adapter_sqlite::{SqliteKillSwitchRepository, SqliteOrderJournal, SqliteStore};
use tickforge_application::ports::{
    ApplicationEvent, BrokerGateway, ClientOrderIdGenerator, Clock, EventPublisher, JournalEntry,
    MarketDataFeed, OrderJournal,
};
use tickforge_application::use_cases::trading::{ReconcileOrdersUseCase, TradingPorts};
use tickforge_domain::DomainError;
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

struct IgnoreEvents;

impl EventPublisher for IgnoreEvents {
    fn publish(&self, _event: ApplicationEvent) {}
}

/// La reconciliación no genera ids; este generador nunca se usa.
struct NoIds;

impl ClientOrderIdGenerator for NoIds {
    fn next_id(&self) -> Result<ClientOrderId, DomainError> {
        ClientOrderId::new("unused")
    }
}

fn limit_buy(client_order_id: &str, limit: Decimal) -> OrderRequest {
    OrderRequest::new(
        ClientOrderId::new(client_order_id).unwrap(),
        Symbol::new("AAPL").unwrap(),
        OrderSide::Buy,
        OrderType::Limit {
            limit_price: Price::new(limit).unwrap(),
        },
        Quantity::new(Decimal::ONE).unwrap(),
        TimeInForce::Day,
    )
    .unwrap()
}

#[tokio::test]
#[ignore = "escribe en la cuenta paper: coloca y cancela una orden límite"]
async fn an_order_sent_before_a_crash_is_recovered_on_restart() {
    dotenvy::dotenv().ok();
    let config = AlpacaConfig::paper(AlpacaCredentials::new(
        std::env::var("ALPACA_API_KEY_ID").expect("falta ALPACA_API_KEY_ID en .env"),
        std::env::var("ALPACA_API_SECRET_KEY").expect("falta ALPACA_API_SECRET_KEY en .env"),
    ));
    // Salvaguarda: jamás contra la cuenta real
    assert_eq!(config.environment, AlpacaEnvironment::Paper);
    assert!(config.trading_base_url.contains("paper-api"));

    let broker = Arc::new(AlpacaBrokerGateway::new(&config).unwrap());
    let market_data = Arc::new(AlpacaMarketData::new(&config, Arc::new(SystemClock)).unwrap());

    // Base SQLite real en una carpeta temporal
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let database = std::env::temp_dir()
        .join(format!("tickforge-reconcile-{stamp}"))
        .join("tickforge.db");

    let quote = market_data
        .latest_quote(&Symbol::new("AAPL").unwrap())
        .await
        .unwrap();
    let reference = quote
        .bid()
        .or(quote.ask())
        .map_or(Decimal::ONE_HUNDRED, Price::value);
    let limit =
        (reference * Decimal::new(8, 1)).round_dp_with_strategy(2, RoundingStrategy::ToZero);

    let sent = limit_buy(&format!("tf-crash-{stamp}"), limit);
    let never_sent = limit_buy(&format!("tf-lost-{stamp}"), limit);

    // --- Sesión 1: intención registrada, orden enviada, "caída" antes del resultado ---
    {
        let store = SqliteStore::open(&database).await.unwrap();
        let journal = SqliteOrderJournal::new(store.clone());
        for request in [&sent, &never_sent] {
            journal
                .append(JournalEntry::Intent {
                    request: request.clone(),
                    at: SystemClock.now(),
                })
                .await
                .unwrap();
        }
        let order = broker.submit_order(&sent).await.unwrap();
        println!(
            "sesión 1: orden {} enviada (límite {limit}) y la aplicación \"se cae\"",
            order.id()
        );
        store.close().await;
    }

    // --- Sesión 2: al reiniciar, la reconciliación consulta a Alpaca ---
    let store = SqliteStore::open(&database).await.unwrap();
    let journal = Arc::new(SqliteOrderJournal::new(store.clone()));
    let pending = journal.pending_intents().await.unwrap();
    println!(
        "sesión 2: {} intenciones pendientes en SQLite",
        pending.len()
    );
    assert_eq!(pending.len(), 2);

    let reconcile = ReconcileOrdersUseCase::new(TradingPorts {
        broker: broker.clone(),
        market_data,
        journal: journal.clone(),
        events: Arc::new(IgnoreEvents),
        clock: Arc::new(SystemClock),
        client_order_ids: Arc::new(NoIds),
        kill_switch: Arc::new(SqliteKillSwitchRepository::new(store.clone())),
    });
    let report = reconcile.execute().await.unwrap();
    println!(
        "reconciliación: {} revisadas, {} recuperadas, {} nunca llegaron, {} sin verificar",
        report.checked,
        report.recovered.len(),
        report.not_submitted.len(),
        report.unresolved.len()
    );

    assert_eq!(report.recovered.len(), 1);
    let recovered = &report.recovered[0];
    assert_eq!(recovered.client_order_id(), sent.client_order_id());
    assert_eq!(
        report.not_submitted,
        vec![never_sent.client_order_id().clone()]
    );
    assert!(journal.pending_intents().await.unwrap().is_empty());
    println!(
        "orden recuperada: {} ({})",
        recovered.id(),
        recovered.status()
    );

    // Limpieza: cancelar la orden de prueba
    broker.cancel_order(recovered.id()).await.unwrap();
    let final_state = broker.order(recovered.id()).await.unwrap().unwrap();
    println!("orden de prueba cancelada: {}", final_state.status());
    assert!(matches!(
        final_state.status(),
        OrderStatus::Canceled | OrderStatus::PendingCancel
    ));

    store.close().await;
    if let Some(directory) = database.parent() {
        let _ = std::fs::remove_dir_all(directory);
    }
}
