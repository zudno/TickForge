//! Pruebas de integración de la consulta de portafolio y del flujo de actualizaciones.

mod common;

use common::{Harness, command, limit};
use futures::StreamExt;
use rust_decimal_macros::dec;
use tickforge_application::ports::BrokerGateway;
use tickforge_domain::shared::{Money, Quantity};
use tickforge_domain::trading::{Order, OrderSide, OrderStatus, OrderType};

#[tokio::test]
async fn snapshot_reflects_fills_and_open_orders() {
    let harness = Harness::new();
    harness.quote(dec!(100), dec!(100));

    harness
        .place_order
        .execute(command(OrderSide::Buy, OrderType::Market, dec!(10)))
        .await
        .unwrap();
    harness
        .place_order
        .execute(command(OrderSide::Buy, limit(dec!(95)), dec!(1)))
        .await
        .unwrap();

    let snapshot = harness.portfolio.execute().await.unwrap();

    assert_eq!(snapshot.account.cash(), Money::usd(dec!(9000)));
    assert_eq!(snapshot.account.equity(), Money::usd(dec!(10000)));
    assert_eq!(snapshot.positions.len(), 1);
    assert_eq!(
        snapshot.positions[0].quantity(),
        Quantity::new(dec!(10)).unwrap()
    );
    assert_eq!(snapshot.open_orders.len(), 1);
}

#[tokio::test]
async fn applying_broker_updates_mirrors_the_broker_order() {
    let harness = Harness::new();
    harness.quote(dec!(99.99), dec!(100));
    let mut updates = harness.broker.order_updates().await.unwrap();

    let submitted = harness
        .place_order
        .execute(command(OrderSide::Buy, limit(dec!(99)), dec!(2)))
        .await
        .unwrap();
    // El ask baja al límite y la orden se ejecuta
    harness.quote(dec!(98.90), dec!(99));

    // Copia local reconstruida solo a partir de los eventos del broker
    let mut local = Order::new(
        submitted.id().clone(),
        submitted.request().clone(),
        submitted.submitted_at(),
    );
    for _ in 0..2 {
        let update = updates.next().await.unwrap();
        update.apply_to(&mut local).unwrap();
    }

    let remote = harness.broker.order(submitted.id()).await.unwrap().unwrap();
    assert_eq!(local.status(), OrderStatus::Filled);
    assert_eq!(local.status(), remote.status());
    assert_eq!(local.average_fill_price(), remote.average_fill_price());
}
