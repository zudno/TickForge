//! Pruebas del `AlpacaBrokerGateway` contra un servidor HTTP falso.

// Crate de pruebas: unwrap/expect también en funciones auxiliares sin #[test]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{KEY_ID, SECRET_KEY, error_json, gateway, order_json};
use rust_decimal_macros::dec;
use serde_json::json;
use tickforge_adapter_alpaca::{AlpacaBrokerGateway, AlpacaConfig, AlpacaCredentials};
use tickforge_application::ports::{BrokerError, BrokerGateway};
use tickforge_domain::shared::{Money, Price, Quantity, Symbol};
use tickforge_domain::trading::{
    ClientOrderId, OrderId, OrderRequest, OrderSide, OrderStatus, OrderType, TimeInForce,
};
use wiremock::matchers::{body_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn limit_request() -> OrderRequest {
    OrderRequest::new(
        ClientOrderId::new("tf-000001").unwrap(),
        Symbol::new("AAPL").unwrap(),
        OrderSide::Buy,
        OrderType::Limit {
            limit_price: Price::new(dec!(180.5)).unwrap(),
        },
        Quantity::new(dec!(10)).unwrap(),
        TimeInForce::Day,
    )
    .unwrap()
}

#[tokio::test]
async fn account_sends_credentials_and_maps_balances() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/account"))
        .and(header("apca-api-key-id", KEY_ID))
        .and(header("apca-api-secret-key", SECRET_KEY))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "904837e3-3b76-47ec-b432-046db621571b",
            "status": "ACTIVE",
            "currency": "USD",
            "cash": "95000.50",
            "equity": "101250.75",
            "last_equity": "100000",
            "buying_power": "190001",
            "multiplier": "2",
            "trading_blocked": false,
            "account_blocked": false,
            "pattern_day_trader": false
        })))
        .expect(1)
        .mount(&server)
        .await;

    let account = gateway(&server).account().await.unwrap();

    assert_eq!(account.cash(), Money::usd(dec!(95000.50)));
    assert_eq!(account.equity(), Money::usd(dec!(101250.75)));
    assert_eq!(account.buying_power(), Money::usd(dec!(190001)));
    assert_eq!(account.day_pnl().unwrap(), Money::usd(dec!(1250.75)));
    assert!(!account.is_trading_blocked());
}

#[tokio::test]
async fn account_block_flags_block_trading() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/account"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "currency": "USD", "cash": "1", "equity": "1", "last_equity": "1",
            "buying_power": "1", "trading_blocked": false, "account_blocked": true
        })))
        .mount(&server)
        .await;

    let account = gateway(&server).account().await.unwrap();
    assert!(account.is_trading_blocked());
}

#[tokio::test]
async fn wrong_credentials_are_unauthorized() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/account"))
        .respond_with(
            ResponseTemplate::new(401).set_body_json(json!({ "message": "unauthorized." })),
        )
        .mount(&server)
        .await;

    assert_eq!(
        gateway(&server).account().await,
        Err(BrokerError::Unauthorized)
    );
}

#[tokio::test]
async fn positions_are_mapped_and_missing_position_is_none() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/positions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "asset_id": "b0b6dd9d-8b9b-48a9-ba46-b9d54906e415",
            "symbol": "AAPL",
            "exchange": "NASDAQ",
            "asset_class": "us_equity",
            "qty": "5",
            "avg_entry_price": "170.25",
            "side": "long",
            "market_value": "900",
            "cost_basis": "851.25"
        }])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/positions/MSFT"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(error_json(40410000, "position does not exist")),
        )
        .mount(&server)
        .await;

    let gateway = gateway(&server);
    let positions = gateway.positions().await.unwrap();
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].quantity(), Quantity::new(dec!(5)).unwrap());
    assert_eq!(positions[0].average_entry_price(), Some(dec!(170.25)));

    let missing = gateway
        .position(&Symbol::new("MSFT").unwrap())
        .await
        .unwrap();
    assert_eq!(missing, None);
}

#[tokio::test]
async fn short_positions_are_reported_as_invalid() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/positions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "symbol": "TSLA", "qty": "-3", "avg_entry_price": "200", "side": "short"
        }])))
        .mount(&server)
        .await;

    assert!(matches!(
        gateway(&server).positions().await,
        Err(BrokerError::InvalidResponse(_))
    ));
}

#[tokio::test]
async fn open_orders_query_open_status_and_map_partial_fills() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/orders"))
        .and(query_param("status", "open"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([order_json(
            "partially_filled",
            "4",
            Some("180.45")
        )])))
        .expect(1)
        .mount(&server)
        .await;

    let orders = gateway(&server).open_orders().await.unwrap();

    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].status(), OrderStatus::PartiallyFilled);
    assert_eq!(orders[0].filled_quantity(), Quantity::new(dec!(4)).unwrap());
    assert_eq!(orders[0].average_fill_price(), Some(dec!(180.45)));
}

#[tokio::test]
async fn unsupported_order_types_are_reported_as_invalid() {
    let server = MockServer::start().await;
    let mut stop_order = order_json("new", "0", None);
    stop_order["type"] = json!("stop");
    Mock::given(method("GET"))
        .and(path("/v2/orders"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([stop_order])))
        .mount(&server)
        .await;

    assert!(matches!(
        gateway(&server).open_orders().await,
        Err(BrokerError::InvalidResponse(message)) if message.contains("stop")
    ));
}

#[tokio::test]
async fn submit_order_sends_the_expected_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/orders"))
        .and(body_json(json!({
            "symbol": "AAPL",
            "qty": "10",
            "side": "buy",
            "type": "limit",
            "time_in_force": "day",
            "limit_price": "180.5",
            "client_order_id": "tf-000001"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(order_json("accepted", "0", None)))
        .expect(1)
        .mount(&server)
        .await;

    let order = gateway(&server)
        .submit_order(&limit_request())
        .await
        .unwrap();

    assert_eq!(order.status(), OrderStatus::PendingNew);
    assert_eq!(order.id().as_str(), "61e69015-8549-4bfd-b9c3-01e75843f47d");
    assert_eq!(order.client_order_id().as_str(), "tf-000001");
}

#[tokio::test]
async fn duplicate_client_order_id_returns_the_existing_order() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/orders"))
        .respond_with(
            ResponseTemplate::new(422)
                .set_body_json(error_json(40010001, "client_order_id must be unique")),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/orders:by_client_order_id"))
        .and(query_param("client_order_id", "tf-000001"))
        .respond_with(ResponseTemplate::new(200).set_body_json(order_json("new", "0", None)))
        .expect(1)
        .mount(&server)
        .await;

    let order = gateway(&server)
        .submit_order(&limit_request())
        .await
        .unwrap();
    assert_eq!(order.status(), OrderStatus::New);
}

#[tokio::test]
async fn business_rejections_keep_the_broker_message() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/orders"))
        .respond_with(
            ResponseTemplate::new(403)
                .set_body_json(error_json(40310000, "insufficient buying power")),
        )
        .mount(&server)
        .await;

    let result = gateway(&server).submit_order(&limit_request()).await;
    assert_eq!(
        result,
        Err(BrokerError::Rejected {
            reason: "insufficient buying power".to_owned()
        })
    );
}

#[tokio::test]
async fn transient_failures_are_retryable() {
    for (status, expected_retryable) in [(429, true), (503, true), (422, false)] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v2/account"))
            .respond_with(ResponseTemplate::new(status).set_body_string("oops"))
            .mount(&server)
            .await;

        let error = gateway(&server).account().await.unwrap_err();
        assert_eq!(
            error.is_retryable(),
            expected_retryable,
            "HTTP {status} -> {error:?}"
        );
    }
}

#[tokio::test]
async fn unreachable_broker_is_unavailable() {
    // Puerto 9 (discard) cerrado en local: falla de conexión, no de HTTP
    let mut config = AlpacaConfig::paper(AlpacaCredentials::new(
        KEY_ID.to_owned(),
        SECRET_KEY.to_owned(),
    ))
    .with_base_urls("http://127.0.0.1:9", "http://127.0.0.1:9");
    // Sin reintentos: en Windows cada conexión rechazada tarda ~2 s
    config.max_retries = 0;
    let gateway = AlpacaBrokerGateway::new(&config).unwrap();

    assert!(matches!(
        gateway.account().await,
        Err(BrokerError::Unavailable(_))
    ));
}

#[tokio::test]
async fn cancel_order_maps_success_rejection_and_not_found() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/v2/orders/live-order"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/v2/orders/filled-order"))
        .respond_with(
            ResponseTemplate::new(422)
                .set_body_json(error_json(42210000, "order is not cancelable")),
        )
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/v2/orders/ghost-order"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(error_json(40410000, "order not found")),
        )
        .mount(&server)
        .await;

    let gateway = gateway(&server);
    let id = |raw: &str| OrderId::new(raw).unwrap();

    assert_eq!(gateway.cancel_order(&id("live-order")).await, Ok(()));
    assert!(matches!(
        gateway.cancel_order(&id("filled-order")).await,
        Err(BrokerError::Rejected { .. })
    ));
    assert_eq!(
        gateway.cancel_order(&id("ghost-order")).await,
        Err(BrokerError::OrderNotFound(id("ghost-order")))
    );
}

#[tokio::test]
async fn unknown_order_is_none() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/orders/ghost-order"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(error_json(40410000, "order not found")),
        )
        .mount(&server)
        .await;

    let order = gateway(&server)
        .order(&OrderId::new("ghost-order").unwrap())
        .await
        .unwrap();
    assert_eq!(order, None);
}

#[tokio::test]
async fn market_clock_is_mapped_from_new_york_time() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/clock"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "timestamp": "2024-03-01T10:00:00.000000-05:00",
            "is_open": true,
            "next_open": "2024-03-04T09:30:00-05:00",
            "next_close": "2024-03-01T16:00:00-05:00"
        })))
        .mount(&server)
        .await;

    let clock = gateway(&server).market_clock().await.unwrap();

    assert!(clock.is_open());
    assert_eq!(clock.time_until_next_change(), chrono::TimeDelta::hours(6));
}

#[tokio::test]
async fn order_by_client_order_id_finds_or_reports_missing_orders() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/orders:by_client_order_id"))
        .and(query_param("client_order_id", "tf-000001"))
        .respond_with(ResponseTemplate::new(200).set_body_json(order_json("new", "0", None)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/orders:by_client_order_id"))
        .and(query_param("client_order_id", "tf-never-sent"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(error_json(40410000, "order not found")),
        )
        .mount(&server)
        .await;

    let gateway = gateway(&server);
    let found = gateway
        .order_by_client_order_id(&ClientOrderId::new("tf-000001").unwrap())
        .await
        .unwrap();
    let missing = gateway
        .order_by_client_order_id(&ClientOrderId::new("tf-never-sent").unwrap())
        .await
        .unwrap();

    assert_eq!(found.map(|order| order.status()), Some(OrderStatus::New));
    assert_eq!(missing, None);
}

// --- Reintentos (fase 7) ---

fn account_json() -> serde_json::Value {
    json!({
        "currency": "USD", "cash": "100", "equity": "100", "last_equity": "100",
        "buying_power": "100", "trading_blocked": false, "account_blocked": false
    })
}

#[tokio::test]
async fn transient_errors_are_retried_until_the_request_succeeds() {
    let server = MockServer::start().await;
    // Las dos primeras respuestas fallan; la tercera funciona
    Mock::given(method("GET"))
        .and(path("/v2/account"))
        .respond_with(ResponseTemplate::new(503).set_body_string("mantenimiento"))
        .up_to_n_times(2)
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/account"))
        .respond_with(ResponseTemplate::new(200).set_body_json(account_json()))
        .expect(1)
        .mount(&server)
        .await;

    let account = gateway(&server).account().await.unwrap();
    assert_eq!(account.cash(), Money::usd(dec!(100)));
}

#[tokio::test]
async fn rate_limit_responses_wait_for_retry_after() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/account"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("Retry-After", "1")
                .set_body_json(error_json(42910000, "rate limit exceeded")),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/account"))
        .respond_with(ResponseTemplate::new(200).set_body_json(account_json()))
        .mount(&server)
        .await;

    let start = std::time::Instant::now();
    gateway(&server).account().await.unwrap();
    assert!(start.elapsed() >= std::time::Duration::from_secs(1));
}

#[tokio::test]
async fn submitting_an_order_is_retried_safely() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/orders"))
        .respond_with(ResponseTemplate::new(502).set_body_string("bad gateway"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    // El reintento lleva el mismo client_order_id
    Mock::given(method("POST"))
        .and(path("/v2/orders"))
        .and(wiremock::matchers::body_partial_json(
            json!({ "client_order_id": "tf-000001" }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(order_json("new", "0", None)))
        .expect(1)
        .mount(&server)
        .await;

    let order = gateway(&server)
        .submit_order(&limit_request())
        .await
        .unwrap();
    assert_eq!(order.status(), OrderStatus::New);
}

#[tokio::test]
async fn cancellations_are_never_retried() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/v2/orders/live-order"))
        .respond_with(ResponseTemplate::new(503).set_body_string("mantenimiento"))
        .expect(1)
        .mount(&server)
        .await;

    let result = gateway(&server)
        .cancel_order(&OrderId::new("live-order").unwrap())
        .await;
    assert!(matches!(result, Err(BrokerError::Unavailable(_))));
}

#[tokio::test]
async fn client_errors_are_not_retried() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/orders"))
        .respond_with(
            ResponseTemplate::new(403)
                .set_body_json(error_json(40310000, "insufficient buying power")),
        )
        .expect(1)
        .mount(&server)
        .await;

    let result = gateway(&server).submit_order(&limit_request()).await;
    assert!(matches!(result, Err(BrokerError::Rejected { .. })));
}
