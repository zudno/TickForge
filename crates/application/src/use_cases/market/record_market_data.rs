use std::sync::Arc;

use chrono::{DateTime, TimeDelta, Utc};
use tickforge_domain::market::{Bar, Timeframe};
use tickforge_domain::shared::Symbol;

use crate::error::ApplicationError;
use crate::ports::{BarRepository, Clock, MarketDataFeed};

/// Resultado de descargar el histórico de un símbolo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolBackfill {
    pub symbol: Symbol,
    /// Velas guardadas, o el motivo por el que falló este símbolo (los demás siguen).
    pub result: Result<usize, String>,
}

/// Resultado de una descarga de histórico, símbolo por símbolo.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BackfillReport {
    pub symbols: Vec<SymbolBackfill>,
}

impl BackfillReport {
    /// Total de velas guardadas entre todos los símbolos.
    pub fn saved(&self) -> usize {
        self.symbols
            .iter()
            .filter_map(|symbol| symbol.result.as_ref().ok())
            .sum()
    }

    pub fn failed(&self) -> usize {
        self.symbols
            .iter()
            .filter(|symbol| symbol.result.is_err())
            .count()
    }
}

/// Caso de uso: grabar el histórico de velas para entrenar modelos.
///
/// Dos fuentes alimentan el mismo almacén: las velas que llegan por el flujo en vivo mientras
/// la TUI está abierta (`record`) y la descarga del histórico del proveedor (`backfill`), que
/// rellena lo que faltó mientras la aplicación estuvo cerrada.
pub struct RecordMarketDataUseCase {
    market_data: Arc<dyn MarketDataFeed>,
    bars: Arc<dyn BarRepository>,
    clock: Arc<dyn Clock>,
}

impl RecordMarketDataUseCase {
    pub fn new(
        market_data: Arc<dyn MarketDataFeed>,
        bars: Arc<dyn BarRepository>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            market_data,
            bars,
            clock,
        }
    }

    /// Guarda una vela recibida en vivo.
    pub async fn record(&self, bar: &Bar) -> Result<(), ApplicationError> {
        Ok(self.bars.save_bars(std::slice::from_ref(bar)).await?)
    }

    /// Descarga y guarda las velas de los últimos `lookback` de cada símbolo, empezando por
    /// la última vela ya guardada (es incremental: repetirlo solo baja lo nuevo). La última se
    /// vuelve a pedir porque pudo guardarse incompleta (con el mercado abierto); como guardar
    /// reemplaza, queda corregida sin duplicarse.
    /// Los símbolos se procesan uno tras otro para respetar el límite de peticiones.
    pub async fn backfill(
        &self,
        symbols: &[Symbol],
        timeframe: Timeframe,
        lookback: TimeDelta,
    ) -> BackfillReport {
        let now = self.clock.now();
        let earliest = now.checked_sub_signed(lookback).unwrap_or(now);
        let mut report = BackfillReport::default();
        for symbol in symbols {
            let result = self
                .backfill_one(symbol, timeframe, earliest, now)
                .await
                .map_err(|error| error.to_string());
            report.symbols.push(SymbolBackfill {
                symbol: symbol.clone(),
                result,
            });
        }
        report
    }

    async fn backfill_one(
        &self,
        symbol: &Symbol,
        timeframe: Timeframe,
        earliest: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<usize, ApplicationError> {
        let start = match self.bars.latest_bar_start(symbol, timeframe).await? {
            Some(latest) => latest.max(earliest),
            None => earliest,
        };
        if start >= now {
            return Ok(0);
        }
        let bars = self
            .market_data
            .bars_between(symbol, timeframe, start, now)
            .await?;
        self.bars.save_bars(&bars).await?;
        Ok(bars.len())
    }
}
