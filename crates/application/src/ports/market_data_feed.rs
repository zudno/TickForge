use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::stream::BoxStream;
use thiserror::Error;
use tickforge_domain::market::{Bar, Quote, Timeframe, TradeTick};
use tickforge_domain::shared::Symbol;

/// Puerto de datos de mercado, independiente del broker de ejecución.
#[async_trait]
pub trait MarketDataFeed: Send + Sync {
    /// Cotización más reciente de un símbolo.
    async fn latest_quote(&self, symbol: &Symbol) -> Result<Quote, MarketDataError>;

    /// Últimas `limit` velas, de la más antigua a la más reciente.
    async fn bars(
        &self,
        symbol: &Symbol,
        timeframe: Timeframe,
        limit: usize,
    ) -> Result<Vec<Bar>, MarketDataError>;

    /// Todas las velas cuyo inicio está en `[start, end)`, de la más antigua a la más
    /// reciente. Recorre todas las páginas del proveedor: puede hacer varias peticiones.
    async fn bars_between(
        &self,
        symbol: &Symbol,
        timeframe: Timeframe,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Bar>, MarketDataError>;

    /// Flujo en tiempo real de cotizaciones, operaciones y velas de los símbolos indicados.
    async fn subscribe(
        &self,
        symbols: &[Symbol],
    ) -> Result<BoxStream<'static, MarketEvent>, MarketDataError>;
}

/// Evento de mercado en tiempo real.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarketEvent {
    Quote(Quote),
    Trade(TradeTick),
    Bar(Bar),
}

impl MarketEvent {
    pub fn symbol(&self) -> &Symbol {
        match self {
            Self::Quote(quote) => quote.symbol(),
            Self::Trade(trade) => trade.symbol(),
            Self::Bar(bar) => bar.symbol(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MarketDataError {
    #[error("el proveedor de datos no está disponible: {0}")]
    Unavailable(String),

    #[error("el proveedor de datos rechazó las credenciales")]
    Unauthorized,

    #[error("se alcanzó el límite de peticiones del proveedor de datos")]
    RateLimited,

    #[error("no hay datos de mercado para {0}")]
    NoData(Symbol),

    #[error("respuesta inválida del proveedor de datos: {0}")]
    InvalidResponse(String),
}
