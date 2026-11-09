use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, SecondsFormat, TimeDelta, Utc};
use futures::stream::{self, BoxStream};
use futures::{StreamExt, future};
use reqwest::StatusCode;
use secrecy::ExposeSecret;
use serde_json::json;
use tickforge_application::ports::{Clock, MarketDataError, MarketDataFeed, MarketEvent};
use tickforge_domain::market::{Bar, Quote, Timeframe};
use tickforge_domain::shared::Symbol;

use crate::config::{AlpacaConfig, AlpacaCredentials, DataFeed};
use crate::error::AlpacaSetupError;
use crate::http::{AlpacaHttp, HttpError};
use crate::mapping::{self, DataFrameItem, MappingError};
use crate::websocket::{self, Socket, StreamError};
use crate::wire::{BarsResponseWire, LatestQuoteResponseWire};

/// Máximo de velas que la API devuelve en una sola página.
const MAX_BARS_PER_PAGE: usize = 10_000;

/// Mensajes que se toleran durante el saludo antes de considerarlo fallido.
const MAX_HANDSHAKE_MESSAGES: usize = 20;

/// Implementación de `MarketDataFeed` sobre la Market Data API v2 de Alpaca.
pub struct AlpacaMarketData {
    http: AlpacaHttp,
    feed: DataFeed,
    clock: Arc<dyn Clock>,
    credentials: AlpacaCredentials,
    stream_url: String,
}

/// Confirmación que se espera en cada paso del saludo del WebSocket de datos.
#[derive(Debug, Clone, Copy)]
enum Expected {
    Success(&'static str),
    Subscription,
}

impl AlpacaMarketData {
    /// El reloj se usa para calcular la ventana de tiempo al pedir velas históricas.
    pub fn new(config: &AlpacaConfig, clock: Arc<dyn Clock>) -> Result<Self, AlpacaSetupError> {
        Ok(Self {
            http: AlpacaHttp::new(
                &config.data_base_url,
                &config.credentials,
                config.request_timeout,
            )?,
            feed: config.data_feed,
            clock,
            credentials: config.credentials.duplicate(),
            stream_url: config.data_stream_url(),
        })
    }

    /// Conecta, se autentica y se suscribe a cotizaciones, operaciones y velas de 1 minuto.
    async fn open_stream(&self, symbols: &[Symbol]) -> Result<Socket, StreamError> {
        let mut socket = websocket::connect(&self.stream_url).await?;
        wait_for(&mut socket, Expected::Success("connected")).await?;

        websocket::send_json(
            &mut socket,
            &json!({
                "action": "auth",
                "key": self.credentials.key_id.expose_secret(),
                "secret": self.credentials.secret_key.expose_secret(),
            }),
        )
        .await?;
        wait_for(&mut socket, Expected::Success("authenticated")).await?;

        let tickers: Vec<&str> = symbols.iter().map(Symbol::as_str).collect();
        websocket::send_json(
            &mut socket,
            &json!({
                "action": "subscribe",
                "quotes": tickers,
                "trades": tickers,
                "bars": tickers,
            }),
        )
        .await?;
        wait_for(&mut socket, Expected::Subscription).await?;

        Ok(socket)
    }
}

#[async_trait]
impl MarketDataFeed for AlpacaMarketData {
    async fn latest_quote(&self, symbol: &Symbol) -> Result<Quote, MarketDataError> {
        let path = format!("/v2/stocks/{symbol}/quotes/latest");
        let wire: LatestQuoteResponseWire = self
            .http
            .get(&path, &[("feed", self.feed.as_str())])
            .await
            .map_err(|error| market_data_error(error, symbol))?;
        mapping::quote(symbol, wire).map_err(invalid_response)
    }

    async fn bars(
        &self,
        symbol: &Symbol,
        timeframe: Timeframe,
        limit: usize,
    ) -> Result<Vec<Bar>, MarketDataError> {
        let limit = limit.clamp(1, MAX_BARS_PER_PAGE);
        let start = lookback_start(self.clock.now(), timeframe, limit);
        let start = start.to_rfc3339_opts(SecondsFormat::Secs, true);
        let limit_param = limit.to_string();

        // Orden descendente para obtener las `limit` velas más recientes en una sola página
        let path = format!("/v2/stocks/{symbol}/bars");
        let wire: BarsResponseWire = self
            .http
            .get(
                &path,
                &[
                    ("timeframe", mapping::timeframe_param(timeframe)),
                    ("limit", &limit_param),
                    ("start", &start),
                    ("sort", "desc"),
                    ("adjustment", "raw"),
                    ("feed", self.feed.as_str()),
                ],
            )
            .await
            .map_err(|error| market_data_error(error, symbol))?;

        let mut bars = wire
            .bars
            .unwrap_or_default()
            .into_iter()
            .map(|bar| mapping::bar(symbol, timeframe, bar).map_err(invalid_response))
            .collect::<Result<Vec<_>, _>>()?;
        // El puerto promete orden cronológico ascendente
        bars.reverse();
        Ok(bars)
    }

    /// Flujo en tiempo real. El plan gratuito de Alpaca admite **una sola conexión** de datos:
    /// conviene suscribir todos los símbolos en una llamada. El flujo termina si se cierra la
    /// conexión; reconectar es responsabilidad de quien lo consume.
    async fn subscribe(
        &self,
        symbols: &[Symbol],
    ) -> Result<BoxStream<'static, MarketEvent>, MarketDataError> {
        if symbols.is_empty() {
            return Ok(stream::empty().boxed());
        }
        let socket = self
            .open_stream(symbols)
            .await
            .map_err(|error| stream_error(error, &symbols[0]))?;

        let events = socket
            .take_while(|message| future::ready(message.is_ok()))
            .filter_map(|message| future::ready(message.ok().and_then(websocket::payload)))
            .flat_map(|payload| {
                let events =
                    mapping::data_frame(&payload)
                        .into_iter()
                        .filter_map(|item| match item {
                            DataFrameItem::Event(event) => Some(event),
                            _ => None,
                        });
                stream::iter(events)
            })
            .boxed();
        Ok(events)
    }
}

/// Lee mensajes del saludo hasta recibir la confirmación esperada.
async fn wait_for(socket: &mut Socket, expected: Expected) -> Result<(), StreamError> {
    for _ in 0..MAX_HANDSHAKE_MESSAGES {
        let payload = websocket::next_payload(socket).await?;
        for item in mapping::data_frame(&payload) {
            match (item, expected) {
                (DataFrameItem::Error { code, message }, _) => {
                    return Err(classify_stream_error(code, message));
                }
                (DataFrameItem::Success(message), Expected::Success(wanted))
                    if message == wanted =>
                {
                    return Ok(());
                }
                (DataFrameItem::Subscribed, Expected::Subscription) => return Ok(()),
                _ => {}
            }
        }
    }
    Err(StreamError::Protocol(format!(
        "no llegó la confirmación {expected:?} del WebSocket de datos"
    )))
}

/// Códigos de error documentados del WebSocket de datos de Alpaca.
fn classify_stream_error(code: i64, message: String) -> StreamError {
    match code {
        401 | 402 => StreamError::Unauthorized,
        406 => StreamError::ConnectionLimit(format!(
            "{message}: el plan gratuito permite una sola conexión de datos a la vez"
        )),
        409 => {
            StreamError::NotEntitled(format!("{message}: el plan no incluye el feed solicitado"))
        }
        _ => StreamError::Protocol(format!("error {code}: {message}")),
    }
}

fn stream_error(error: StreamError, symbol: &Symbol) -> MarketDataError {
    match error {
        StreamError::Transport(message)
        | StreamError::ConnectionLimit(message)
        | StreamError::NotEntitled(message) => MarketDataError::Unavailable(message),
        StreamError::Timeout => {
            MarketDataError::Unavailable("el WebSocket de datos no respondió a tiempo".to_owned())
        }
        StreamError::Closed => {
            MarketDataError::Unavailable("el WebSocket de datos cerró la conexión".to_owned())
        }
        StreamError::Unauthorized => MarketDataError::Unauthorized,
        StreamError::Protocol(message) => {
            MarketDataError::InvalidResponse(format!("{symbol}: {message}"))
        }
    }
}

/// Inicio de la ventana de búsqueda: holgura suficiente para cubrir noches, fines de semana
/// y feriados. Con orden descendente y `limit`, solo se devuelven las velas más recientes.
fn lookback_start(now: DateTime<Utc>, timeframe: Timeframe, limit: usize) -> DateTime<Utc> {
    let fallback = TimeDelta::days(3_650);
    let periods = i32::try_from(limit).unwrap_or(i32::MAX);
    let lookback = timeframe
        .duration()
        .checked_mul(periods)
        .and_then(|span| span.checked_mul(3))
        .and_then(|span| span.checked_add(&TimeDelta::days(7)))
        .unwrap_or(fallback);
    now.checked_sub_signed(lookback)
        .unwrap_or(DateTime::<Utc>::MIN_UTC)
}

fn market_data_error(error: HttpError, symbol: &Symbol) -> MarketDataError {
    match error {
        HttpError::Transport(message) => MarketDataError::Unavailable(message),
        HttpError::Decode(message) => MarketDataError::InvalidResponse(message),
        HttpError::Status { status: 401, .. } | HttpError::Status { status: 403, .. } => {
            MarketDataError::Unauthorized
        }
        HttpError::Status { status: 429, .. } => MarketDataError::RateLimited,
        HttpError::Status { status, .. } if status == StatusCode::NOT_FOUND.as_u16() => {
            MarketDataError::NoData(symbol.clone())
        }
        HttpError::Status { status, message } if status >= 500 => {
            MarketDataError::Unavailable(format!("HTTP {status}: {message}"))
        }
        HttpError::Status { status, message } => {
            MarketDataError::InvalidResponse(format!("HTTP {status}: {message}"))
        }
    }
}

fn invalid_response(error: MappingError) -> MarketDataError {
    MarketDataError::InvalidResponse(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookback_covers_weekends_for_small_requests() {
        let now = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let start = lookback_start(now, Timeframe::Minute1, 10);
        assert!(now.signed_duration_since(start) >= TimeDelta::days(7));
    }

    #[test]
    fn lookback_never_panics_on_huge_requests() {
        let now = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let start = lookback_start(now, Timeframe::Day1, usize::MAX);
        assert!(start < now);
    }
}
