use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

use crate::theme;

const SHORTCUTS: [(&str, &str); 6] = [
    ("j / ↓", "Siguiente símbolo"),
    ("k / ↑", "Símbolo anterior"),
    ("r", "Actualizar cuenta y reconectar datos"),
    ("?", "Mostrar u ocultar esta ayuda"),
    ("Esc", "Cerrar ventana"),
    ("q / Ctrl+C", "Salir"),
];

/// Ventana centrada con los atajos de teclado.
pub(super) fn render(frame: &mut Frame) {
    let height = u16::try_from(SHORTCUTS.len()).unwrap_or(u16::MAX) + 4;
    let [area] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(frame.area());
    let [area] = Layout::horizontal([Constraint::Length(52)])
        .flex(Flex::Center)
        .areas(area);

    let mut lines: Vec<Line> = SHORTCUTS
        .iter()
        .map(|(keys, description)| {
            Line::from(vec![
                Span::styled(format!("{keys:>12}  "), theme::title()),
                Span::raw(*description),
            ])
        })
        .collect();
    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        "Próximamente: comprar y vender desde aquí",
        theme::muted(),
    )));

    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::bordered().title(Span::styled(" Atajos de teclado ", theme::title()))),
        area,
    );
}
