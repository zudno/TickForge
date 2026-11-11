//! Persistencia local en SQLite.
//!
//! - `SqliteOrderJournal` implementa `OrderJournal` (diario de auditoría, solo se agrega).
//! - `SqliteWatchlistRepository` implementa `WatchlistRepository`.
//!
//! SQLite va incluido en el binario (`rusqlite` con `bundled`) y las migraciones se embeben al
//! compilar (versionadas con `PRAGMA user_version`): no hay que
//! instalar nada. La base se crea sola en el primer arranque.

mod error;
mod order_journal;
mod rows;
mod store;
mod watchlist_repository;

pub use error::StoreError;
pub use order_journal::SqliteOrderJournal;
pub use store::SqliteStore;
pub use watchlist_repository::SqliteWatchlistRepository;
