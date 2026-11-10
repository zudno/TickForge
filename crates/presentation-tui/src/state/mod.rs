//! Estado de la TUI. Es la única fuente de verdad para dibujar: el render es una función pura
//! de este estado, y solo el reducer lo modifica.

mod app_state;
mod notifications;
mod order_ticket;
mod overlay;
mod reducer;
mod watchlist;

pub use app_state::{AppState, Panel, StreamStatus, TradingMode};
pub use notifications::{NotificationLevel, Notifications};
pub use order_ticket::{OrderTicket, TicketField, TicketOrderType};
pub use overlay::{CancelConfirmation, Overlay, PreviewOverlay, SymbolInput};
pub use reducer::{initial_commands, reduce};
pub use watchlist::{PriceDirection, Watchlist, WatchlistRow};
