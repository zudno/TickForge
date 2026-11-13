//! Dibujo de la interfaz. Todo es una función pura de `AppState`: no hay estado oculto
//! en los componentes.

mod account_bar;
mod add_symbol;
mod cancel_confirm;
mod help_overlay;
mod kill_switch_dialog;
mod live_session_start;
mod modal;
mod order_preview;
mod order_ticket;
mod orders_table;
mod positions_table;
mod status_bar;
mod watchlist_table;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::text::Span;
use ratatui::widgets::Block;

use crate::state::{AppState, Overlay};
use crate::theme;

/// Pantalla principal:
///
/// ```text
/// ┌ Cuenta ───────────────────────────────────────────┐
/// ├ Watchlist ──────────────────┬ Posiciones ─────────┤
/// │                             ├ Órdenes abiertas ───┤
/// └─────────────────────────────┴─────────────────────┘
///  PAPER · mercado · conexiones · hora · mensajes
/// ```
pub fn render(frame: &mut Frame, state: &AppState) {
    let [account_area, main_area, status_area] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(6),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    let [watchlist_area, side_area] =
        Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)])
            .areas(main_area);
    let [positions_area, orders_area] =
        Layout::vertical([Constraint::Percentage(45), Constraint::Percentage(55)]).areas(side_area);

    account_bar::render(frame, account_area, state);
    watchlist_table::render(frame, watchlist_area, state);
    positions_table::render(frame, positions_area, state);
    orders_table::render(frame, orders_area, state);
    status_bar::render(frame, status_area, state);

    match &state.overlay {
        Overlay::None => {}
        Overlay::Help => help_overlay::render(frame),
        Overlay::Ticket(ticket) => order_ticket::render(frame, ticket),
        Overlay::Preview(preview) => order_preview::render(frame, state, preview),
        Overlay::CancelConfirm(confirmation) => cancel_confirm::render(frame, state, confirmation),
        Overlay::AddSymbol(input) => add_symbol::render(frame, input),
        Overlay::KillSwitch(dialog) => kill_switch_dialog::render(frame, state, dialog),
        Overlay::LiveSessionStart => live_session_start::render(frame, state),
    }
}

/// Bloque de un panel; el borde se resalta con el foco y es rojo en modo real.
pub(crate) fn panel_block(title: impl Into<String>, focused: bool, live: bool) -> Block<'static> {
    Block::bordered()
        .border_style(theme::panel_border(live, focused))
        .title(Span::styled(title.into(), theme::title()))
}

/// Insignia del modo para el título de una ventana.
pub(crate) fn mode_title(live: bool) -> Span<'static> {
    Span::styled(theme::mode_label(live), theme::mode_badge(live))
}

#[cfg(test)]
mod tests;
