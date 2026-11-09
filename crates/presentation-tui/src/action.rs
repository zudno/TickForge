use chrono::{DateTime, Utc};
use tickforge_application::ports::MarketEvent;
use tickforge_application::use_cases::market::SymbolSnapshot;
use tickforge_application::use_cases::portfolio::PortfolioSnapshot;
use tickforge_domain::market::MarketClock;

/// Todo lo que puede cambiar el estado de la TUI: intenciones del usuario (teclado)
/// y resultados de tareas en segundo plano. Solo el reducer las interpreta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    // --- Usuario ---
    Quit,
    SelectNext,
    SelectPrevious,
    ToggleHelp,
    CloseOverlay,
    Refresh,

    // --- Tiempo ---
    Tick(DateTime<Utc>),

    // --- Datos de mercado ---
    WatchlistSeeded(Vec<SymbolSnapshot>),
    Market(MarketEvent),
    StreamConnected,
    StreamDisconnected(String),

    // --- Cuenta y sesión ---
    PortfolioLoaded(PortfolioSnapshot),
    PortfolioFailed(String),
    MarketClockLoaded(MarketClock),
    MarketClockFailed(String),
}
