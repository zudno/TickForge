use std::sync::Arc;

use tickforge_domain::shared::Symbol;

use crate::error::ApplicationError;
use crate::ports::{MarketDataError, MarketDataFeed, WatchlistRepository};

/// Caso de uso: cargar, ampliar y reducir la watchlist guardada.
pub struct ManageWatchlistUseCase {
    repository: Arc<dyn WatchlistRepository>,
    market_data: Arc<dyn MarketDataFeed>,
}

impl ManageWatchlistUseCase {
    /// Máximo de símbolos. El plan gratuito de Alpaca limita los símbolos por conexión de
    /// datos en tiempo real; se usa un tope conservador.
    pub const MAX_SYMBOLS: usize = 30;

    pub fn new(
        repository: Arc<dyn WatchlistRepository>,
        market_data: Arc<dyn MarketDataFeed>,
    ) -> Self {
        Self {
            repository,
            market_data,
        }
    }

    /// Watchlist guardada o, en el primer arranque, `seed` (que se guarda).
    pub async fn load_or_seed(&self, seed: Vec<Symbol>) -> Result<Vec<Symbol>, ApplicationError> {
        match self.repository.load().await? {
            Some(symbols) if !symbols.is_empty() => Ok(symbols),
            _ => {
                self.repository.save(&seed).await?;
                Ok(seed)
            }
        }
    }

    /// Agrega `symbol` al final, verificando que exista en el proveedor de datos.
    pub async fn add(
        &self,
        current: &[Symbol],
        symbol: Symbol,
    ) -> Result<Vec<Symbol>, ApplicationError> {
        if current.contains(&symbol) {
            return Err(ApplicationError::InvalidWatchlist(format!(
                "{symbol} ya está en la watchlist"
            )));
        }
        if current.len() >= Self::MAX_SYMBOLS {
            return Err(ApplicationError::InvalidWatchlist(format!(
                "la watchlist admite como máximo {} símbolos",
                Self::MAX_SYMBOLS
            )));
        }
        // Un símbolo sin cotización suele ser un error de escritura (o un instrumento que el
        // feed no cubre): se rechaza antes de guardarlo
        match self.market_data.latest_quote(&symbol).await {
            Ok(_) => {}
            Err(MarketDataError::NoData(_)) => {
                return Err(ApplicationError::InvalidWatchlist(format!(
                    "no hay datos de mercado para {symbol}; revisa que el símbolo exista"
                )));
            }
            Err(other) => return Err(other.into()),
        }

        let mut symbols = current.to_vec();
        symbols.push(symbol);
        self.repository.save(&symbols).await?;
        Ok(symbols)
    }

    /// Quita `symbol`. La watchlist nunca queda vacía.
    pub async fn remove(
        &self,
        current: &[Symbol],
        symbol: &Symbol,
    ) -> Result<Vec<Symbol>, ApplicationError> {
        if !current.contains(symbol) {
            return Err(ApplicationError::InvalidWatchlist(format!(
                "{symbol} no está en la watchlist"
            )));
        }
        if current.len() == 1 {
            return Err(ApplicationError::InvalidWatchlist(
                "la watchlist debe tener al menos un símbolo".to_owned(),
            ));
        }
        let symbols: Vec<Symbol> = current
            .iter()
            .filter(|candidate| *candidate != symbol)
            .cloned()
            .collect();
        self.repository.save(&symbols).await?;
        Ok(symbols)
    }
}
