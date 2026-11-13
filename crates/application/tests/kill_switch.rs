//! Pruebas del kill switch contra el broker simulado.

// Crate de pruebas: unwrap/expect también en funciones auxiliares sin #[test]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{Harness, command, limit};
use rust_decimal_macros::dec;
use tickforge_application::ApplicationError;
use tickforge_application::ports::{BrokerGateway, JournalEntry};
use tickforge_domain::risk::RiskViolation;
use tickforge_domain::trading::{OrderSide, OrderStatus, OrderType};

#[tokio::test]
async fn engaging_cancels_every_open_order_and_blocks_new_ones() {
    let harness = Harness::new();
    harness.quote(dec!(99.99), dec!(100));
    let first = harness
        .place_order
        .execute(command(OrderSide::Buy, limit(dec!(99)), dec!(1)))
        .await
        .unwrap();
    let second = harness
        .place_order
        .execute(command(OrderSide::Buy, limit(dec!(98)), dec!(1)))
        .await
        .unwrap();

    let report = harness.kill_switch.engage("prueba").await.unwrap();

    assert_eq!(report.canceled.len(), 2);
    assert!(report.failed.is_empty());
    for order in [&first, &second] {
        let current = harness.broker.order(order.id()).await.unwrap().unwrap();
        assert_eq!(current.status(), OrderStatus::Canceled);
    }
    assert!(harness.broker.open_orders().await.unwrap().is_empty());
    assert!(
        harness
            .journal
            .entries()
            .iter()
            .any(|entry| matches!(entry, JournalEntry::KillSwitchEngaged { .. }))
    );

    // Órdenes nuevas: la vista previa lo explica y el envío se rechaza
    let preview = harness
        .place_order
        .preview(command(OrderSide::Buy, OrderType::Market, dec!(1)))
        .await
        .unwrap();
    assert!(
        preview
            .assessment
            .violations()
            .iter()
            .any(|v| matches!(v, RiskViolation::KillSwitchEngaged { .. }))
    );
    let result = harness
        .place_order
        .execute(command(OrderSide::Buy, OrderType::Market, dec!(1)))
        .await;
    assert!(matches!(result, Err(ApplicationError::RiskRejected(_))));
}

#[tokio::test]
async fn releasing_allows_orders_again() {
    let harness = Harness::new();
    harness.quote(dec!(99.99), dec!(100));
    harness.kill_switch.engage("prueba").await.unwrap();

    harness.kill_switch.release().await.unwrap();

    assert!(!harness.kill_switch.status().await.unwrap().is_engaged());
    let order = harness
        .place_order
        .execute(command(OrderSide::Buy, OrderType::Market, dec!(1)))
        .await;
    assert!(order.is_ok());
    assert!(matches!(
        harness.journal.entries().iter().rev().nth(2),
        Some(JournalEntry::KillSwitchReleased { .. })
    ));
}

#[tokio::test]
async fn if_the_block_cannot_be_saved_nothing_is_canceled() {
    let harness = Harness::new();
    harness.quote(dec!(99.99), dec!(100));
    let order = harness
        .place_order
        .execute(command(OrderSide::Buy, limit(dec!(99)), dec!(1)))
        .await
        .unwrap();
    harness.kill_switch_repository.set_fail_writes(true);

    let result = harness.kill_switch.engage("prueba").await;

    // El usuario debe saber que NO hay bloqueo; las órdenes siguen como estaban
    assert!(matches!(result, Err(ApplicationError::Repository(_))));
    let current = harness.broker.order(order.id()).await.unwrap().unwrap();
    assert_eq!(current.status(), OrderStatus::New);
}

#[tokio::test]
async fn engaging_with_nothing_open_just_blocks() {
    let harness = Harness::new();
    let report = harness.kill_switch.engage("prueba").await.unwrap();

    assert!(report.canceled.is_empty());
    assert!(harness.kill_switch_repository.current().is_engaged());
}

#[tokio::test]
async fn holding_blocks_new_orders_without_canceling_open_ones() {
    let harness = Harness::new();
    harness.quote(dec!(99.99), dec!(100));
    let resting = harness
        .place_order
        .execute(command(OrderSide::Buy, limit(dec!(99)), dec!(1)))
        .await
        .unwrap();

    let held = harness
        .kill_switch
        .hold("inicio de sesión en modo real")
        .await
        .unwrap();

    assert!(held.is_engaged());
    assert!(harness.kill_switch_repository.current().is_engaged());
    // La orden abierta sigue intacta
    let current = harness.broker.order(resting.id()).await.unwrap().unwrap();
    assert!(current.status().is_cancelable());
    assert_eq!(harness.broker.open_orders().await.unwrap().len(), 1);
    // Y las nuevas quedan bloqueadas
    let preview = harness
        .place_order
        .preview(command(OrderSide::Buy, OrderType::Market, dec!(1)))
        .await
        .unwrap();
    assert!(
        preview
            .assessment
            .violations()
            .iter()
            .any(|v| matches!(v, RiskViolation::KillSwitchEngaged { .. }))
    );
}

#[tokio::test]
async fn holding_keeps_an_existing_lock_untouched() {
    let harness = Harness::new();
    harness.kill_switch.engage("pánico de ayer").await.unwrap();
    let before = harness.kill_switch_repository.current();
    let entries_before = harness.journal.entries().len();

    let held = harness.kill_switch.hold("inicio de sesión").await.unwrap();

    assert_eq!(held, before);
    assert_eq!(harness.kill_switch_repository.current(), before);
    assert_eq!(harness.journal.entries().len(), entries_before);
}
