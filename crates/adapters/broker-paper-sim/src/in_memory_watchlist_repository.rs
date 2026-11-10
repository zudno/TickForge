use std::sync::Mutex;

use async_trait::async_trait;
use tickforge_application::ports::{RepositoryError, WatchlistRepository};
use tickforge_domain::shared::Symbol;

use crate::lock::lock;

/// Watchlist en memoria, para pruebas y demos.
#[derive(Debug, Default)]
pub struct InMemoryWatchlistRepository {
    symbols: Mutex<Option<Vec<Symbol>>>,
}

impl InMemoryWatchlistRepository {
    pub fn new() -> Self {
        Self::default()
    }

    /// Lo último que se guardó (`None` si nunca se guardó nada).
    pub fn saved(&self) -> Option<Vec<Symbol>> {
        lock(&self.symbols).clone()
    }
}

#[async_trait]
impl WatchlistRepository for InMemoryWatchlistRepository {
    async fn load(&self) -> Result<Option<Vec<Symbol>>, RepositoryError> {
        Ok(lock(&self.symbols).clone())
    }

    async fn save(&self, symbols: &[Symbol]) -> Result<(), RepositoryError> {
        *lock(&self.symbols) = Some(symbols.to_vec());
        Ok(())
    }
}
