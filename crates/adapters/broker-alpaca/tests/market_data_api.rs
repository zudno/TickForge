//! Pruebas de `AlpacaMarketData` contra un servidor HTTP falso.

// Crate de pruebas: unwrap/expect también en funciones auxiliares sin #[test]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::market_data;
use rust_decimal_macros::dec;
use serde_json::json;
use tickforge_application::ports::{MarketDataError, MarketDataFeed};
use tickforge_domain::market::Timeframe;
use tickforge_domain::shared::{Quantity, Symbol};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn aapl() -> Symbol {
    Symbol::new("AAPL").unwrap()
}

#[tokio::test]
async fn latest_quote_uses_the_configured_feed_and_maps_prices() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/stocks/AAPL/quotes/latest"))
        .and(query_param("feed", "iex"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "symbol": "AAPL",
            "quote": {
                "t": "2024-03-01T19:59:59.123456789Z",
                "ax": "V", "ap": 179.67, "as": 2,
                "bx": "V", "bp": 179.65, "bs": 1,
                "c": ["R"], "z": "C"
            }
        })))
        .expect(1)
        .mount(&server)
        .await;

    let quote = market_data(&server).latest_quote(&aapl()).await.unwrap();

    assert_eq!(quote.bid().unwrap().value(), dec!(179.65));
    assert_eq!(quote.ask().unwrap().value(), dec!(179.67));
    assert_eq!(quote.ask_size(), Quantity::new(dec!(2)).unwrap());
    assert_eq!(quote.spread().unwrap(), Some(dec!(0.02)));
}

#[tokio::test]
async fn zero_price_means_that_side_has_no_offers() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/stocks/AAPL/quotes/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "symbol": "AAPL",
            "quote": { "t": "2024-03-01T23:00:00Z", "ap": 180.1, "as": 3, "bp": 0, "bs": 0 }
        })))
        .mount(&server)
        .await;

    let quote = market_data(&server).latest_quote(&aapl()).await.unwrap();
    assert_eq!(quote.bid(), None);
    assert!(quote.ask().is_some());
}

#[tokio::test]
async fn bars_request_latest_page_and_return_ascending_order() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/stocks/AAPL/bars"))
        .and(query_param("timeframe", "5Min"))
        .and(query_param("limit", "2"))
        .and(query_param("sort", "desc"))
        .and(query_param("adjustment", "raw"))
        .and(query_param("feed", "iex"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "symbol": "AAPL",
            "next_page_token": null,
            "bars": [
                { "t": "2024-03-01T19:55:00Z", "o": 179.5, "h": 179.9, "l": 179.4, "c": 179.66, "v": 12000, "n": 140, "vw": 179.62 },
                { "t": "2024-03-01T19:50:00Z", "o": 179.2, "h": 179.6, "l": 179.1, "c": 179.5, "v": 9000, "n": 120, "vw": 179.4 }
            ]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let bars = market_data(&server)
        .bars(&aapl(), Timeframe::Minute5, 2)
        .await
        .unwrap();

    assert_eq!(bars.len(), 2);
    assert!(bars[0].starts_at() < bars[1].starts_at());
    assert_eq!(bars[1].close().value(), dec!(179.66));
    assert_eq!(bars[1].volume(), Quantity::new(dec!(12000)).unwrap());
}

#[tokio::test]
async fn null_bars_mean_an_empty_series() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/stocks/AAPL/bars"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "symbol": "AAPL", "bars": null, "next_page_token": null
        })))
        .mount(&server)
        .await;

    let bars = market_data(&server)
        .bars(&aapl(), Timeframe::Day1, 30)
        .await
        .unwrap();
    assert!(bars.is_empty());
}

#[tokio::test]
async fn http_errors_are_mapped_to_market_data_errors() {
    let cases = [
        (404, MarketDataError::NoData(aapl())),
        (401, MarketDataError::Unauthorized),
        (429, MarketDataError::RateLimited),
    ];
    for (status, expected) in cases {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v2/stocks/AAPL/quotes/latest"))
            .respond_with(ResponseTemplate::new(status).set_body_string("{}"))
            .mount(&server)
            .await;

        assert_eq!(
            market_data(&server).latest_quote(&aapl()).await,
            Err(expected),
            "HTTP {status}"
        );
    }
}

#[tokio::test]
async fn streaming_is_not_available_yet() {
    let server = MockServer::start().await;
    assert!(matches!(
        market_data(&server).subscribe(&[aapl()]).await,
        Err(MarketDataError::Unavailable(_))
    ));
}
