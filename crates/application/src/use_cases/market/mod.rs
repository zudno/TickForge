//! Casos de uso de mercado: seguimiento de la watchlist, estado de la sesión y grabación
//! del histórico de velas.

mod get_market_clock;
mod manage_watchlist;
mod record_market_data;
mod watch_market;

pub use get_market_clock::GetMarketClockUseCase;
pub use manage_watchlist::ManageWatchlistUseCase;
pub use record_market_data::{BackfillReport, RecordMarketDataUseCase, SymbolBackfill};
pub use watch_market::{SymbolSnapshot, WatchMarketUseCase};
