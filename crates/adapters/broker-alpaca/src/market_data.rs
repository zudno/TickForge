use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, SecondsFormat, TimeDelta, Utc};
use futures::stream::BoxStream;
use reqwest::StatusCode;
use tickforge_application::ports::{Clock, MarketDataError, MarketDataFeed, MarketEvent};
use tickforge_domain::market::{Bar, Quote, Timeframe};
use tickforge_domain::shared::Symbol;

use crate::config::{AlpacaConfig, DataFeed};
use crate::error::AlpacaSetupError;
use crate::http::{AlpacaHttp, HttpError};
use crate::mapping::{self, MappingError};
use crate::wire::{BarsResponseWire, LatestQuoteResponseWire};

/// Máximo de velas que la API devuelve en una sola página.
const MAX_BARS_PER_PAGE: usize = 10_000;

/// Implementación de `MarketDataFeed` sobre la Market Data API v2 de Alpaca.
pub struct AlpacaMarketData {
    http: AlpacaHttp,
    feed: DataFeed,
    clock: Arc<dyn Clock>,
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
        })
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

    async fn subscribe(
        &self,
        _symbols: &[Symbol],
    ) -> Result<BoxStream<'static, MarketEvent>, MarketDataError> {
        // El streaming por WebSocket se implementa en el siguiente incremento
        Err(MarketDataError::Unavailable(
            "el flujo de datos en tiempo real (WebSocket) aún no está implementado".to_owned(),
        ))
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
