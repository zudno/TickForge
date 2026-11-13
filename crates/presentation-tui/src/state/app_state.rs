use chrono::{DateTime, Utc};
use tickforge_application::use_cases::portfolio::PortfolioSnapshot;
use tickforge_domain::market::MarketClock;
use tickforge_domain::risk::KillSwitch;
use tickforge_domain::shared::Symbol;
use tickforge_domain::trading::Order;

use crate::state::{Notifications, Overlay, Watchlist};

/// Modo de operación. Se muestra siempre en la barra de estado para que nunca haya duda
/// de si se opera con dinero simulado o real.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradingMode {
    Paper,
    Live,
}

/// Estado de una conexión en tiempo real (datos de mercado o actualizaciones de órdenes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamStatus {
    Connecting,
    Live,
    Disconnected(String),
    /// Esperando para reintentar; `retry_at` es cuándo se hará el intento número `attempt`.
    Reconnecting {
        attempt: u32,
        retry_at: DateTime<Utc>,
    },
}

impl StreamStatus {
    /// Sin conexión (caída o esperando reconectar): `r` fuerza un reintento inmediato.
    pub fn is_down(&self) -> bool {
        matches!(self, Self::Disconnected(_) | Self::Reconnecting { .. })
    }
}

/// Panel que recibe la navegación con `j`/`k`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Watchlist,
    Orders,
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
    pub order_stream_status: StreamStatus,
    pub notifications: Notifications,
    pub overlay: Overlay,
    pub focus: Panel,
    pub selected_order: usize,
    /// Interruptor de emergencia; se carga del almacenamiento al iniciar.
    pub kill_switch: KillSwitch,
    pub should_quit: bool,
    pub(crate) last_portfolio_request: Option<DateTime<Utc>>,
    pub(crate) last_clock_request: Option<DateTime<Utc>>,
    /// Un evento de orden pidió refrescar el portafolio; se agrupa para no saturar la API.
    pub(crate) portfolio_refresh_pending: bool,
    /// Último dato de mercado recibido; sirve para detectar un flujo "conectado" pero congelado.
    pub(crate) last_market_event_at: Option<DateTime<Utc>>,
    /// El flujo de órdenes ya estuvo conectado antes (una nueva conexión es una reconexión).
    pub(crate) order_stream_was_connected: bool,
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
            order_stream_status: StreamStatus::Connecting,
            notifications: Notifications::default(),
            // En real, la sesión empieza mostrando la cuenta antes que nada
            overlay: match mode {
                TradingMode::Paper => Overlay::None,
                TradingMode::Live => Overlay::LiveSessionStart,
            },
            focus: Panel::Watchlist,
            selected_order: 0,
            kill_switch: KillSwitch::Released,
            should_quit: false,
            last_portfolio_request: None,
            last_clock_request: None,
            portfolio_refresh_pending: false,
            last_market_event_at: None,
            order_stream_was_connected: false,
        }
    }

    pub fn is_live(&self) -> bool {
        self.mode == TradingMode::Live
    }

    /// Órdenes abiertas según el último portafolio cargado.
    pub fn open_orders(&self) -> &[Order] {
        self.portfolio
            .as_ref()
            .map_or(&[], |snapshot| snapshot.open_orders.as_slice())
    }

    pub fn selected_open_order(&self) -> Option<&Order> {
        self.open_orders().get(self.selected_order)
    }
}
