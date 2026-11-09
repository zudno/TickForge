//! Dibujo de la interfaz. Todo es una función pura de `AppState`: no hay estado oculto
//! en los componentes.

mod account_bar;
mod help_overlay;
mod positions_table;
mod status_bar;
mod watchlist_table;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};

use crate::state::AppState;

/// Pantalla principal:
///
/// ```text
/// ┌ Cuenta ─────────────────────────────────────┐
/// ├ Watchlist ──────────────┬ Posiciones ────────┤
/// │                         │                    │
/// └─────────────────────────┴────────────────────┘
///  PAPER · mercado · datos · hora · mensajes
/// ```
pub fn render(frame: &mut Frame, state: &AppState) {
    let [account_area, main_area, status_area] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(6),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    let [watchlist_area, positions_area] =
        Layout::horizontal([Constraint::Percentage(62), Constraint::Percentage(38)])
            .areas(main_area);

    account_bar::render(frame, account_area, state);
    watchlist_table::render(frame, watchlist_area, state);
    positions_table::render(frame, positions_area, state);
    status_bar::render(frame, status_area, state);

    if state.show_help {
        help_overlay::render(frame);
    }
}

#[cfg(test)]
mod tests;
