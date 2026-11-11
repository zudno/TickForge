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
    if state.kill_switch.is_engaged() {
        spans.push(Span::styled(" ⛔ DETENIDO ", theme::kill_switch_badge()));
        spans.push(Span::raw(" "));
    }
    spans.extend(market_session(state));
    spans.push(separator());
    spans.push(connections(state));
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

/// Estado combinado de las dos conexiones en tiempo real (datos y órdenes): se muestra el
/// problema más importante, o "En vivo" si ambas funcionan.
fn connections(state: &AppState) -> Span<'static> {
    let down = Style::new().fg(theme::NEGATIVE);
    let retrying = Style::new().fg(theme::WARNING);
    match (&state.stream_status, &state.order_stream_status) {
        (StreamStatus::Reconnecting { attempt, retry_at }, _) => Span::styled(
            format!(
                "↻ Datos: reintento {attempt} {}",
                countdown(state, *retry_at)
            ),
            retrying,
        ),
        (_, StreamStatus::Reconnecting { attempt, retry_at }) => Span::styled(
            format!(
                "↻ Órdenes: reintento {attempt} {}",
                countdown(state, *retry_at)
            ),
            retrying,
        ),
        (StreamStatus::Disconnected(_), _) => Span::styled("✕ Sin datos en vivo", down),
        (_, StreamStatus::Disconnected(_)) => Span::styled("✕ Órdenes sin conexión", down),
        (StreamStatus::Live, StreamStatus::Live) => {
            Span::styled("● En vivo", Style::new().fg(theme::POSITIVE))
        }
        _ => Span::styled("○ Conectando…", theme::muted()),
    }
}

/// "en 8s" mientras se espera; "ahora" cuando ya toca reintentar.
fn countdown(state: &AppState, retry_at: chrono::DateTime<chrono::Utc>) -> String {
    let remaining = retry_at.signed_duration_since(state.now).num_seconds();
    if remaining > 0 {
        format!("en {remaining}s")
    } else {
        "ahora".to_owned()
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
