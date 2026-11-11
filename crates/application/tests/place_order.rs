//! Pruebas de integración de `PlaceOrderUseCase` contra el broker simulado.

mod common;

use chrono::TimeDelta;
use common::{Harness, command, limit};
use rust_decimal_macros::dec;
use tickforge_application::ApplicationError;
use tickforge_application::ports::{ApplicationEvent, BrokerError, JournalEntry};
use tickforge_domain::risk::RiskViolation;
use tickforge_domain::trading::{OrderSide, OrderStatus, OrderType};

#[tokio::test]
async fn approved_market_buy_is_journaled_submitted_and_filled() {
    let harness = Harness::new();
    harness.quote(dec!(99.99), dec!(100));

    let order = harness
        .place_order
        .execute(command(OrderSide::Buy, OrderType::Market, dec!(10)))
        .await
        .unwrap();

    assert_eq!(order.status(), OrderStatus::Filled);
    assert_eq!(order.client_order_id().as_str(), "tf-test-000001");

    let journal = harness.journal.entries();
    assert!(matches!(
        journal.as_slice(),
        [JournalEntry::Intent { .. }, JournalEntry::Submitted { .. }]
    ));
    assert!(matches!(
        harness.events.events().as_slice(),
        [ApplicationEvent::OrderSubmitted(_)]
    ));
}

#[tokio::test]
async fn risk_rejection_never_reaches_the_broker() {
    let harness = Harness::new();
    harness.quote(dec!(99.99), dec!(100));

    // 60 acciones * 100 = 6,000 > 5,000 de máximo por orden
    let result = harness
        .place_order
        .execute(command(OrderSide::Buy, OrderType::Market, dec!(60)))
        .await;

    let Err(ApplicationError::RiskRejected(violations)) = result else {
        panic!("se esperaba un rechazo por riesgo, se obtuvo {result:?}");
    };
    assert!(
        violations
            .iter()
            .any(|v| matches!(v, RiskViolation::OrderNotionalExceeded { .. }))
    );
    assert_eq!(harness.broker.order_count(), 0);
    assert!(matches!(
        harness.journal.entries().as_slice(),
        [JournalEntry::RiskRejected { .. }]
    ));
    assert!(matches!(
        harness.events.events().as_slice(),
        [ApplicationEvent::OrderRejectedByRisk { .. }]
    ));
}

#[tokio::test]
async fn stale_quote_blocks_the_order() {
    let harness = Harness::new();
    harness.quote(dec!(99.99), dec!(100));
    harness.clock.advance(TimeDelta::seconds(30));

    let result = harness
        .place_order
        .execute(command(OrderSide::Buy, OrderType::Market, dec!(1)))
        .await;

    assert!(matches!(
        result,
        Err(ApplicationError::RiskRejected(ref violations))
            if matches!(violations.as_slice(), [RiskViolation::StaleQuote { .. }])
    ));
}

#[tokio::test]
async fn broker_failure_is_journaled_and_reported() {
    let harness = Harness::new();
    harness.quote(dec!(99.99), dec!(100));
    harness
        .broker
        .fail_next_submit(BrokerError::Unavailable("timeout".to_owned()));

    let result = harness
        .place_order
        .execute(command(OrderSide::Buy, OrderType::Market, dec!(1)))
        .await;

    assert!(matches!(
        result,
        Err(ApplicationError::Broker(BrokerError::Unavailable(_)))
    ));
    assert!(matches!(
        harness.journal.entries().as_slice(),
        [
            JournalEntry::Intent { .. },
            JournalEntry::SubmissionFailed { .. }
        ]
    ));
    assert!(matches!(
        harness.events.events().as_slice(),
        [ApplicationEvent::OrderSubmissionFailed { .. }]
    ));
}

#[tokio::test]
async fn nothing_is_sent_if_the_intent_cannot_be_journaled() {
    let harness = Harness::new();
    harness.quote(dec!(99.99), dec!(100));
    harness.journal.set_fail_writes(true);

    let result = harness
        .place_order
        .execute(command(OrderSide::Buy, OrderType::Market, dec!(1)))
        .await;

    assert!(matches!(result, Err(ApplicationError::Journal(_))));
    assert_eq!(harness.broker.order_count(), 0);
}

#[tokio::test]
async fn missing_quote_is_a_market_data_error() {
    let harness = Harness::new();

    let result = harness
        .place_order
        .execute(command(OrderSide::Buy, limit(dec!(100)), dec!(1)))
        .await;

    assert!(matches!(result, Err(ApplicationError::MarketData(_))));
    assert_eq!(harness.broker.order_count(), 0);
}

#[tokio::test]
async fn each_order_gets_a_new_client_order_id() {
    let harness = Harness::new();
    harness.quote(dec!(99.99), dec!(100));

    let first = harness
        .place_order
        .execute(command(OrderSide::Buy, limit(dec!(99)), dec!(1)))
        .await
        .unwrap();
    let second = harness
        .place_order
        .execute(command(OrderSide::Buy, limit(dec!(99)), dec!(1)))
        .await
        .unwrap();

    assert_ne!(first.client_order_id(), second.client_order_id());
    assert_eq!(harness.broker.order_count(), 2);
}

// --- Vista previa y confirmación (flujo de la TUI) ---

#[tokio::test]
async fn preview_estimates_cost_without_sending_anything() {
    let harness = Harness::new();
    harness.quote(dec!(99.99), dec!(100));

    let preview = harness
        .place_order
        .preview(command(OrderSide::Buy, OrderType::Market, dec!(10)))
        .await
        .unwrap();

    assert!(preview.assessment.is_approved());
    assert_eq!(preview.reference_price.map(|p| p.value()), Some(dec!(100)));
    assert_eq!(
        preview.estimated_notional,
        Some(tickforge_domain::shared::Money::usd(dec!(1000)))
    );
    assert_eq!(harness.broker.order_count(), 0);
    assert!(harness.journal.entries().is_empty());
}

#[tokio::test]
async fn preview_lists_risk_violations_instead_of_failing() {
    let harness = Harness::new();
    harness.quote(dec!(99.99), dec!(100));

    let preview = harness
        .place_order
        .preview(command(OrderSide::Buy, OrderType::Market, dec!(60)))
        .await
        .unwrap();

    assert!(!preview.assessment.is_approved());
    assert!(
        preview
            .assessment
            .violations()
            .iter()
            .any(|v| matches!(v, RiskViolation::OrderNotionalExceeded { .. }))
    );
    assert_eq!(harness.broker.order_count(), 0);
}

#[tokio::test]
async fn confirming_the_same_preview_twice_never_duplicates_the_order() {
    let harness = Harness::new();
    harness.quote(dec!(99.99), dec!(100));
    let preview = harness
        .place_order
        .preview(command(OrderSide::Buy, limit(dec!(99)), dec!(1)))
        .await
        .unwrap();

    let first = harness
        .place_order
        .submit(preview.request.clone())
        .await
        .unwrap();
    let second = harness.place_order.submit(preview.request).await.unwrap();

    assert_eq!(first.id(), second.id());
    assert_eq!(harness.broker.order_count(), 1);
}

#[tokio::test]
async fn submit_re_evaluates_risk_with_fresh_data() {
    let harness = Harness::new();
    harness.quote(dec!(99.99), dec!(100));
    let preview = harness
        .place_order
        .preview(command(OrderSide::Buy, OrderType::Market, dec!(1)))
        .await
        .unwrap();
    assert!(preview.assessment.is_approved());

    // El usuario tarda en confirmar y la cotización envejece
    harness.clock.advance(TimeDelta::seconds(30));
    let result = harness.place_order.submit(preview.request).await;

    assert!(matches!(result, Err(ApplicationError::RiskRejected(_))));
    assert_eq!(harness.broker.order_count(), 0);
}
