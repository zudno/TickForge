//! Regla del modo real: órdenes a mercado solo con el mercado abierto.

// Crate de pruebas: unwrap/expect también en funciones auxiliares sin #[test]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use chrono::TimeDelta;
use common::{Harness, command, limit};
use rust_decimal_macros::dec;
use tickforge_application::ApplicationError;
use tickforge_application::use_cases::trading::PlaceOrderUseCase;
use tickforge_domain::risk::{RiskLimits, RiskPolicy, RiskViolation};
use tickforge_domain::shared::Money;
use tickforge_domain::trading::{OrderSide, OrderType};

/// Caso de uso con los límites del modo real: mercado solo con la sesión abierta.
fn strict(harness: &Harness) -> PlaceOrderUseCase {
    let limits = RiskLimits::new(
        Money::usd(dec!(5000)),
        Money::usd(dec!(8000)),
        dec!(5),
        TimeDelta::seconds(5),
    )
    .unwrap()
    .restrict_market_orders_to_open_market();
    PlaceOrderUseCase::new(harness.ports.clone(), RiskPolicy::new(limits))
}

fn blocked_by_market_hours(violations: &[RiskViolation]) -> bool {
    violations.contains(&RiskViolation::MarketOrderOutsideRegularHours)
}

#[tokio::test]
async fn market_orders_are_blocked_while_the_market_is_closed() {
    let harness = Harness::new();
    harness.quote(dec!(99.99), dec!(100));
    harness.broker.set_market_open(false);
    let place_order = strict(&harness);

    let preview = place_order
        .preview(command(OrderSide::Buy, OrderType::Market, dec!(1)))
        .await
        .unwrap();
    assert!(blocked_by_market_hours(preview.assessment.violations()));

    let result = place_order
        .execute(command(OrderSide::Buy, OrderType::Market, dec!(1)))
        .await;
    assert!(matches!(
        result,
        Err(ApplicationError::RiskRejected(violations)) if blocked_by_market_hours(&violations)
    ));
}

#[tokio::test]
async fn market_orders_pass_with_the_market_open() {
    let harness = Harness::new();
    harness.quote(dec!(99.99), dec!(100));
    harness.broker.set_market_open(true);

    let order = strict(&harness)
        .execute(command(OrderSide::Buy, OrderType::Market, dec!(1)))
        .await
        .unwrap();
    assert_eq!(order.symbol().as_str(), "AAPL");
}

#[tokio::test]
async fn limit_orders_are_accepted_with_the_market_closed() {
    let harness = Harness::new();
    harness.quote(dec!(99.99), dec!(100));
    harness.broker.set_market_open(false);

    let preview = strict(&harness)
        .preview(command(OrderSide::Buy, limit(dec!(99.50)), dec!(1)))
        .await
        .unwrap();
    assert!(
        preview.assessment.is_approved(),
        "{:?}",
        preview.assessment.violations()
    );
}

#[tokio::test]
async fn paper_limits_ignore_market_hours() {
    let harness = Harness::new();
    harness.quote(dec!(99.99), dec!(100));
    harness.broker.set_market_open(false);

    let preview = harness
        .place_order
        .preview(command(OrderSide::Buy, OrderType::Market, dec!(1)))
        .await
        .unwrap();
    assert!(!blocked_by_market_hours(preview.assessment.violations()));
}
