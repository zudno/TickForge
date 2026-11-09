//! Pruebas de integración de `CancelOrderUseCase` contra el broker simulado.

mod common;

use common::{Harness, command, limit};
use rust_decimal_macros::dec;
use tickforge_application::ApplicationError;
use tickforge_application::ports::{ApplicationEvent, BrokerGateway, JournalEntry};
use tickforge_domain::DomainError;
use tickforge_domain::trading::{OrderEventKind, OrderId, OrderSide, OrderStatus, OrderType};

#[tokio::test]
async fn cancels_a_resting_limit_order() {
    let harness = Harness::new();
    harness.quote(dec!(99.99), dec!(100));
    let order = harness
        .place_order
        .execute(command(OrderSide::Buy, limit(dec!(99)), dec!(1)))
        .await
        .unwrap();

    harness.cancel_order.execute(order.id()).await.unwrap();

    let order = harness.broker.order(order.id()).await.unwrap().unwrap();
    assert_eq!(order.status(), OrderStatus::Canceled);
    assert!(matches!(
        harness.journal.entries().last(),
        Some(JournalEntry::CancelRequested { .. })
    ));
    assert!(matches!(
        harness.events.events().last(),
        Some(ApplicationEvent::Order(event)) if event.kind == OrderEventKind::CancelRequested
    ));
}

#[tokio::test]
async fn filled_order_cannot_be_canceled() {
    let harness = Harness::new();
    harness.quote(dec!(99.99), dec!(100));
    let order = harness
        .place_order
        .execute(command(OrderSide::Buy, OrderType::Market, dec!(1)))
        .await
        .unwrap();

    let result = harness.cancel_order.execute(order.id()).await;

    assert!(matches!(
        result,
        Err(ApplicationError::Domain(
            DomainError::InvalidStatusTransition { .. }
        ))
    ));
}

#[tokio::test]
async fn unknown_order_is_not_found() {
    let harness = Harness::new();
    let missing = OrderId::new("does-not-exist").unwrap();

    let result = harness.cancel_order.execute(&missing).await;

    assert_eq!(result, Err(ApplicationError::OrderNotFound(missing)));
}
