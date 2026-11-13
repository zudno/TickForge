use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::params;
use tickforge_application::ports::{BarRepository, RepositoryError};
use tickforge_domain::market::{Bar, Timeframe};
use tickforge_domain::shared::Symbol;

use crate::rows::timestamp;
use crate::store::SqliteStore;

/// Histórico de velas en la base de mercado (`market.db`).
#[derive(Debug, Clone)]
pub struct SqliteBarRepository {
    store: SqliteStore,
}

impl SqliteBarRepository {
    /// `store` debe abrirse con `SqliteStore::open_market_data`.
    pub fn new(store: SqliteStore) -> Self {
        Self { store }
    }
}

/// Fila a guardar, ya convertida a texto.
struct BarRow {
    symbol: String,
    timeframe: &'static str,
    starts_at: String,
    open: String,
    high: String,
    low: String,
    close: String,
    volume: String,
}

impl From<&Bar> for BarRow {
    fn from(bar: &Bar) -> Self {
        Self {
            symbol: bar.symbol().as_str().to_owned(),
            timeframe: bar.timeframe().as_str(),
            starts_at: timestamp(bar.starts_at()),
            open: bar.open().value().to_string(),
            high: bar.high().value().to_string(),
            low: bar.low().value().to_string(),
            close: bar.close().value().to_string(),
            volume: bar.volume().value().to_string(),
        }
    }
}

#[async_trait]
impl BarRepository for SqliteBarRepository {
    async fn save_bars(&self, bars: &[Bar]) -> Result<(), RepositoryError> {
        if bars.is_empty() {
            return Ok(());
        }
        let rows: Vec<BarRow> = bars.iter().map(BarRow::from).collect();
        self.store
            .with_connection(move |connection| {
                // Una sola transacción: miles de velas se guardan en milisegundos
                let transaction = connection.transaction()?;
                {
                    let mut statement = transaction.prepare(
                        "INSERT INTO bars (symbol, timeframe, starts_at, open, high, low, close, \
                         volume) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
                         ON CONFLICT (symbol, timeframe, starts_at) DO UPDATE SET \
                         open = excluded.open, high = excluded.high, low = excluded.low, \
                         close = excluded.close, volume = excluded.volume",
                    )?;
                    for row in &rows {
                        statement.execute(params![
                            row.symbol,
                            row.timeframe,
                            row.starts_at,
                            row.open,
                            row.high,
                            row.low,
                            row.close,
                            row.volume,
                        ])?;
                    }
                }
                transaction.commit()
            })
            .await
            .map_err(|error| RepositoryError::Storage(error.to_string()))
    }

    async fn latest_bar_start(
        &self,
        symbol: &Symbol,
        timeframe: Timeframe,
    ) -> Result<Option<DateTime<Utc>>, RepositoryError> {
        let symbol = symbol.as_str().to_owned();
        let timeframe = timeframe.as_str();
        let latest: Option<String> = self
            .store
            .with_connection(move |connection| {
                connection.query_row(
                    "SELECT MAX(starts_at) FROM bars WHERE symbol = ?1 AND timeframe = ?2",
                    params![symbol, timeframe],
                    |row| row.get(0),
                )
            })
            .await
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;

        latest
            .map(|raw| {
                DateTime::parse_from_rfc3339(&raw)
                    .map(|instant| instant.to_utc())
                    .map_err(|_| {
                        RepositoryError::Corrupted(format!("fecha de vela inválida: '{raw}'"))
                    })
            })
            .transpose()
    }
}
