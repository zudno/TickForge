use async_trait::async_trait;
use futures::stream::BoxStream;
use reqwest::StatusCode;
use tickforge_application::ports::{BrokerError, BrokerGateway, OrderUpdate};
use tickforge_domain::market::MarketClock;
use tickforge_domain::portfolio::{Account, Position};
use tickforge_domain::shared::Symbol;
use tickforge_domain::trading::{Order, OrderId, OrderRequest};

use crate::config::AlpacaConfig;
use crate::error::AlpacaSetupError;
use crate::http::{AlpacaHttp, HttpError};
use crate::mapping::{self, MappingError};
use crate::wire::{AccountWire, ClockWire, OrderWire, PositionWire};

/// Mensaje con el que Alpaca rechaza un `client_order_id` repetido.
const DUPLICATE_CLIENT_ORDER_ID: &str = "client_order_id must be unique";

/// Límite máximo de órdenes por consulta que acepta la API.
const MAX_ORDERS_PER_PAGE: &str = "500";

/// Implementación de `BrokerGateway` sobre la Trading API v2 de Alpaca.
pub struct AlpacaBrokerGateway {
    http: AlpacaHttp,
}

impl AlpacaBrokerGateway {
    pub fn new(config: &AlpacaConfig) -> Result<Self, AlpacaSetupError> {
        Ok(Self {
            http: AlpacaHttp::new(
                &config.trading_base_url,
                &config.credentials,
                config.request_timeout,
            )?,
        })
    }

    async fn order_by_client_id(&self, request: &OrderRequest) -> Result<Order, BrokerError> {
        let wire: OrderWire = self
            .http
            .get(
                "/v2/orders:by_client_order_id",
                &[("client_order_id", request.client_order_id().as_str())],
            )
            .await
            .map_err(broker_error)?;
        mapping::order(wire).map_err(invalid_response)
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

    async fn submit_order(&self, request: &OrderRequest) -> Result<Order, BrokerError> {
        let body = mapping::new_order_body(request);
        match self.http.post::<_, OrderWire>("/v2/orders", &body).await {
            Ok(wire) => mapping::order(wire).map_err(invalid_response),
            // Reintento de un envío que sí llegó: se devuelve la orden ya existente
            Err(HttpError::Status { status, message })
                if status == StatusCode::UNPROCESSABLE_ENTITY.as_u16()
                    && message.contains(DUPLICATE_CLIENT_ORDER_ID) =>
            {
                self.order_by_client_id(request).await
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

    async fn order_updates(&self) -> Result<BoxStream<'static, OrderUpdate>, BrokerError> {
        // El flujo `trade_updates` por WebSocket se implementa en el siguiente incremento
        Err(BrokerError::Unavailable(
            "el flujo de actualizaciones de órdenes (WebSocket) aún no está implementado"
                .to_owned(),
        ))
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
