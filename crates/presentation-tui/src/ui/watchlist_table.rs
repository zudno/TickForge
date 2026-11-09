use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Cell, Row, Table, TableState};
use rust_decimal::Decimal;

use crate::format;
use crate::state::{AppState, PriceDirection, WatchlistRow};
use crate::theme;

const COLUMNS: [&str; 7] = [
    "Símbolo", "Último", "Cambio", "Bid", "Ask", "Spread", "Hora",
];

/// Tabla de la watchlist con precios en vivo y la fila seleccionada resaltada.
pub(super) fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let header = Row::new(
        COLUMNS
            .iter()
            .enumerate()
            .map(|(index, title)| aligned(*title, index > 0).style(theme::header())),
    );

    let rows = state.watchlist.rows().iter().map(row);
    let widths = [
        Constraint::Length(8),
        Constraint::Length(10),
        Constraint::Length(9),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(7),
        Constraint::Min(11),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::bordered().title(Span::styled(" Watchlist ", theme::title())))
        .row_highlight_style(theme::selected_row())
        .column_spacing(1);

    let mut table_state = TableState::default().with_selected(state.watchlist.selected_index());
    frame.render_stateful_widget(table, area, &mut table_state);
}

fn row(row: &WatchlistRow) -> Row<'static> {
    let change = row.change_pct();
    let change_style = change.map_or_else(Style::new, |pct| {
        theme::signed(pct > Decimal::ZERO, pct < Decimal::ZERO)
    });
    let last_style = match row.direction {
        PriceDirection::Up => Style::new().fg(theme::POSITIVE),
        PriceDirection::Down => Style::new().fg(theme::NEGATIVE),
        PriceDirection::Unchanged => Style::new(),
    };

    Row::new(vec![
        Cell::from(row.symbol.to_string()).style(theme::title()),
        aligned(format::price(row.last), true).style(last_style),
        aligned(format::percent(change), true).style(change_style),
        aligned(format::price(row.bid), true),
        aligned(format::price(row.ask), true),
        aligned(format::spread(row.spread()), true).style(theme::muted()),
        aligned(
            row.updated_at
                .map_or_else(|| format::MISSING.to_owned(), format::new_york_time),
            true,
        )
        .style(theme::muted()),
    ])
}

/// Celda con alineación: los números van a la derecha para que se lean en columna.
fn aligned(text: impl Into<String>, right: bool) -> Cell<'static> {
    let alignment = if right {
        Alignment::Right
    } else {
        Alignment::Left
    };
    Cell::from(Line::from(text.into()).alignment(alignment))
}
