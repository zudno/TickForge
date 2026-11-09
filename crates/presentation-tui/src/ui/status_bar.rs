use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::format;
use crate::state::{AppState, NotificationLevel, StreamStatus, TradingMode};
use crate::theme;

/// Barra inferior: modo de operación (siempre visible), sesión, conexión, hora y mensajes.
pub(super) fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let hint = " ? ayuda  q salir ";
    let hint_width = u16::try_from(hint.chars().count()).unwrap_or(u16::MAX);
    let [left_area, right_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(hint_width)]).areas(area);

    let mut spans = vec![mode_badge(state.mode), Span::raw(" ")];
    spans.extend(market_session(state));
    spans.push(separator());
    spans.push(stream_status(&state.stream_status));
    spans.push(separator());
    spans.push(Span::raw(format::new_york_time(state.now)));
    if let Some(notification) = state.notifications.latest() {
        spans.push(separator());
        spans.push(Span::styled(
            notification.message.clone(),
            notification_style(notification.level),
        ));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), left_area);
    frame.render_widget(
        Paragraph::new(Span::styled(hint, theme::muted())),
        right_area,
    );
}

fn mode_badge(mode: TradingMode) -> Span<'static> {
    match mode {
        TradingMode::Paper => Span::styled(" PAPER ", theme::paper_badge()),
        TradingMode::Live => Span::styled(" LIVE ", theme::live_badge()),
    }
}

fn market_session(state: &AppState) -> Vec<Span<'static>> {
    let Some(clock) = state.market_clock else {
        return vec![Span::styled("Mercado: —", theme::muted())];
    };
    // Tiempo restante desde ahora, no desde que se consultó el reloj
    let elapsed = state.now.signed_duration_since(clock.as_of());
    let remaining = format::duration(
        clock
            .time_until_next_change()
            .checked_sub(&elapsed)
            .unwrap_or_default(),
    );

    if clock.is_open() {
        vec![
            Span::styled("Mercado ABIERTO", Style::new().fg(theme::POSITIVE)),
            Span::styled(format!(" · cierra en {remaining}"), theme::muted()),
        ]
    } else {
        vec![
            Span::styled("Mercado CERRADO", Style::new().fg(theme::WARNING)),
            Span::styled(format!(" · abre en {remaining}"), theme::muted()),
        ]
    }
}

fn stream_status(status: &StreamStatus) -> Span<'static> {
    match status {
        StreamStatus::Connecting => Span::styled("○ Conectando…", theme::muted()),
        StreamStatus::Live => Span::styled("● En vivo", Style::new().fg(theme::POSITIVE)),
        StreamStatus::Disconnected(_) => {
            Span::styled("✕ Sin datos en vivo", Style::new().fg(theme::NEGATIVE))
        }
    }
}

fn notification_style(level: NotificationLevel) -> Style {
    match level {
        NotificationLevel::Info => theme::muted(),
        NotificationLevel::Warning => Style::new().fg(theme::WARNING),
        NotificationLevel::Error => Style::new().fg(theme::NEGATIVE),
    }
}

fn separator() -> Span<'static> {
    Span::styled(" │ ", theme::muted())
}
