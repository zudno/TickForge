//! Pérdida máxima del día y tope de órdenes, contra el broker simulado.

// Crate de pruebas: unwrap/expect también en funciones auxiliares sin #[test]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use chrono::TimeDelta;
use common::{Harness, command, limit};
use rust_decimal_macros::dec;
use tickforge_application::ApplicationError;
use tickforge_application::ports::{BrokerGateway, JournalEntry};
use tickforge_application::use_cases::trading::{DailyLossGuard, PlaceOrderUseCase};
use tickforge_domain::risk::{KillSwitch, RiskLimits, RiskPolicy, RiskViolation};
use tickforge_domain::shared::Money;
use tickforge_domain::trading::{OrderSide, OrderStatus, OrderType};

/// Límites del arnés más pérdida diaria de 100 USD y tope de 2 órdenes/min y 3/día.
fn strict_limits() -> RiskLimits {
    RiskLimits::new(
        Money::usd(dec!(5000)),
        Money::usd(dec!(8000)),
        dec!(5),
        TimeDelta::seconds(5),
    )
    .unwrap()
    .with_max_daily_loss(Money::usd(dec!(100)))
    .unwrap()
    .with_order_rate_limits(2, 3)
    .unwrap()
}

fn strict(harness: &Harness) -> PlaceOrderUseCase {
    PlaceOrderUseCase::new(harness.ports.clone(), RiskPolicy::new(strict_limits()))
}

fn rejected_with(
    result: Result<tickforge_domain::trading::Order, ApplicationError>,
    expected: fn(&RiskViolation) -> bool,
) -> bool {
    matches!(result, Err(ApplicationError::RiskRejected(violations)) if violations.iter().any(expected))
}

#[tokio::test]
async fn the_order_cap_per_minute_resets_as_the_window_moves() {
    let harness = Harness::new();
    let place_order = strict(&harness);
    let buy = || command(OrderSide::Buy, limit(dec!(90)), dec!(1));

    harness.quote(dec!(89.99), dec!(90));
    place_order.execute(buy()).await.unwrap();
    place_order.execute(buy()).await.unwrap();
    let third = place_order.execute(buy()).await;
    assert!(rejected_with(third, |v| matches!(
        v,
        RiskViolation::TooManyOrdersPerMinute { count: 2, limit: 2 }
    )));

    // Un minuto después vuelve a haber cupo por minuto
    harness.clock.advance(TimeDelta::seconds(61));
    harness.quote(dec!(89.99), dec!(90));
    place_order.execute(buy()).await.unwrap();
}

#[tokio::test]
async fn the_daily_order_cap_counts_the_last_24_hours() {
    let harness = Harness::new();
    let place_order = strict(&harness);
    let buy = || command(OrderSide::Buy, limit(dec!(90)), dec!(1));

    for _ in 0..3 {
        harness.quote(dec!(89.99), dec!(90));
        place_order.execute(buy()).await.unwrap();
        harness.clock.advance(TimeDelta::minutes(5));
    }
    harness.quote(dec!(89.99), dec!(90));
    let fourth = place_order.execute(buy()).await;
    assert!(rejected_with(fourth, |v| matches!(
        v,
        RiskViolation::TooManyOrdersPerDay { count: 3, limit: 3 }
    )));

    harness.clock.advance(TimeDelta::hours(24));
    harness.quote(dec!(89.99), dec!(90));
    place_order.execute(buy()).await.unwrap();
}

#[tokio::test]
async fn rejected_orders_do_not_use_up_the_cap() {
    let harness = Harness::new();
    let place_order = strict(&harness);
    harness.quote(dec!(89.99), dec!(90));

    // Bloqueadas por riesgo (precio límite absurdo): no llegan al diario como intención
    for _ in 0..3 {
        let result = place_order
            .execute(command(OrderSide::Buy, limit(dec!(50)), dec!(1)))
            .await;
        assert!(matches!(result, Err(ApplicationError::RiskRejected(_))));
    }
    place_order
        .execute(command(OrderSide::Buy, limit(dec!(90)), dec!(1)))
        .await
        .unwrap();
}

#[tokio::test]
async fn reaching_the_daily_loss_blocks_new_orders() {
    let harness = Harness::new();
    harness.quote(dec!(99.99), dec!(100));
    harness.broker.set_last_equity(Money::usd(dec!(10100)));

    let result = strict(&harness)
        .execute(command(OrderSide::Buy, OrderType::Market, dec!(1)))
        .await;
    assert!(rejected_with(result, |v| matches!(
        v,
        RiskViolation::DailyLossLimitReached { .. }
    )));
}

#[tokio::test]
async fn the_guard_engages_the_kill_switch_once_and_cancels_open_orders() {
    let harness = Harness::new();
    harness.quote(dec!(99.99), dec!(100));
    let resting = harness
        .place_order
        .execute(command(OrderSide::Buy, limit(dec!(99)), dec!(1)))
        .await
        .unwrap();
    let guard = DailyLossGuard::new(harness.ports.clone(), strict_limits());

    // Sin pérdida: nada que hacer
    let account = harness.broker.account().await.unwrap();
    assert!(guard.check(&account).await.unwrap().is_none());
    assert!(!harness.kill_switch_repository.current().is_engaged());

    // Pérdida de 100 USD (justo el máximo): se activa y cancela la orden abierta
    harness.broker.set_last_equity(Money::usd(dec!(10100)));
    let account = harness.broker.account().await.unwrap();
    let report = guard.check(&account).await.unwrap().unwrap();
    assert_eq!(report.canceled, vec![resting.id().clone()]);
    let current = harness.broker.order(resting.id()).await.unwrap().unwrap();
    assert_eq!(current.status(), OrderStatus::Canceled);

    let KillSwitch::Engaged { reason, .. } = harness.kill_switch_repository.current() else {
        panic!("el kill switch debería estar activo");
    };
    assert!(reason.contains("pérdida del día"), "{reason}");
    assert!(harness.journal.entries().iter().any(|entry| matches!(
        entry,
        JournalEntry::KillSwitchEngaged { reason, .. } if reason.contains("pérdida del día")
    )));

    // Ya activo: una segunda revisión no repite nada
    assert!(guard.check(&account).await.unwrap().is_none());
}
