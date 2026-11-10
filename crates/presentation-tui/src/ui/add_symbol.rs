use ratatui::Frame;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};

use crate::state::SymbolInput;
use crate::theme;
use crate::ui::modal;

/// Ventana para escribir un símbolo nuevo de la watchlist.
pub(super) fn render(frame: &mut Frame, input: &SymbolInput) {
    let mut value = vec![
        Span::styled("Símbolo  ", theme::muted()),
        Span::styled(
            input.text.clone(),
            Style::new().add_modifier(Modifier::BOLD),
        ),
    ];
    if !input.awaiting {
        value.push(Span::styled("█", theme::title()));
    }

    let status = if input.awaiting {
        Line::from(Span::styled(
            "Verificando con el proveedor de datos…",
            theme::muted(),
        ))
    } else if let Some(error) = &input.error {
        Line::from(Span::styled(
            error.clone(),
            Style::new().fg(theme::NEGATIVE),
        ))
    } else {
        Line::from(Span::styled("Ejemplos: TSLA, GOOGL, BRK.B", theme::muted()))
    };

    let lines = vec![
        Line::from(value),
        Line::default(),
        status,
        Line::from(Span::styled("Enter agregar · Esc cancelar", theme::muted())),
    ];

    // El motivo de un rechazo puede ser largo: se ajusta por palabras, nunca se corta
    let width = 52;
    let area = modal::centered(frame.area(), width, modal::wrapped_height(&lines, width));
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }).block(
            Block::bordered().title(Span::styled(" Agregar a la watchlist ", theme::title())),
        ),
        area,
    );
}
