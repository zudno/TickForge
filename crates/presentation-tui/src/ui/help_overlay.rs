use ratatui::Frame;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

use crate::theme;
use crate::ui::modal;

const SHORTCUTS: [(&str, &str); 12] = [
    ("j / ↓", "Bajar en el panel activo"),
    ("k / ↑", "Subir en el panel activo"),
    ("Tab", "Cambiar entre watchlist y órdenes"),
    ("b", "Comprar el símbolo seleccionado"),
    ("s", "Vender el símbolo seleccionado"),
    ("c", "Cancelar la orden seleccionada"),
    ("a", "Agregar un símbolo a la watchlist"),
    ("d", "Quitar el símbolo seleccionado"),
    ("r", "Actualizar y reconectar"),
    ("?", "Mostrar u ocultar esta ayuda"),
    ("Esc", "Cerrar o volver"),
    ("q / Ctrl+C", "Salir"),
];

/// Ventana centrada con los atajos de teclado.
pub(super) fn render(frame: &mut Frame) {
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
        "Toda orden muestra su costo y pasa la política de riesgo antes de enviarse",
        theme::muted(),
    )));

    let area = modal::centered(frame.area(), 80, modal::height_for(lines.len()));
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::bordered().title(Span::styled(" Atajos de teclado ", theme::title()))),
        area,
    );
}
