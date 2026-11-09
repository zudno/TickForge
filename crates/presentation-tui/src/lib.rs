//! Capa de presentación de TickForge (TUI con ratatui).
//!
//! Sigue un flujo unidireccional: `Event -> Action -> reducer -> AppState -> render`.
//! Nunca llama directamente a un adaptador; solo invoca casos de uso de la capa de aplicación.
//! Los efectos secundarios (red) se piden como `Command` y se ejecutan en tareas aparte.

mod action;
mod app;
mod command;
mod format;
mod keymap;
mod labels;
mod state;
mod theme;
mod ui;

pub use app::{TuiConfig, TuiError, run};
pub use command::TuiServices;
pub use state::TradingMode;
