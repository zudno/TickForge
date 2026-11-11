use ratatui::Frame;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use tickforge_domain::risk::KillSwitch;

use crate::format;
use crate::state::{AppState, KillSwitchDialog};
use crate::theme;
use crate::ui::modal;

/// Confirmación del kill switch: deja claro qué se cancela y qué **no** se toca.
pub(super) fn render(frame: &mut Frame, state: &AppState, dialog: &KillSwitchDialog) {
    let mut lines = if dialog.engage {
        engage_lines(dialog)
    } else {
        release_lines(state)
    };

    if let Some(error) = &dialog.error {
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            format!("✕ {error}"),
            Style::new().fg(theme::NEGATIVE),
        )));
    }
    lines.push(Line::default());
    let hint = match (dialog.submitting, dialog.engage) {
        (true, true) => "Activando y cancelando órdenes…",
        (true, false) => "Desactivando…",
        (false, true) => "Enter activar · Esc cancelar",
        (false, false) => "Enter desactivar · Esc cancelar",
    };
    lines.push(Line::from(Span::styled(hint, theme::muted())));

    let danger = Style::new().fg(theme::NEGATIVE);
    let title = if dialog.engage {
        " ⛔ Activar kill switch "
    } else {
        " Desactivar kill switch "
    };
    let block = Block::bordered()
        .border_style(danger)
        .title(Span::styled(title, danger.add_modifier(Modifier::BOLD)));

    let width = 64;
    let area = modal::centered(frame.area(), width, modal::wrapped_height(&lines, width));
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(block),
        area,
    );
}

fn engage_lines(dialog: &KillSwitchDialog) -> Vec<Line<'static>> {
    let bold = Style::new().add_modifier(Modifier::BOLD);
    let cancel = match dialog.open_orders {
        0 => "  • No hay órdenes abiertas que cancelar".to_owned(),
        1 => "  • Cancelar la orden abierta".to_owned(),
        count => format!("  • Cancelar las {count} órdenes abiertas"),
    };
    vec![
        Line::from(Span::styled("Esto hará de inmediato:", bold)),
        Line::from(cancel),
        Line::from("  • Bloquear toda orden nueva hasta que lo desactives"),
        Line::default(),
        Line::from(Span::styled(
            "Las posiciones abiertas NO se cierran.",
            Style::new().fg(theme::WARNING),
        )),
        Line::from(Span::styled(
            "El bloqueo se guarda: sigue activo aunque cierres TickForge.",
            theme::muted(),
        )),
    ]
}

fn release_lines(state: &AppState) -> Vec<Line<'static>> {
    let since = match &state.kill_switch {
        KillSwitch::Engaged { since, reason } => {
            format!(
                "Activo desde las {} ({reason}).",
                format::new_york_time(*since)
            )
        }
        KillSwitch::Released => "No está activo.".to_owned(),
    };
    vec![
        Line::from(since),
        Line::default(),
        Line::from("¿Permitir órdenes nuevas otra vez?"),
    ]
}
