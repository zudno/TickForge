//! Pruebas de la reconciliación al iniciar y de la watchlist persistente.

// Crate de pruebas: unwrap/expect también en funciones auxiliares sin #[test]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use chrono::DateTime;
use common::{Harness, aapl, command, limit};
use rust_decimal_macros::dec;
use tickforge_application::ApplicationError;
use tickforge_application::ports::{BrokerGateway, JournalEntry, ReconcileOutcome};
use tickforge_domain::shared::{Price, Quantity, Symbol};
use tickforge_domain::trading::{ClientOrderId, OrderRequest, OrderSide, OrderType, TimeInForce};

fn crashed_request(client_order_id: &str) -> OrderRequest {
    OrderRequest::new(
        ClientOrderId::new(client_order_id).unwrap(),
        aapl(),
        OrderSide::Buy,
        OrderType::Limit {
            limit_price: Price::new(dec!(99)).unwrap(),
        },
        Quantity::new(dec!(1)).unwrap(),
        TimeInForce::Day,
    )
    .unwrap()
}

fn symbol(raw: &str) -> Symbol {
    Symbol::new(raw).unwrap()
}

// --- Reconciliación ---

#[tokio::test]
async fn an_order_that_reached_the_broker_before_a_crash_is_recovered() {
    let harness = Harness::new();
    harness.quote(dec!(99.99), dec!(100));

    // La sesión anterior registró la intención y el broker recibió la orden, pero la aplicación
    // se cerró antes de registrar el resultado
    let request = crashed_request("tf-crash-1");
    harness.journal.seed(JournalEntry::Intent {
        request: request.clone(),
        at: DateTime::UNIX_EPOCH,
    });
    harness.broker.submit_order(&request).await.unwrap();

    let report = harness.reconcile_orders.execute().await.unwrap();

    assert_eq!(report.checked, 1);
    assert_eq!(report.recovered.len(), 1);
    assert!(report.not_submitted.is_empty());
    assert!(matches!(
        harness.journal.entries().last(),
        Some(JournalEntry::Reconciled {
            outcome: ReconcileOutcome::FoundAtBroker { .. },
            ..
        })
    ));

    // Ya resuelta: el siguiente arranque no la vuelve a revisar
    let second = harness.reconcile_orders.execute().await.unwrap();
    assert!(second.is_clean());
}

#[tokio::test]
async fn an_intent_that_never_reached_the_broker_is_marked_as_not_submitted() {
    let harness = Harness::new();
    harness.journal.seed(JournalEntry::Intent {
        request: crashed_request("tf-crash-2"),
        at: DateTime::UNIX_EPOCH,
    });

    let report = harness.reconcile_orders.execute().await.unwrap();

    assert_eq!(
        report.not_submitted,
        vec![ClientOrderId::new("tf-crash-2").unwrap()]
    );
    assert!(matches!(
        harness.journal.entries().last(),
        Some(JournalEntry::Reconciled {
            outcome: ReconcileOutcome::NotFoundAtBroker,
            ..
        })
    ));
}

#[tokio::test]
async fn orders_placed_normally_need_no_reconciliation() {
    let harness = Harness::new();
    harness.quote(dec!(99.99), dec!(100));
    harness
        .place_order
        .execute(command(OrderSide::Buy, limit(dec!(99)), dec!(1)))
        .await
        .unwrap();

    assert!(harness.reconcile_orders.execute().await.unwrap().is_clean());
}

// --- Watchlist ---

#[tokio::test]
async fn first_run_saves_the_seed_and_later_runs_load_the_saved_list() {
    let harness = Harness::new();
    let seed = vec![symbol("AAPL"), symbol("SPY")];

    assert_eq!(
        harness.watchlist.load_or_seed(seed.clone()).await.unwrap(),
        seed
    );
    assert_eq!(harness.watchlist_repository.saved(), Some(seed.clone()));

    // Con una lista guardada, la semilla se ignora
    let loaded = harness
        .watchlist
        .load_or_seed(vec![symbol("QQQ")])
        .await
        .unwrap();
    assert_eq!(loaded, seed);
}

#[tokio::test]
async fn adding_a_symbol_checks_it_has_market_data() {
    let harness = Harness::new();
    harness.quote(dec!(99.99), dec!(100));
    let current = vec![symbol("SPY")];

    let updated = harness.watchlist.add(&current, aapl()).await.unwrap();
    assert_eq!(updated, vec![symbol("SPY"), aapl()]);
    assert_eq!(harness.watchlist_repository.saved(), Some(updated.clone()));

    // Sin cotización: casi siempre es un error de escritura
    let unknown = harness.watchlist.add(&updated, symbol("ZZZZ")).await;
    assert!(matches!(
        unknown,
        Err(ApplicationError::InvalidWatchlist(message)) if message.contains("no hay datos")
    ));

    let duplicate = harness.watchlist.add(&updated, aapl()).await;
    assert!(matches!(
        duplicate,
        Err(ApplicationError::InvalidWatchlist(_))
    ));
}

#[tokio::test]
async fn removing_saves_the_list_but_never_leaves_it_empty() {
    let harness = Harness::new();
    let current = vec![symbol("SPY"), aapl()];

    let updated = harness.watchlist.remove(&current, &aapl()).await.unwrap();
    assert_eq!(updated, vec![symbol("SPY")]);
    assert_eq!(harness.watchlist_repository.saved(), Some(updated.clone()));

    let last = harness.watchlist.remove(&updated, &symbol("SPY")).await;
    assert!(matches!(last, Err(ApplicationError::InvalidWatchlist(_))));
}
