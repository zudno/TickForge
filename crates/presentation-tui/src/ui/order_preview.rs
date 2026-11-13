use ratatui::Frame;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use tickforge_domain::trading::OrderSide;

use crate::format;
use crate::labels;
use crate::state::{AppState, PreviewOverlay};
use crate::theme;
use crate::ui::modal;

/// Confirmación de la orden: costo estimado, efecto en la posición y resultado del riesgo.
pub(super) fn render(frame: &mut Frame, state: &AppState, overlay: &PreviewOverlay) {
    let preview = &overlay.preview;
    let request = &preview.request;
    let side_style = theme::side(request.side());

    let position_after = match request.side() {
        OrderSide::Buy => preview.position_quantity.checked_add(request.quantity()),
        OrderSide::Sell => preview.position_quantity.checked_sub(request.quantity()),
    };
    let notional_label = match request.side() {
        OrderSide::Buy => "Costo estimado",
        OrderSide::Sell => "Monto estimado",
    };

    let mut lines = Vec::new();
    if state.is_live() {
        let account = state
            .portfolio
            .as_ref()
            .and_then(|snapshot| snapshot.account.number())
            .map(|number| format!(" · cuenta {}", format::masked_account(number)))
            .unwrap_or_default();
        lines.push(Line::from(Span::styled(
            format!("⚠ DINERO REAL{account}"),
            theme::live_badge(),
        )));
        lines.push(Line::default());
    }
    lines.extend([
        Line::from(Span::styled(
            format!(
                "{} {} {}",
                labels::side(request.side()),
                request.quantity(),
                request.symbol()
            ),
            side_style.add_modifier(Modifier::BOLD),
        )),
        detail(
            "Tipo",
            format!(
                "{} · {}",
                labels::order_type(request.order_type()),
                labels::time_in_force(request.time_in_force())
            ),
        ),
        detail(
            "Precio de referencia",
            format::price(preview.reference_price),
        ),
        detail(
            notional_label,
            preview.estimated_notional.map_or_else(
                || format::MISSING.to_owned(),
                |n| format!("{} USD", format::money(n)),
            ),
        ),
        detail(
            "Poder de compra",
            format!("{} USD", format::money(preview.buying_power)),
        ),
        detail(
            "Posición",
            format!(
                "{} → {}",
                preview.position_quantity,
                position_after.map_or_else(|_| format::MISSING.to_owned(), |q| q.to_string())
            ),
        ),
        Line::default(),
    ]);

    if preview.assessment.is_approved() {
        lines.push(Line::from(Span::styled(
            "✓ Cumple todas las validaciones de riesgo",
            Style::new().fg(theme::POSITIVE),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "✕ Bloqueada por la política de riesgo:",
            Style::new()
                .fg(theme::NEGATIVE)
                .add_modifier(Modifier::BOLD),
        )));
        for violation in preview.assessment.violations() {
            lines.push(Line::from(Span::styled(
                format!("  • {violation}"),
                Style::new().fg(theme::NEGATIVE),
            )));
        }
    }

    if state.market_clock.is_some_and(|clock| !clock.is_open()) {
        lines.push(Line::from(Span::styled(
            "⚠ Mercado cerrado: la orden esperará a la apertura",
            Style::new().fg(theme::WARNING),
        )));
    }
    if let Some(error) = &overlay.error {
        lines.push(Line::from(Span::styled(
            format!("✕ {error}"),
            Style::new().fg(theme::NEGATIVE),
        )));
    }
    if let Some(input) = &overlay.typed_symbol {
        lines.extend(typed_symbol_lines(request.symbol().as_str(), input));
    }

    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        hint(overlay, state.is_live()),
        theme::muted(),
    )));

    let mode = crate::ui::mode_title(state.is_live());
    let block = Block::bordered()
        .border_style(side_style)
        .title(Span::styled(
            " Confirmar orden ",
            side_style.add_modifier(Modifier::BOLD),
        ))
        .title(mode);

    // Los motivos de un rechazo pueden ser largos: se ajustan por palabras, nunca se cortan
    let width = 72;
    let area = modal::centered(frame.area(), width, modal::wrapped_height(&lines, width));
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(block),
        area,
    );
}

fn detail(label: &str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<22}"), theme::muted()),
        Span::raw(value),
    ])
}

/// Segunda confirmación del modo real: escribir el símbolo.
fn typed_symbol_lines(symbol: &str, input: &crate::state::SymbolInput) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::default(),
        Line::from(vec![
            Span::raw("Para enviar con dinero real, escribe "),
            Span::styled(symbol.to_owned(), Style::new().add_modifier(Modifier::BOLD)),
            Span::raw(":"),
        ]),
        Line::from(vec![
            Span::styled("› ", theme::title()),
            Span::styled(
                input.text.clone(),
                Style::new().add_modifier(Modifier::BOLD),
            ),
            Span::styled("_", theme::muted()),
        ]),
    ];
    if let Some(error) = &input.error {
        lines.push(Line::from(Span::styled(
            format!("✕ {error}"),
            Style::new().fg(theme::NEGATIVE),
        )));
    }
    lines
}

fn hint(overlay: &PreviewOverlay, live: bool) -> &'static str {
    if overlay.submitting {
        "Enviando al broker…"
    } else if !overlay.preview.assessment.is_approved() {
        "Esc para editar la orden"
    } else if overlay.typed_symbol.is_some() {
        "Enter enviar · Esc volver"
    } else if live {
        "Enter continuar · Esc editar"
    } else {
        "Enter enviar · Esc editar"
    }
}
