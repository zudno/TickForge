use chrono::{DateTime, Utc};
use tickforge_application::use_cases::portfolio::PortfolioSnapshot;
use tickforge_domain::market::MarketClock;
use tickforge_domain::shared::Symbol;

use crate::state::{Notifications, Watchlist};

/// Modo de operación. Se muestra siempre en la barra de estado para que nunca haya duda
/// de si se opera con dinero simulado o real.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradingMode {
    Paper,
    Live,
}

/// Estado de la conexión de datos en tiempo real.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamStatus {
    Connecting,
    Live,
    Disconnected(String),
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub mode: TradingMode,
    pub broker_name: String,
    /// Hora actual según el reloj inyectado; se actualiza con cada `Tick`.
    pub now: DateTime<Utc>,
    pub watchlist: Watchlist,
    pub portfolio: Option<PortfolioSnapshot>,
    pub market_clock: Option<MarketClock>,
    pub stream_status: StreamStatus,
    pub notifications: Notifications,
    pub show_help: bool,
    pub should_quit: bool,
    pub(crate) last_portfolio_request: Option<DateTime<Utc>>,
    pub(crate) last_clock_request: Option<DateTime<Utc>>,
}

impl AppState {
    pub fn new(
        mode: TradingMode,
        broker_name: impl Into<String>,
        symbols: Vec<Symbol>,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            mode,
            broker_name: broker_name.into(),
            now,
            watchlist: Watchlist::new(symbols),
            portfolio: None,
            market_clock: None,
            stream_status: StreamStatus::Connecting,
            notifications: Notifications::default(),
            show_help: false,
            should_quit: false,
            last_portfolio_request: None,
            last_clock_request: None,
        }
    }
}
