use chrono::{DateTime, Utc};
use tickforge_application::ports::{MarketEvent, OrderUpdate};
use tickforge_application::use_cases::market::SymbolSnapshot;
use tickforge_application::use_cases::portfolio::PortfolioSnapshot;
use tickforge_application::use_cases::trading::{OrderPreview, ReconciliationReport};
use tickforge_domain::market::MarketClock;
use tickforge_domain::shared::Symbol;
use tickforge_domain::trading::{Order, OrderId, OrderSide};

/// Todo lo que puede cambiar el estado de la TUI: intenciones del usuario (teclado)
/// y resultados de tareas en segundo plano. Solo el reducer las interpreta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    // --- Usuario: navegación ---
    Quit,
    SelectNext,
    SelectPrevious,
    FocusNextPanel,
    ToggleHelp,
    CloseOverlay,
    Refresh,
    /// `Enter`: avanza el paso de la ventana activa (revisar, enviar, confirmar).
    Confirm,

    // --- Usuario: órdenes ---
    OpenTicket(OrderSide),
    TicketNextField,
    TicketPreviousField,
    TicketToggle,
    TicketInput(char),
    TicketBackspace,
    RequestCancelSelected,

    // --- Usuario: watchlist ---
    OpenAddSymbol,
    AddSymbolInput(char),
    AddSymbolBackspace,
    RemoveSelectedSymbol,

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

    // --- Resultados de órdenes ---
    OrderPreviewReady(OrderPreview),
    OrderPreviewFailed(String),
    OrderSubmitted(Order),
    OrderSubmitFailed(String),
    OrderCancelRequested(OrderId),
    OrderCancelFailed(String),
    OrderUpdateReceived(OrderUpdate),
    OrderStreamConnected,
    OrderStreamDisconnected(String),

    // --- Persistencia ---
    WatchlistChanged(Vec<Symbol>),
    WatchlistChangeFailed(String),
    ReconciliationFinished(ReconciliationReport),
    ReconciliationFailed(String),
}
