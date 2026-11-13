use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table};
use rust_decimal::Decimal;
use tickforge_domain::portfolio::Position;

use crate::format;
use crate::state::AppState;
use crate::theme;

/// Posiciones abiertas con su P&L no realizado al último precio conocido.
pub(super) fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    // El número de posiciones va en el título para no ocupar espacio de la barra de cuenta
    let title = match &state.portfolio {
        Some(snapshot) => format!(" Posiciones ({}) ", snapshot.positions.len()),
        None => " Posiciones ".to_owned(),
    };
    let block = crate::ui::panel_block(title, false, state.is_live());

    let positions = match &state.portfolio {
        None => {
            let loading = Paragraph::new(Span::styled("Cargando…", theme::muted()));
            frame.render_widget(loading.block(block), area);
            return;
        }
        Some(snapshot) if snapshot.positions.is_empty() => {
            let empty = Paragraph::new(Span::styled("Sin posiciones abiertas", theme::muted()));
            frame.render_widget(empty.block(block), area);
            return;
        }
        Some(snapshot) => &snapshot.positions,
    };

    let header = Row::new(
        ["Símbolo", "Cant.", "Promedio", "P&L"]
            .into_iter()
            .enumerate()
            .map(|(index, title)| right_if(title, index > 0).style(theme::header())),
    );
    let rows = positions.iter().map(|position| row(position, state));
    let widths = [
        Constraint::Length(8),
        Constraint::Length(8),
        Constraint::Length(10),
        Constraint::Min(10),
    ];

    frame.render_widget(Table::new(rows, widths).header(header).block(block), area);
}

fn row(position: &Position, state: &AppState) -> Row<'static> {
    // El P&L se calcula con el precio de la watchlist si el símbolo está en ella
    let price = state
        .watchlist
        .row(position.symbol())
        .and_then(|row| row.reference_price());
    let pnl = price.and_then(|price| position.unrealized_pnl(price).ok());
    let pnl_style = pnl.map_or_else(Style::new, |pnl| {
        theme::signed(pnl.amount() > Decimal::ZERO, pnl.is_negative())
    });

    Row::new(vec![
        Cell::from(position.symbol().to_string()).style(theme::title()),
        right_if(position.quantity().to_string(), true),
        right_if(
            position
                .average_entry_price()
                .map_or_else(|| format::MISSING.to_owned(), format::decimal_price),
            true,
        ),
        right_if(
            pnl.map_or_else(|| format::MISSING.to_owned(), format::signed_money),
            true,
        )
        .style(pnl_style),
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
