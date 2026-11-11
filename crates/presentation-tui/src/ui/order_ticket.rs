use ratatui::Frame;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use tickforge_domain::trading::OrderSide;

use crate::format;
use crate::labels;
use crate::state::{OrderTicket, TicketField, TicketOrderType};
use crate::theme;
use crate::ui::modal;

/// Formulario de orden nueva.
pub(super) fn render(frame: &mut Frame, ticket: &OrderTicket) {
    let mut lines = vec![
        Line::from(vec![
            Span::styled("Bid ", theme::muted()),
            Span::raw(format::price(ticket.bid)),
            Span::styled("   Ask ", theme::muted()),
            Span::raw(format::price(ticket.ask)),
        ]),
        Line::default(),
    ];
    lines.extend(
        ticket
            .fields()
            .into_iter()
            .map(|field| field_line(ticket, field)),
    );
    lines.push(Line::default());
    lines.push(status_line(ticket));
    lines.push(Line::from(Span::styled(
        "Tab campo · ←/→ cambiar · Enter revisar · Esc cancelar",
        theme::muted(),
    )));

    let side_style = theme::side(ticket.side);
    let block = Block::bordered()
        .border_style(side_style)
        .title(Span::styled(
            format!(" Nueva orden · {} ", ticket.symbol),
            side_style.add_modifier(Modifier::BOLD),
        ));

    let area = modal::centered(frame.area(), 60, modal::height_for(lines.len()));
    frame.render_widget(Clear, area);
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn field_line(ticket: &OrderTicket, field: TicketField) -> Line<'static> {
    let focused = ticket.focused == field;
    let marker = if focused { "▸ " } else { "  " };
    let label = match field {
        TicketField::Side => "Lado",
        TicketField::OrderType => "Tipo",
        TicketField::Quantity => "Cantidad",
        TicketField::LimitPrice => "Límite",
        TicketField::TimeInForce => "Vigencia",
    };

    let value = match field {
        TicketField::Side => choice(labels::side(ticket.side), focused)
            .patch_style(theme::side(ticket.side).add_modifier(Modifier::BOLD)),
        TicketField::OrderType => choice(
            match ticket.order_type {
                TicketOrderType::Market => "Mercado",
                TicketOrderType::Limit => "Límite",
            },
            focused,
        ),
        TicketField::TimeInForce => choice(labels::time_in_force(ticket.time_in_force), focused),
        TicketField::Quantity => text_input(&ticket.quantity, focused),
        TicketField::LimitPrice => text_input(&ticket.limit_price, focused),
    };

    let mut spans = vec![
        Span::styled(marker, theme::title()),
        Span::styled(format!("{label:<10}"), theme::muted()),
    ];
    spans.extend(value.spans);
    Line::from(spans)
}

/// Opción alternable: muestra flechas cuando tiene el foco.
fn choice(text: &str, focused: bool) -> Line<'static> {
    if focused {
        Line::from(format!("◂ {text} ▸"))
    } else {
        Line::from(text.to_owned())
    }
}

/// Campo de texto con cursor cuando tiene el foco.
fn text_input(text: &str, focused: bool) -> Line<'static> {
    let mut spans = Vec::new();
    if text.is_empty() && !focused {
        spans.push(Span::styled("—", theme::muted()));
    } else {
        spans.push(Span::styled(
            text.to_owned(),
            Style::new().add_modifier(Modifier::BOLD),
        ));
    }
    if focused {
        spans.push(Span::styled("█", theme::title()));
    }
    Line::from(spans)
}

fn status_line(ticket: &OrderTicket) -> Line<'static> {
    if ticket.awaiting_preview {
        Line::from(Span::styled(
            "Calculando costo y validando riesgo…",
            theme::muted(),
        ))
    } else if let Some(error) = &ticket.error {
        Line::from(Span::styled(
            error.clone(),
            Style::new().fg(theme::NEGATIVE),
        ))
    } else {
        let hint = match ticket.side {
            OrderSide::Buy => "Revisarás el costo antes de enviar",
            OrderSide::Sell => "Revisarás el monto antes de enviar",
        };
        Line::from(Span::styled(hint, theme::muted()))
    }
}
