use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table, TableState};
use tickforge_domain::trading::{Order, OrderType};

use crate::format;
use crate::labels;
use crate::state::{AppState, Panel};
use crate::theme;
use crate::ui::panel_block;

/// Órdenes abiertas. Con el panel enfocado (`Tab`), `c` cancela la seleccionada.
pub(super) fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let focused = state.focus == Panel::Orders;
    let orders = state.open_orders();
    let title = match &state.portfolio {
        Some(_) => format!(" Órdenes abiertas ({}) ", orders.len()),
        None => " Órdenes abiertas ".to_owned(),
    };
    let block = panel_block(title, focused, state.is_live());

    if state.portfolio.is_none() || orders.is_empty() {
        let message = if state.portfolio.is_none() {
            "Cargando…"
        } else {
            "Sin órdenes abiertas · b comprar · s vender"
        };
        frame.render_widget(
            Paragraph::new(Span::styled(message, theme::muted())).block(block),
            area,
        );
        return;
    }

    let header = Row::new(
        ["Lado", "Símbolo", "Cant.", "Precio", "Hecho", "Estado"]
            .into_iter()
            .enumerate()
            .map(|(index, title)| right_if(title, (2..5).contains(&index)).style(theme::header())),
    );
    // Ajustado para caber en ~48 columnas (el panel lateral en una terminal de 120)
    let widths = [
        Constraint::Length(6),
        Constraint::Length(8),
        Constraint::Length(6),
        Constraint::Length(8),
        Constraint::Length(6),
        Constraint::Min(9),
    ];

    let mut table = Table::new(orders.iter().map(row), widths)
        .header(header)
        .block(block);
    // La selección solo se resalta cuando el panel tiene el foco
    if focused {
        table = table.row_highlight_style(theme::selected_row());
    }
    let mut table_state =
        TableState::default().with_selected(focused.then_some(state.selected_order));
    frame.render_stateful_widget(table, area, &mut table_state);
}

fn row(order: &Order) -> Row<'static> {
    let request = order.request();
    let price = match request.order_type() {
        OrderType::Market => "Mercado".to_owned(),
        OrderType::Limit { limit_price } => format::decimal_price(limit_price.value()),
    };
    Row::new(vec![
        Cell::from(labels::side(request.side())).style(theme::side(request.side())),
        Cell::from(request.symbol().to_string()).style(theme::title()),
        right_if(request.quantity().to_string(), true),
        right_if(price, true),
        right_if(
            format!("{}/{}", order.filled_quantity(), request.quantity()),
            true,
        ),
        Cell::from(labels::status(order.status())),
    ])
}

fn right_if(text: impl Into<String>, right: bool) -> Cell<'static> {
    let alignment = if right {
        Alignment::Right
    } else {
        Alignment::Left
    };
    Cell::from(Line::from(text.into()).alignment(alignment))
}
