use async_trait::async_trait;
use rusqlite::params;
use tickforge_application::ports::{RepositoryError, WatchlistRepository};
use tickforge_domain::shared::Symbol;

use crate::store::SqliteStore;

/// Watchlist guardada en SQLite, en el orden en que se muestra.
#[derive(Debug, Clone)]
pub struct SqliteWatchlistRepository {
    store: SqliteStore,
}

impl SqliteWatchlistRepository {
    pub fn new(store: SqliteStore) -> Self {
        Self { store }
    }
}

#[async_trait]
impl WatchlistRepository for SqliteWatchlistRepository {
    async fn load(&self) -> Result<Option<Vec<Symbol>>, RepositoryError> {
        let raw_symbols = self
            .store
            .with_connection(|connection| {
                let mut statement =
                    connection.prepare("SELECT symbol FROM watchlist ORDER BY position")?;
                let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
                rows.collect::<Result<Vec<_>, _>>()
            })
            .await
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;

        if raw_symbols.is_empty() {
            return Ok(None);
        }
        raw_symbols
            .iter()
            .map(|raw| {
                Symbol::new(raw).map_err(|error| RepositoryError::Corrupted(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some)
    }

    /// Reemplaza la lista completa en una transacción: o se guarda toda, o nada.
    async fn save(&self, symbols: &[Symbol]) -> Result<(), RepositoryError> {
        let symbols: Vec<String> = symbols.iter().map(|s| s.as_str().to_owned()).collect();
        self.store
            .with_connection(move |connection| {
                let transaction = connection.transaction()?;
                transaction.execute("DELETE FROM watchlist", [])?;
                for (position, symbol) in symbols.iter().enumerate() {
                    let position = i64::try_from(position).unwrap_or(i64::MAX);
                    transaction.execute(
                        "INSERT INTO watchlist (position, symbol) VALUES (?1, ?2)",
                        params![position, symbol],
                    )?;
                }
                transaction.commit()
            })
            .await
            .map_err(|error| RepositoryError::Storage(error.to_string()))
    }
}
