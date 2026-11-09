use ratatui::Frame;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

use crate::labels;
use crate::state::CancelConfirmation;
use crate::theme;
use crate::ui::modal;

/// Confirmación antes de cancelar una orden abierta.
pub(super) fn render(frame: &mut Frame, confirmation: &CancelConfirmation) {
    let order = &confirmation.order;
    let mut lines = vec![
        Line::from(Span::styled(
            "¿Cancelar esta orden?",
            Style::new().add_modifier(Modifier::BOLD),
        )),
        Line::from(labels::order_summary(order)),
        Line::from(Span::styled(
            format!(
                "{} · ejecutadas {} de {}",
                labels::status(order.status()),
                order.filled_quantity(),
                order.request().quantity()
            ),
            theme::muted(),
        )),
    ];
    if let Some(error) = &confirmation.error {
        lines.push(Line::from(Span::styled(
            format!("✕ {error}"),
            Style::new().fg(theme::NEGATIVE),
        )));
    }
    lines.push(Line::default());
    let hint = if confirmation.submitting {
        "Cancelando…"
    } else {
        "Enter / s: sí, cancelar · Esc / n: no"
    };
    lines.push(Line::from(Span::styled(hint, theme::muted())));

    let block = Block::bordered()
        .border_style(Style::new().fg(theme::WARNING))
        .title(Span::styled(
            " Cancelar orden ",
            Style::new().fg(theme::WARNING).add_modifier(Modifier::BOLD),
        ));
    let area = modal::centered(frame.area(), 60, modal::height_for(lines.len()));
    frame.render_widget(Clear, area);
    frame.render_widget(Paragraph::new(lines).block(block), area);
}
