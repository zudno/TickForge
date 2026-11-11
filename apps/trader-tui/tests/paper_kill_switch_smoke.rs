//! Prueba de humo del kill switch contra **paper trading** y SQLite en disco.
//!
//! Escribe en la cuenta paper con las mismas garantías que las demás pruebas de escritura:
//! coloca una compra límite de 1 acción muy por debajo del mercado y la cancela (aquí, el
//! propio kill switch la cancela). La vista previa del final no envía nada.
//!
//! **Importante:** el kill switch cancela *todas* las órdenes abiertas de la cuenta. Por eso
//! la prueba se niega a correr si la cuenta tiene órdenes abiertas que no son suyas.
//!
//! ```text
//! cargo test -p tickforge --test paper_kill_switch_smoke -- --ignored --nocapture
//! ```

// Crate de pruebas: unwrap/expect también en funciones auxiliares sin #[test]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, TimeDelta, Utc};
use rust_decimal::{Decimal, RoundingStrategy};
use tickforge_adapter_alpaca::{
    AlpacaBrokerGateway, AlpacaConfig, AlpacaCredentials, AlpacaEnvironment, AlpacaMarketData,
};
use tickforge_adapter_sqlite::{SqliteKillSwitchRepository, SqliteOrderJournal, SqliteStore};
use tickforge_application::ports::{
    ApplicationEvent, BrokerGateway, ClientOrderIdGenerator, Clock, EventPublisher,
    KillSwitchRepository, MarketDataFeed,
};
use tickforge_application::use_cases::trading::{
    KillSwitchUseCase, PlaceOrderCommand, PlaceOrderUseCase, TradingPorts,
};
use tickforge_domain::DomainError;
use tickforge_domain::risk::{RiskLimits, RiskPolicy, RiskViolation};
use tickforge_domain::shared::{Money, Price, Quantity, Symbol};
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

struct StampedIds {
    stamp: u128,
    sequence: AtomicU32,
}

impl ClientOrderIdGenerator for StampedIds {
    fn next_id(&self) -> Result<ClientOrderId, DomainError> {
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed);
        ClientOrderId::new(&format!("tf-ks-{}-{sequence}", self.stamp))
    }
}

#[tokio::test]
#[ignore = "escribe en la cuenta paper: coloca una orden límite que el kill switch cancela"]
async fn kill_switch_cancels_open_orders_blocks_new_ones_and_persists() {
    dotenvy::dotenv().ok();
    let config = AlpacaConfig::paper(AlpacaCredentials::new(
        std::env::var("ALPACA_API_KEY_ID").expect("falta ALPACA_API_KEY_ID en .env"),
        std::env::var("ALPACA_API_SECRET_KEY").expect("falta ALPACA_API_SECRET_KEY en .env"),
    ));
    // Salvaguarda: jamás contra la cuenta real
    assert_eq!(config.environment, AlpacaEnvironment::Paper);
    assert!(config.trading_base_url.contains("paper-api"));

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let database = std::env::temp_dir()
        .join(format!("tickforge-kill-switch-{stamp}"))
        .join("tickforge.db");
    let store = SqliteStore::open(&database).await.unwrap();

    let broker = Arc::new(AlpacaBrokerGateway::new(&config).unwrap());
    let market_data = Arc::new(AlpacaMarketData::new(&config, Arc::new(SystemClock)).unwrap());
    let ports = TradingPorts {
        broker: broker.clone(),
        market_data: market_data.clone(),
        journal: Arc::new(SqliteOrderJournal::new(store.clone())),
        events: Arc::new(IgnoreEvents),
        clock: Arc::new(SystemClock),
        client_order_ids: Arc::new(StampedIds {
            stamp,
            sequence: AtomicU32::new(0),
        }),
        kill_switch: Arc::new(SqliteKillSwitchRepository::new(store.clone())),
    };
    let kill_switch = KillSwitchUseCase::new(ports.clone());
    let place_order = PlaceOrderUseCase::new(
        ports,
        RiskPolicy::new(
            RiskLimits::new(
                Money::usd(Decimal::from(5_000)),
                Money::usd(Decimal::from(20_000)),
                Decimal::from(5),
                TimeDelta::seconds(60),
            )
            .unwrap(),
        ),
    );

    // Salvaguarda: el kill switch cancela TODAS las órdenes abiertas de la cuenta. Si el
    // usuario tiene órdenes propias, la prueba se niega a correr para no tocarlas
    let existing = broker.open_orders().await.unwrap();
    assert!(
        existing.is_empty(),
        "la cuenta paper tiene {} órdenes abiertas y el kill switch las cancelaría; \
         esta prueba solo corre con la cuenta sin órdenes abiertas",
        existing.len()
    );

    // 1. Una orden abierta que nunca se ejecutará
    let aapl = Symbol::new("AAPL").unwrap();
    let quote = market_data.latest_quote(&aapl).await.unwrap();
    let reference = quote
        .bid()
        .or(quote.ask())
        .map_or(Decimal::ONE_HUNDRED, Price::value);
    let limit =
        (reference * Decimal::new(8, 1)).round_dp_with_strategy(2, RoundingStrategy::ToZero);
    let resting = broker
        .submit_order(
            &OrderRequest::new(
                ClientOrderId::new(&format!("tf-ks-rest-{stamp}")).unwrap(),
                aapl.clone(),
                OrderSide::Buy,
                OrderType::Limit {
                    limit_price: Price::new(limit).unwrap(),
                },
                Quantity::new(Decimal::ONE).unwrap(),
                TimeInForce::Day,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    println!("orden abierta {} (límite {limit})", resting.id());

    // 2. Activar: bloquea y cancela todo
    let report = kill_switch.engage("prueba de humo").await.unwrap();
    println!(
        "kill switch: {} canceladas, {} fallidas",
        report.canceled.len(),
        report.failed.len()
    );
    assert!(report.canceled.contains(resting.id()));
    assert!(report.failed.is_empty());

    // Alpaca confirma la cancelación (puede tardar un instante en pasar de pending_cancel)
    let mut status = OrderStatus::PendingCancel;
    for _ in 0..10 {
        status = broker.order(resting.id()).await.unwrap().unwrap().status();
        if status == OrderStatus::Canceled {
            break;
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    println!("estado de la orden en Alpaca: {status}");
    assert_eq!(status, OrderStatus::Canceled);

    // 3. Una orden nueva queda bloqueada por la política de riesgo (solo vista previa)
    let preview = place_order
        .preview(PlaceOrderCommand {
            symbol: aapl,
            side: OrderSide::Buy,
            order_type: OrderType::Market,
            quantity: Quantity::new(Decimal::ONE).unwrap(),
            time_in_force: TimeInForce::Day,
        })
        .await
        .unwrap();
    let blocked = preview
        .assessment
        .violations()
        .iter()
        .find(|violation| matches!(violation, RiskViolation::KillSwitchEngaged { .. }));
    println!(
        "vista previa de una orden nueva: {}",
        blocked.map_or("SIN BLOQUEO".to_owned(), ToString::to_string)
    );
    assert!(blocked.is_some());

    // 4. El bloqueo sobrevive a cerrar y reabrir la base
    store.close().await;
    let reopened = SqliteStore::open(&database).await.unwrap();
    let persisted = SqliteKillSwitchRepository::new(reopened.clone());
    assert!(persisted.load().await.unwrap().is_engaged());
    println!("tras reabrir la base: el kill switch sigue activo");

    // 5. Desactivar
    kill_switch.release().await.unwrap();
    assert!(!persisted.load().await.unwrap().is_engaged());
    println!("kill switch desactivado");

    reopened.close().await;
    if let Some(directory) = database.parent() {
        let _ = std::fs::remove_dir_all(directory);
    }
}
