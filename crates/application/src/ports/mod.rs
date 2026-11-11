//! Puertos: contratos que la capa de aplicación necesita del exterior.
//!
//! Cada adaptador (Alpaca, IBKR, SQLite, simulador en memoria, ONNX) implementa uno o más
//! de estos traits. Los casos de uso solo conocen los traits, nunca las implementaciones.

mod broker_gateway;
mod client_order_id_generator;
mod clock;
mod event_publisher;
mod kill_switch_repository;
mod market_data_feed;
mod order_journal;
mod watchlist_repository;

pub use broker_gateway::{BrokerError, BrokerGateway, OrderUpdate, OrderUpdateKind};
pub use client_order_id_generator::ClientOrderIdGenerator;
pub use clock::Clock;
pub use event_publisher::{ApplicationEvent, EventPublisher};
pub use kill_switch_repository::KillSwitchRepository;
pub use market_data_feed::{MarketDataError, MarketDataFeed, MarketEvent};
pub use order_journal::{JournalEntry, JournalError, OrderJournal, ReconcileOutcome};
pub use watchlist_repository::{RepositoryError, WatchlistRepository};
