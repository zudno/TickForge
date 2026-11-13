use async_trait::async_trait;
use futures::stream::BoxStream;
use futures::{StreamExt, future};
use reqwest::StatusCode;
use secrecy::ExposeSecret;
use serde_json::{Value, json};
use tickforge_application::ports::{BrokerError, BrokerGateway, OrderUpdate};
use tickforge_domain::market::MarketClock;
use tickforge_domain::portfolio::{Account, Position};
use tickforge_domain::shared::Symbol;
use tickforge_domain::trading::{ClientOrderId, Order, OrderId, OrderRequest};

use crate::config::{AlpacaConfig, AlpacaCredentials};
use crate::error::AlpacaSetupError;
use crate::http::{AlpacaHttp, HttpError, RequestPolicy, Retry};
use crate::mapping::{self, MappingError};
use crate::websocket::{self, Socket, StreamError};
use crate::wire::{
    AccountWire, ClockWire, OrderWire, PositionWire, TradeUpdateWire, TradingStreamMessageWire,
};

/// Mensaje con el que Alpaca rechaza un `client_order_id` repetido.
const DUPLICATE_CLIENT_ORDER_ID: &str = "client_order_id must be unique";

/// Límite máximo de órdenes por consulta que acepta la API.
const MAX_ORDERS_PER_PAGE: &str = "500";

/// Mensajes que se toleran durante el saludo antes de considerarlo fallido.
const MAX_HANDSHAKE_MESSAGES: usize = 20;

/// Implementación de `BrokerGateway` sobre la Trading API v2 de Alpaca.
pub struct AlpacaBrokerGateway {
    http: AlpacaHttp,
    credentials: AlpacaCredentials,
    stream_url: String,
}

impl AlpacaBrokerGateway {
    pub fn new(config: &AlpacaConfig) -> Result<Self, AlpacaSetupError> {
        config.validate()?;
        Ok(Self {
            http: AlpacaHttp::new(
                &config.trading_base_url,
                &config.credentials,
                config.request_timeout,
                RequestPolicy {
                    max_retries: config.max_retries,
                    requests_per_minute: config.requests_per_minute,
                },
            )?,
            credentials: config.credentials.duplicate(),
            stream_url: config.trading_stream_url.clone(),
        })
    }

    /// Conecta al WebSocket de trading, se autentica y se suscribe a `trade_updates`.
    async fn open_trade_updates(&self) -> Result<Socket, StreamError> {
        let mut socket = websocket::connect(&self.stream_url).await?;

        websocket::send_json(
            &mut socket,
            &json!({
                "action": "auth",
                "key": self.credentials.key_id.expose_secret(),
                "secret": self.credentials.secret_key.expose_secret(),
            }),
        )
        .await?;
        let authorization = wait_for_stream(&mut socket, "authorization").await?;
        if authorization.get("status").and_then(Value::as_str) != Some("authorized") {
            return Err(StreamError::Unauthorized);
        }

        websocket::send_json(
            &mut socket,
            &json!({ "action": "listen", "data": { "streams": ["trade_updates"] } }),
        )
        .await?;
        let listening = wait_for_stream(&mut socket, "listening").await?;
        let subscribed = listening
            .get("streams")
            .and_then(Value::as_array)
            .is_some_and(|streams| streams.iter().any(|s| s == "trade_updates"));
        if !subscribed {
            return Err(StreamError::Protocol(
                "Alpaca no confirmó la suscripción a trade_updates".to_owned(),
            ));
        }

        Ok(socket)
    }

    /// Recupera la orden ya existente tras un `client_order_id` duplicado.
    async fn existing_order(&self, request: &OrderRequest) -> Result<Order, BrokerError> {
        self.order_by_client_order_id(request.client_order_id())
            .await?
            .ok_or_else(|| {
                BrokerError::InvalidResponse(
                    "Alpaca reportó un client_order_id duplicado pero no encontró la orden"
                        .to_owned(),
                )
            })
    }
}

#[async_trait]
impl BrokerGateway for AlpacaBrokerGateway {
    async fn account(&self) -> Result<Account, BrokerError> {
        let wire: AccountWire = self
            .http
            .get("/v2/account", &[])
            .await
            .map_err(broker_error)?;
        mapping::account(wire).map_err(invalid_response)
    }

    async fn positions(&self) -> Result<Vec<Position>, BrokerError> {
        let wires: Vec<PositionWire> = self
            .http
            .get("/v2/positions", &[])
            .await
            .map_err(broker_error)?;
        wires
            .into_iter()
            .map(|wire| mapping::position(wire).map_err(invalid_response))
            .collect()
    }

    async fn position(&self, symbol: &Symbol) -> Result<Option<Position>, BrokerError> {
        let path = format!("/v2/positions/{symbol}");
        match self.http.get::<PositionWire>(&path, &[]).await {
            Ok(wire) => mapping::position(wire).map(Some).map_err(invalid_response),
            // Alpaca responde 404 cuando no hay posición abierta en el símbolo
            Err(error) if error.is_status(StatusCode::NOT_FOUND) => Ok(None),
            Err(error) => Err(broker_error(error)),
        }
    }

    async fn open_orders(&self) -> Result<Vec<Order>, BrokerError> {
        let wires: Vec<OrderWire> = self
            .http
            .get(
                "/v2/orders",
                &[("status", "open"), ("limit", MAX_ORDERS_PER_PAGE)],
            )
            .await
            .map_err(broker_error)?;
        wires
            .into_iter()
            .map(|wire| mapping::order(wire).map_err(invalid_response))
            .collect()
    }

    async fn order(&self, order_id: &OrderId) -> Result<Option<Order>, BrokerError> {
        let path = format!("/v2/orders/{order_id}");
        match self.http.get::<OrderWire>(&path, &[]).await {
            Ok(wire) => mapping::order(wire).map(Some).map_err(invalid_response),
            Err(error) if error.is_status(StatusCode::NOT_FOUND) => Ok(None),
            Err(error) => Err(broker_error(error)),
        }
    }

    async fn order_by_client_order_id(
        &self,
        client_order_id: &ClientOrderId,
    ) -> Result<Option<Order>, BrokerError> {
        let result = self
            .http
            .get::<OrderWire>(
                "/v2/orders:by_client_order_id",
                &[("client_order_id", client_order_id.as_str())],
            )
            .await;
        match result {
            Ok(wire) => mapping::order(wire).map(Some).map_err(invalid_response),
            // 404: la orden nunca llegó a Alpaca
            Err(error) if error.is_status(StatusCode::NOT_FOUND) => Ok(None),
            Err(error) => Err(broker_error(error)),
        }
    }

    async fn submit_order(&self, request: &OrderRequest) -> Result<Order, BrokerError> {
        let body = mapping::new_order_body(request);
        // Reintentar es seguro: el `client_order_id` hace que Alpaca no duplique la orden
        match self
            .http
            .post::<_, OrderWire>("/v2/orders", &body, Retry::Allowed)
            .await
        {
            Ok(wire) => mapping::order(wire).map_err(invalid_response),
            // Reintento de un envío que sí llegó: se devuelve la orden ya existente
            Err(HttpError::Status { status, message })
                if status == StatusCode::UNPROCESSABLE_ENTITY.as_u16()
                    && message.contains(DUPLICATE_CLIENT_ORDER_ID) =>
            {
                self.existing_order(request).await
            }
            Err(error) => Err(broker_error(error)),
        }
    }

    async fn cancel_order(&self, order_id: &OrderId) -> Result<(), BrokerError> {
        let path = format!("/v2/orders/{order_id}");
        match self.http.delete(&path).await {
            Ok(()) => Ok(()),
            Err(error) if error.is_status(StatusCode::NOT_FOUND) => {
                Err(BrokerError::OrderNotFound(order_id.clone()))
            }
            Err(error) => Err(broker_error(error)),
        }
    }

    /// Flujo de `trade_updates`. Termina si se cierra la conexión; reconectar (y reconciliar
    /// por REST lo ocurrido mientras tanto) es responsabilidad de quien lo consume.
    async fn order_updates(&self) -> Result<BoxStream<'static, OrderUpdate>, BrokerError> {
        let socket = self.open_trade_updates().await.map_err(stream_error)?;

        let updates = socket
            .take_while(|message| future::ready(message.is_ok()))
            .filter_map(|message| future::ready(message.ok().and_then(websocket::payload)))
            .filter_map(|payload| future::ready(parse_trade_update(&payload)))
            .boxed();
        Ok(updates)
    }

    async fn market_clock(&self) -> Result<MarketClock, BrokerError> {
        let wire: ClockWire = self
            .http
            .get("/v2/clock", &[])
            .await
            .map_err(broker_error)?;
        mapping::clock(wire).map_err(invalid_response)
    }
}

/// Traduce un error HTTP al error del puerto de ejecución.
fn broker_error(error: HttpError) -> BrokerError {
    match error {
        HttpError::Transport(message) => BrokerError::Unavailable(message),
        HttpError::Decode(message) => BrokerError::InvalidResponse(message),
        HttpError::Status { status: 401, .. } => BrokerError::Unauthorized,
        HttpError::Status { status: 429, .. } => BrokerError::RateLimited,
        HttpError::Status { status, message } if status >= 500 => {
            BrokerError::Unavailable(format!("HTTP {status}: {message}"))
        }
        // 403 (poder de compra insuficiente, PDT), 422 (orden inválida), etc.
        HttpError::Status { message, .. } => BrokerError::Rejected { reason: message },
    }
}

fn invalid_response(error: MappingError) -> BrokerError {
    BrokerError::InvalidResponse(error.to_string())
}

fn stream_error(error: StreamError) -> BrokerError {
    match error {
        StreamError::Transport(message) => BrokerError::Unavailable(message),
        StreamError::Timeout => {
            BrokerError::Unavailable("el WebSocket de trading no respondió a tiempo".to_owned())
        }
        StreamError::Closed => {
            BrokerError::Unavailable("el WebSocket de trading cerró la conexión".to_owned())
        }
        StreamError::Unauthorized => BrokerError::Unauthorized,
        StreamError::ConnectionLimit(message) | StreamError::NotEntitled(message) => {
            BrokerError::Rejected { reason: message }
        }
        StreamError::Protocol(message) => BrokerError::InvalidResponse(message),
    }
}

/// Lee mensajes del saludo hasta encontrar el del `stream` indicado y devuelve su `data`.
async fn wait_for_stream(socket: &mut Socket, stream: &str) -> Result<Value, StreamError> {
    for _ in 0..MAX_HANDSHAKE_MESSAGES {
        let payload = websocket::next_payload(socket).await?;
        let message: TradingStreamMessageWire = serde_json::from_slice(&payload)
            .map_err(|error| StreamError::Protocol(error.to_string()))?;
        if message.stream == stream {
            return Ok(message.data);
        }
    }
    Err(StreamError::Protocol(format!(
        "no llegó la respuesta '{stream}' del WebSocket de trading"
    )))
}

/// Mensaje del stream ya autenticado. Lo que no sea un `trade_updates` válido se descarta.
fn parse_trade_update(payload: &[u8]) -> Option<OrderUpdate> {
    let message: TradingStreamMessageWire = serde_json::from_slice(payload).ok()?;
    if message.stream != "trade_updates" {
        return None;
    }
    let wire: TradeUpdateWire = serde_json::from_value(message.data).ok()?;
    mapping::trade_update(wire).ok().flatten()
}
