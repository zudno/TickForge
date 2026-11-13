//! Paleta de la TUI. Se usan los 16 colores básicos para que se vea igual en cualquier
//! terminal (incluida la consola clásica de Windows).

use ratatui::style::{Color, Modifier, Style};

pub const POSITIVE: Color = Color::Green;
pub const NEGATIVE: Color = Color::Red;
pub const MUTED: Color = Color::DarkGray;
pub const ACCENT: Color = Color::Cyan;
pub const WARNING: Color = Color::Yellow;

pub fn title() -> Style {
    Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)
}

pub fn header() -> Style {
    Style::new().fg(Color::Gray).add_modifier(Modifier::BOLD)
}

pub fn muted() -> Style {
    Style::new().fg(MUTED)
}

pub fn selected_row() -> Style {
    Style::new().add_modifier(Modifier::REVERSED)
}

/// Insignia del modo de operación: amarilla en paper, roja en real.
pub fn paper_badge() -> Style {
    Style::new()
        .fg(Color::Black)
        .bg(Color::Yellow)
        .add_modifier(Modifier::BOLD)
}

pub fn live_badge() -> Style {
    Style::new()
        .fg(Color::White)
        .bg(Color::Red)
        .add_modifier(Modifier::BOLD)
}

/// Borde de un panel: cian con el foco; sin foco, rojo en modo real (para que el dinero
/// real se note en toda la pantalla) y neutro en paper.
pub fn panel_border(live: bool, focused: bool) -> Style {
    match (focused, live) {
        (true, _) => Style::new().fg(ACCENT),
        (false, true) => Style::new().fg(NEGATIVE),
        (false, false) => Style::new(),
    }
}

/// Texto de la insignia del modo, igual en todas las ventanas.
pub fn mode_label(live: bool) -> &'static str {
    if live {
        " LIVE · DINERO REAL "
    } else {
        " PAPER "
    }
}

pub fn mode_badge(live: bool) -> Style {
    if live { live_badge() } else { paper_badge() }
}

/// Insignia del kill switch activo: imposible de pasar por alto.
pub fn kill_switch_badge() -> Style {
    Style::new()
        .fg(Color::White)
        .bg(Color::Red)
        .add_modifier(Modifier::BOLD)
}

/// Color del lado de una orden: verde compra, rojo vende.
pub fn side(side: tickforge_domain::trading::OrderSide) -> Style {
    match side {
        tickforge_domain::trading::OrderSide::Buy => Style::new().fg(POSITIVE),
        tickforge_domain::trading::OrderSide::Sell => Style::new().fg(NEGATIVE),
    }
}

/// Color según el signo de un cambio (verde sube, rojo baja).
pub fn signed(is_positive: bool, is_negative: bool) -> Style {
    if is_positive {
        Style::new().fg(POSITIVE)
    } else if is_negative {
        Style::new().fg(NEGATIVE)
    } else {
        Style::new()
    }
}
