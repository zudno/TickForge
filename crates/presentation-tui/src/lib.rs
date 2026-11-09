//! Capa de presentación de TickForge (TUI con ratatui).
//!
//! Sigue un flujo unidireccional: `AppEvent -> Action -> reducer -> AppState -> render`.
//! Nunca llama directamente a un adaptador; solo invoca casos de uso de la capa de aplicación.
