use ratatui::layout::{Constraint, Flex, Layout, Rect};

/// Rectángulo centrado de `width` x `height`, recortado al área disponible.
pub(super) fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let [area] = Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .areas(area);
    let [area] = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .areas(area);
    area
}

/// Alto de una ventana para `lines` líneas de contenido más los dos bordes.
pub(super) fn height_for(lines: usize) -> u16 {
    u16::try_from(lines).unwrap_or(u16::MAX).saturating_add(2)
}

/// Alto de una ventana de ancho `width` cuyo contenido se ajusta por palabras: cada línea
/// ocupa tantas filas como haga falta para el ancho interior (sin los bordes).
pub(super) fn wrapped_height(lines: &[ratatui::text::Line], width: u16) -> u16 {
    let inner = usize::from(width.saturating_sub(2).max(1));
    // Se suma una fila de holgura por línea larga: el ajuste por palabras puede necesitarla
    let rows: usize = lines
        .iter()
        .map(|line| match line.width() {
            0 => 1,
            w if w <= inner => 1,
            w => w.div_ceil(inner) + 1,
        })
        .sum();
    height_for(rows)
}
