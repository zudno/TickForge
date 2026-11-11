use chrono::{DateTime, TimeDelta, Utc};
use tickforge_application::ports::{OrderUpdate, OrderUpdateKind};
use tickforge_domain::trading::OrderSide;

use crate::action::Action;
use crate::command::Command;
use crate::labels;
use crate::state::{
    AppState, CancelConfirmation, NotificationLevel, OrderTicket, Overlay, Panel, PreviewOverlay,
    StreamStatus,
};

/// Cada cuánto se refresca el portafolio por REST (posiciones, saldos).
const PORTFOLIO_REFRESH: TimeDelta = TimeDelta::seconds(30);

/// Espera mínima entre refrescos provocados por eventos de órdenes (agrupa ráfagas de fills).
const EVENT_PORTFOLIO_REFRESH: TimeDelta = TimeDelta::seconds(1);

/// Cada cuánto se consulta el reloj de mercado.
const CLOCK_REFRESH: TimeDelta = TimeDelta::seconds(60);

/// Comandos a ejecutar al arrancar.
pub fn initial_commands(state: &mut AppState) -> Vec<Command> {
    state.last_portfolio_request = Some(state.now);
    state.last_clock_request = Some(state.now);
    vec![
        Command::LoadPortfolio,
        Command::LoadMarketClock,
        Command::StartMarketStream(state.watchlist.symbols()),
        Command::StartOrderStream,
    ]
}

/// Aplica una acción al estado y devuelve los efectos secundarios a ejecutar.
///
/// Es una función pura respecto al mundo exterior: no hace I/O ni lee el reloj, lo que
/// permite probar todo el comportamiento de la TUI sin terminal ni red.
pub fn reduce(state: &mut AppState, action: Action) -> Vec<Command> {
    let now = state.now;
    match action {
        // --- Navegación ---
        Action::Quit => state.should_quit = true,
        Action::SelectNext => select(state, 1),
        Action::SelectPrevious => select(state, -1),
        Action::FocusNextPanel => {
            state.focus = match state.focus {
                Panel::Watchlist => Panel::Orders,
                Panel::Orders => Panel::Watchlist,
            };
        }
        Action::ToggleHelp => {
            state.overlay = match state.overlay {
                Overlay::Help => Overlay::None,
                _ => Overlay::Help,
            };
        }
        Action::CloseOverlay => close_overlay(state),
        Action::Refresh => return refresh(state),
        Action::Confirm => return confirm(state),

        // --- Ticket de orden ---
        Action::OpenTicket(side) => open_ticket(state, side),
        Action::TicketNextField => with_ticket(state, OrderTicket::next_field),
        Action::TicketPreviousField => with_ticket(state, OrderTicket::previous_field),
        Action::TicketToggle => with_ticket(state, OrderTicket::toggle),
        Action::TicketInput(character) => with_ticket(state, |ticket| ticket.input(character)),
        Action::TicketBackspace => with_ticket(state, OrderTicket::backspace),
        Action::RequestCancelSelected => request_cancel(state),

        Action::Tick(instant) => return tick(state, instant),

        // --- Datos de mercado ---
        Action::WatchlistSeeded(snapshots) => {
            for snapshot in snapshots {
                state.watchlist.apply_snapshot(snapshot);
            }
        }
        Action::Market(event) => {
            state.watchlist.apply_event(&event);
        }
        Action::StreamConnected => {
            state.stream_status = StreamStatus::Live;
            state.notifications.push(
                now,
                NotificationLevel::Info,
                "Datos en tiempo real conectados",
            );
        }
        Action::StreamDisconnected(reason) => {
            state.notifications.push(
                now,
                NotificationLevel::Warning,
                format!("Datos en tiempo real desconectados: {reason}. Pulsa r para reconectar"),
            );
            state.stream_status = StreamStatus::Disconnected(reason);
        }

        // --- Cuenta y sesión ---
        Action::PortfolioLoaded(snapshot) => {
            state.portfolio = Some(snapshot);
            // La orden seleccionada pudo haberse cerrado
            let count = state.open_orders().len();
            state.selected_order = state.selected_order.min(count.saturating_sub(1));
        }
        Action::PortfolioFailed(reason) => state.notifications.push(
            now,
            NotificationLevel::Error,
            format!("No se pudo cargar la cuenta: {reason}"),
        ),
        Action::MarketClockLoaded(clock) => state.market_clock = Some(clock),
        Action::MarketClockFailed(reason) => state.notifications.push(
            now,
            NotificationLevel::Warning,
            format!("No se pudo consultar el horario del mercado: {reason}"),
        ),

        // --- Resultados de órdenes ---
        Action::OrderPreviewReady(preview) => {
            // Solo si el usuario sigue esperando (pudo haber cerrado el ticket)
            if let Overlay::Ticket(ticket) = &state.overlay
                && ticket.awaiting_preview
            {
                let mut ticket = ticket.clone();
                ticket.awaiting_preview = false;
                state.overlay = Overlay::Preview(PreviewOverlay {
                    preview,
                    ticket,
                    submitting: false,
                    error: None,
                });
            }
        }
        Action::OrderPreviewFailed(reason) => {
            if let Overlay::Ticket(ticket) = &mut state.overlay {
                ticket.awaiting_preview = false;
                ticket.error = Some(reason);
            }
        }
        Action::OrderSubmitted(order) => {
            if matches!(state.overlay, Overlay::Preview(_)) {
                state.overlay = Overlay::None;
            }
            state.notifications.push(
                now,
                NotificationLevel::Info,
                format!(
                    "Orden enviada: {} ({})",
                    labels::order_summary(&order),
                    labels::status(order.status())
                ),
            );
            return refresh_portfolio_now(state);
        }
        Action::OrderSubmitFailed(reason) => {
            if let Overlay::Preview(preview) = &mut state.overlay {
                preview.submitting = false;
                preview.error = Some(reason.clone());
            }
            state.notifications.push(
                now,
                NotificationLevel::Error,
                format!("No se envió la orden: {reason}"),
            );
        }
        Action::OrderCancelRequested(_) => {
            if matches!(state.overlay, Overlay::CancelConfirm(_)) {
                state.overlay = Overlay::None;
            }
            state
                .notifications
                .push(now, NotificationLevel::Info, "Cancelación solicitada");
            return refresh_portfolio_now(state);
        }
        Action::OrderCancelFailed(reason) => {
            if let Overlay::CancelConfirm(confirmation) = &mut state.overlay {
                confirmation.submitting = false;
                confirmation.error = Some(reason.clone());
            }
            state.notifications.push(
                now,
                NotificationLevel::Error,
                format!("No se pudo cancelar: {reason}"),
            );
        }
        Action::OrderUpdateReceived(update) => {
            let (level, message) = describe_update(state, &update);
            state.notifications.push(now, level, message);
            state.portfolio_refresh_pending = true;
            return refresh_portfolio_if_due(state);
        }
        Action::OrderStreamConnected => state.order_stream_status = StreamStatus::Live,
        Action::OrderStreamDisconnected(reason) => {
            state.notifications.push(
                now,
                NotificationLevel::Warning,
                format!(
                    "Actualizaciones de órdenes desconectadas: {reason}. Pulsa r para reconectar"
                ),
            );
            state.order_stream_status = StreamStatus::Disconnected(reason);
        }
    }
    Vec::new()
}

fn select(state: &mut AppState, step: isize) {
    match state.focus {
        Panel::Watchlist if step > 0 => state.watchlist.select_next(),
        Panel::Watchlist => state.watchlist.select_previous(),
        Panel::Orders => {
            let count = state.open_orders().len();
            if count > 0 {
                let count = isize::try_from(count).unwrap_or(1);
                let current = isize::try_from(state.selected_order).unwrap_or_default();
                state.selected_order =
                    usize::try_from((current + step).rem_euclid(count)).unwrap_or_default();
            }
        }
    }
}

fn with_ticket(state: &mut AppState, update: impl FnOnce(&mut OrderTicket)) {
    if let Overlay::Ticket(ticket) = &mut state.overlay
        && !ticket.awaiting_preview
    {
        update(ticket);
    }
}

fn open_ticket(state: &mut AppState, side: OrderSide) {
    if state.overlay != Overlay::None {
        return;
    }
    let Some(row) = state.watchlist.selected_row() else {
        return;
    };
    state.overlay = Overlay::Ticket(OrderTicket::new(row.symbol.clone(), side, row.bid, row.ask));
}

fn request_cancel(state: &mut AppState) {
    if state.overlay != Overlay::None || state.focus != Panel::Orders {
        return;
    }
    if let Some(order) = state.selected_open_order() {
        state.overlay = Overlay::CancelConfirm(CancelConfirmation {
            order: order.clone(),
            submitting: false,
            error: None,
        });
    }
}

/// `Esc`: cierra la ventana activa. Desde la confirmación se vuelve al ticket para editar.
/// Mientras se espera al broker no se cierra nada, para no perder el resultado.
fn close_overlay(state: &mut AppState) {
    state.overlay = match std::mem::take(&mut state.overlay) {
        Overlay::Preview(preview) if !preview.submitting => Overlay::Ticket(preview.ticket),
        Overlay::CancelConfirm(confirmation) if confirmation.submitting => {
            Overlay::CancelConfirm(confirmation)
        }
        Overlay::Preview(preview) => Overlay::Preview(preview),
        Overlay::Ticket(ticket) if ticket.awaiting_preview => Overlay::Ticket(ticket),
        Overlay::None | Overlay::Help | Overlay::Ticket(_) | Overlay::CancelConfirm(_) => {
            Overlay::None
        }
    };
}

/// `Enter`: ticket → vista previa → envío; confirmación de cancelación → cancelar.
fn confirm(state: &mut AppState) -> Vec<Command> {
    match &mut state.overlay {
        Overlay::Ticket(ticket) if !ticket.awaiting_preview => match ticket.to_command() {
            Ok(command) => {
                ticket.awaiting_preview = true;
                ticket.error = None;
                vec![Command::PreviewOrder(command)]
            }
            Err(message) => {
                ticket.error = Some(message);
                Vec::new()
            }
        },
        // Solo se envía lo que la política de riesgo aprobó en la vista previa
        Overlay::Preview(preview)
            if !preview.submitting && preview.preview.assessment.is_approved() =>
        {
            preview.submitting = true;
            preview.error = None;
            vec![Command::SubmitOrder(preview.preview.request.clone())]
        }
        Overlay::CancelConfirm(confirmation) if !confirmation.submitting => {
            confirmation.submitting = true;
            confirmation.error = None;
            vec![Command::CancelOrder(confirmation.order.id().clone())]
        }
        _ => Vec::new(),
    }
}

fn refresh(state: &mut AppState) -> Vec<Command> {
    state.last_portfolio_request = Some(state.now);
    state.last_clock_request = Some(state.now);
    let mut commands = vec![Command::LoadPortfolio, Command::LoadMarketClock];

    if matches!(state.stream_status, StreamStatus::Disconnected(_)) {
        state.stream_status = StreamStatus::Connecting;
        commands.push(Command::StartMarketStream(state.watchlist.symbols()));
    }
    if matches!(state.order_stream_status, StreamStatus::Disconnected(_)) {
        state.order_stream_status = StreamStatus::Connecting;
        commands.push(Command::StartOrderStream);
    }
    state
        .notifications
        .push(state.now, NotificationLevel::Info, "Actualizando…");
    commands
}

fn refresh_portfolio_now(state: &mut AppState) -> Vec<Command> {
    state.last_portfolio_request = Some(state.now);
    state.portfolio_refresh_pending = false;
    vec![Command::LoadPortfolio]
}

fn refresh_portfolio_if_due(state: &mut AppState) -> Vec<Command> {
    if state.portfolio_refresh_pending
        && is_due(
            state.last_portfolio_request,
            state.now,
            EVENT_PORTFOLIO_REFRESH,
        )
    {
        return refresh_portfolio_now(state);
    }
    Vec::new()
}

/// Avanza el reloj y programa los refrescos periódicos que correspondan.
fn tick(state: &mut AppState, instant: DateTime<Utc>) -> Vec<Command> {
    state.now = instant;
    let mut commands = refresh_portfolio_if_due(state);

    if commands.is_empty() && is_due(state.last_portfolio_request, instant, PORTFOLIO_REFRESH) {
        commands.extend(refresh_portfolio_now(state));
    }

    // El reloj se consulta periódicamente y también justo después de abrir o cerrar el mercado
    let session_changed = state.market_clock.is_some_and(|clock| {
        clock.time_until_next_change() <= instant.signed_duration_since(clock.as_of())
    });
    let clock_due = is_due(state.last_clock_request, instant, CLOCK_REFRESH);
    if clock_due
        || (session_changed && is_due(state.last_clock_request, instant, TimeDelta::seconds(5)))
    {
        state.last_clock_request = Some(instant);
        commands.push(Command::LoadMarketClock);
    }
    commands
}

fn is_due(last: Option<DateTime<Utc>>, now: DateTime<Utc>, interval: TimeDelta) -> bool {
    last.is_none_or(|last| now.signed_duration_since(last) >= interval)
}

/// Mensaje para una actualización del broker, con el resumen de la orden si se conoce.
fn describe_update(state: &AppState, update: &OrderUpdate) -> (NotificationLevel, String) {
    let summary = state
        .open_orders()
        .iter()
        .find(|order| order.id() == &update.order_id)
        .map_or_else(
            || format!("orden {}", update.order_id),
            labels::order_summary,
        );

    match &update.kind {
        OrderUpdateKind::Accepted => (NotificationLevel::Info, format!("Aceptada: {summary}")),
        OrderUpdateKind::Filled(fill) => (
            NotificationLevel::Info,
            format!(
                "Ejecución: {} acciones a {} ({summary})",
                fill.quantity(),
                crate::format::decimal_price(fill.price().value())
            ),
        ),
        OrderUpdateKind::Canceled => (NotificationLevel::Info, format!("Cancelada: {summary}")),
        OrderUpdateKind::CancelRejected => (
            NotificationLevel::Warning,
            format!("El broker rechazó la cancelación: {summary}"),
        ),
        OrderUpdateKind::Rejected { reason } => (
            NotificationLevel::Error,
            format!("Rechazada: {summary} ({reason})"),
        ),
        OrderUpdateKind::Expired => (NotificationLevel::Info, format!("Expirada: {summary}")),
    }
}

#[cfg(test)]
mod tests;
