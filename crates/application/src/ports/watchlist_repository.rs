use async_trait::async_trait;
use thiserror::Error;
use tickforge_domain::shared::Symbol;

/// Persistencia de la watchlist del usuario (los símbolos y su orden).
#[async_trait]
pub trait WatchlistRepository: Send + Sync {
    /// Watchlist guardada; `None` si nunca se guardó una (primer arranque).
    async fn load(&self) -> Result<Option<Vec<Symbol>>, RepositoryError>;

    /// Reemplaza la watchlist guardada por `symbols`, en ese orden.
    async fn save(&self, symbols: &[Symbol]) -> Result<(), RepositoryError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepositoryError {
    #[error("no se pudo acceder al almacenamiento local: {0}")]
    Storage(String),

    #[error("el almacenamiento local contiene datos inválidos: {0}")]
    Corrupted(String),
}
