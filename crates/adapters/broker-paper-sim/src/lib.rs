//! Implementaciones en memoria de los puertos de la aplicación.
//!
//! Sirven para pruebas de casos de uso, demos y desarrollo de la TUI sin red ni llaves de API.
//! Ninguna de estas piezas debe usarse para operar dinero real.

mod in_memory_bar_repository;
mod in_memory_kill_switch_repository;
mod in_memory_order_journal;
mod in_memory_watchlist_repository;
mod lock;
mod manual_clock;
mod paper_broker;
mod recording_event_publisher;
mod sequential_client_order_id_generator;

pub use in_memory_bar_repository::InMemoryBarRepository;
pub use in_memory_kill_switch_repository::InMemoryKillSwitchRepository;
pub use in_memory_order_journal::InMemoryOrderJournal;
pub use in_memory_watchlist_repository::InMemoryWatchlistRepository;
pub use manual_clock::ManualClock;
pub use paper_broker::PaperBroker;
pub use recording_event_publisher::RecordingEventPublisher;
pub use sequential_client_order_id_generator::SequentialClientOrderIdGenerator;
