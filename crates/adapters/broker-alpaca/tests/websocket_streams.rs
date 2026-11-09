//! Pruebas de los WebSocket de Alpaca contra un servidor WebSocket local con guion.

// Crate de pruebas: unwrap/expect también en funciones auxiliares sin #[test]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::future::Future;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use futures::{SinkExt, StreamExt};
use rust_decimal_macros::dec;
use serde_json::{Value, json};
use tickforge_adapter_alpaca::{
    AlpacaBrokerGateway, AlpacaConfig, AlpacaCredentials, AlpacaMarketData,
};
use tickforge_application::ports::{
    BrokerError, BrokerGateway, MarketDataError, MarketDataFeed, MarketEvent, OrderUpdateKind,
};
use tickforge_domain::shared::{Quantity, Symbol};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;

use common::{FixedClock, KEY_ID, SECRET_KEY};

type ServerSocket = WebSocketStream<TcpStream>;

/// Levanta un servidor WebSocket que atiende una sola conexión siguiendo `script`.
/// Devuelve la URL `ws://` para conectarse.
async fn scripted_server<F, Fut>(script: F) -> String
where
    F: FnOnce(ServerSocket) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let socket = tokio_tungstenite::accept_async(tcp).await.unwrap();
        script(socket).await;
    });
    format!("ws://{address}")
}

async fn receive(socket: &mut ServerSocket) -> Value {
    loop {
        let message = socket.next().await.unwrap().unwrap();
        if message.is_text() || message.is_binary() {
            return serde_json::from_slice(&message.into_data()).unwrap();
        }
    }
}

/// El stream de trading de Alpaca responde en frames binarios.
async fn send_binary(socket: &mut ServerSocket, value: Value) {
    socket
        .send(Message::binary(value.to_string().into_bytes()))
        .await
        .unwrap();
}

async fn send_text(socket: &mut ServerSocket, value: Value) {
    socket.send(Message::text(value.to_string())).await.unwrap();
}

fn config(trading_stream: &str, data_stream_base: &str) -> AlpacaConfig {
    AlpacaConfig::paper(AlpacaCredentials::new(
        KEY_ID.to_owned(),
        SECRET_KEY.to_owned(),
    ))
    .with_stream_urls(trading_stream, data_stream_base)
}

fn market_data(data_stream_base: &str) -> AlpacaMarketData {
    let clock = Arc::new(FixedClock(DateTime::<Utc>::UNIX_EPOCH));
    AlpacaMarketData::new(&config("ws://unused", data_stream_base), clock).unwrap()
}

fn trade_update(event: &str, extra: Value) -> Value {
    let mut data = json!({
        "event": event,
        "timestamp": "2024-03-01T15:30:02.5Z",
        "order": { "id": "ord-1", "updated_at": "2024-03-01T15:30:02Z", "status": event }
    });
    if let (Some(target), Value::Object(fields)) = (data.as_object_mut(), extra) {
        target.extend(fields);
    }
    json!({ "stream": "trade_updates", "data": data })
}

#[tokio::test]
async fn trade_updates_authenticate_listen_and_stream_events() {
    let url = scripted_server(|mut socket| async move {
        let auth = receive(&mut socket).await;
        assert_eq!(auth["action"], "auth");
        assert_eq!(auth["key"], KEY_ID);
        assert_eq!(auth["secret"], SECRET_KEY);
        send_binary(
            &mut socket,
            json!({ "stream": "authorization", "data": { "action": "authenticate", "status": "authorized" } }),
        )
        .await;

        let listen = receive(&mut socket).await;
        assert_eq!(listen["data"]["streams"], json!(["trade_updates"]));
        send_binary(
            &mut socket,
            json!({ "stream": "listening", "data": { "streams": ["trade_updates"] } }),
        )
        .await;

        // `pending_new` no cambia el dominio y debe ignorarse
        send_binary(&mut socket, trade_update("pending_new", json!({}))).await;
        send_binary(&mut socket, trade_update("new", json!({}))).await;
        send_binary(
            &mut socket,
            trade_update(
                "partial_fill",
                json!({ "execution_id": "exec-1", "price": "179.08", "qty": "3", "position_qty": "3" }),
            ),
        )
        .await;
        socket.close(None).await.unwrap();
    })
    .await;

    let gateway = AlpacaBrokerGateway::new(&config(&url, "ws://unused")).unwrap();
    let updates: Vec<_> = gateway.order_updates().await.unwrap().collect().await;

    assert_eq!(updates.len(), 2);
    assert_eq!(updates[0].kind, OrderUpdateKind::Accepted);
    let OrderUpdateKind::Filled(fill) = &updates[1].kind else {
        panic!("se esperaba una ejecución, llegó {:?}", updates[1].kind);
    };
    assert_eq!(fill.id().as_str(), "exec-1");
    assert_eq!(fill.price().value(), dec!(179.08));
    assert_eq!(fill.quantity(), Quantity::new(dec!(3)).unwrap());
    assert_eq!(updates[1].order_id.as_str(), "ord-1");
}

#[tokio::test]
async fn trade_updates_with_wrong_keys_are_unauthorized() {
    let url = scripted_server(|mut socket| async move {
        receive(&mut socket).await;
        send_binary(
            &mut socket,
            json!({ "stream": "authorization", "data": { "action": "authenticate", "status": "unauthorized" } }),
        )
        .await;
    })
    .await;

    let gateway = AlpacaBrokerGateway::new(&config(&url, "ws://unused")).unwrap();
    assert!(matches!(
        gateway.order_updates().await,
        Err(BrokerError::Unauthorized)
    ));
}

#[tokio::test]
async fn market_stream_subscribes_and_maps_quotes_trades_and_bars() {
    let base = scripted_server(|mut socket| async move {
        send_text(&mut socket, json!([{ "T": "success", "msg": "connected" }])).await;

        let auth = receive(&mut socket).await;
        assert_eq!(auth["action"], "auth");
        send_text(&mut socket, json!([{ "T": "success", "msg": "authenticated" }])).await;

        let subscribe = receive(&mut socket).await;
        assert_eq!(subscribe["action"], "subscribe");
        assert_eq!(subscribe["quotes"], json!(["AAPL"]));
        assert_eq!(subscribe["trades"], json!(["AAPL"]));
        assert_eq!(subscribe["bars"], json!(["AAPL"]));
        send_text(
            &mut socket,
            json!([{ "T": "subscription", "quotes": ["AAPL"], "trades": ["AAPL"], "bars": ["AAPL"] }]),
        )
        .await;

        // Varios eventos en un mismo mensaje, como hace Alpaca
        send_text(
            &mut socket,
            json!([
                { "T": "q", "S": "AAPL", "bp": 179.1, "bs": 1, "ap": 179.12, "as": 2, "t": "2024-03-01T15:30:00Z" },
                { "T": "t", "S": "AAPL", "p": 179.11, "s": 50, "t": "2024-03-01T15:30:00.5Z", "i": 1, "x": "V" }
            ]),
        )
        .await;
        send_text(
            &mut socket,
            json!([{ "T": "b", "S": "AAPL", "o": 179.0, "h": 179.2, "l": 178.9, "c": 179.1, "v": 1500, "t": "2024-03-01T15:30:00Z" }]),
        )
        .await;
        socket.close(None).await.unwrap();
    })
    .await;

    let events: Vec<_> = market_data(&base)
        .subscribe(&[Symbol::new("AAPL").unwrap()])
        .await
        .unwrap()
        .collect()
        .await;

    assert_eq!(events.len(), 3);
    let MarketEvent::Quote(quote) = &events[0] else {
        panic!("se esperaba una cotización");
    };
    assert_eq!(quote.ask().unwrap().value(), dec!(179.12));
    assert!(
        matches!(&events[1], MarketEvent::Trade(trade) if trade.price().value() == dec!(179.11))
    );
    assert!(matches!(&events[2], MarketEvent::Bar(bar) if bar.close().value() == dec!(179.1)));
}

#[tokio::test]
async fn market_stream_reports_the_connection_limit() {
    let base = scripted_server(|mut socket| async move {
        send_text(&mut socket, json!([{ "T": "success", "msg": "connected" }])).await;
        receive(&mut socket).await;
        send_text(
            &mut socket,
            json!([{ "T": "error", "code": 406, "msg": "connection limit exceeded" }]),
        )
        .await;
    })
    .await;

    let result = market_data(&base)
        .subscribe(&[Symbol::new("AAPL").unwrap()])
        .await;
    assert!(matches!(
        result,
        Err(MarketDataError::Unavailable(message)) if message.contains("una sola conexión")
    ));
}

#[tokio::test]
async fn market_stream_with_wrong_keys_is_unauthorized() {
    let base = scripted_server(|mut socket| async move {
        send_text(&mut socket, json!([{ "T": "success", "msg": "connected" }])).await;
        receive(&mut socket).await;
        send_text(
            &mut socket,
            json!([{ "T": "error", "code": 402, "msg": "auth failed" }]),
        )
        .await;
    })
    .await;

    let result = market_data(&base)
        .subscribe(&[Symbol::new("AAPL").unwrap()])
        .await;
    assert_eq!(result.err(), Some(MarketDataError::Unauthorized));
}

#[tokio::test]
async fn subscribing_to_nothing_opens_no_connection() {
    // Nadie escucha en esta URL: si intentara conectar, fallaría
    let events: Vec<_> = market_data("ws://127.0.0.1:9")
        .subscribe(&[])
        .await
        .unwrap()
        .collect()
        .await;
    assert!(events.is_empty());
}
