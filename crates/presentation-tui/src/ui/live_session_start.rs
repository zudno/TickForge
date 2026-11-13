use ratatui::Frame;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use tickforge_domain::risk::KillSwitch;

use crate::format;
use crate::state::AppState;
use crate::theme;
use crate::ui::modal;

/// Cuántas posiciones se listan; el resto se resume en una línea.
const MAX_POSITIONS: usize = 6;

/// Modo real: lo primero que se ve al abrir es la cuenta en la que se va a operar.
pub(super) fn render(frame: &mut Frame, state: &AppState) {
    let mut lines = vec![
        Line::from(Span::styled(
            "Vas a operar con DINERO REAL. Revisa que esta sea la cuenta correcta.",
            Style::new().add_modifier(Modifier::BOLD),
        )),
        Line::default(),
    ];

    match &state.portfolio {
        None => lines.push(Line::from(Span::styled(
            "Cargando la cuenta…",
            theme::muted(),
        ))),
        Some(snapshot) => {
            let account = &snapshot.account;
            let number = account
                .number()
                .map_or_else(|| format::MISSING.to_owned(), format::masked_account);
            lines.push(detail("Cuenta", format!("{} {number}", state.broker_name)));
            lines.push(detail(
                "Equity",
                format!("{} USD", format::money(account.equity())),
            ));
            lines.push(detail(
                "Efectivo",
                format!("{} USD", format::money(account.cash())),
            ));
            lines.push(detail(
                "Poder de compra",
                format!("{} USD", format::money(account.buying_power())),
            ));
            if account.is_trading_blocked() {
                lines.push(Line::from(Span::styled(
                    "⚠ El broker tiene la operación bloqueada en esta cuenta",
                    Style::new().fg(theme::WARNING),
                )));
            }
            lines.push(Line::default());

            lines.push(detail("Posiciones", snapshot.positions.len().to_string()));
            for position in snapshot.positions.iter().take(MAX_POSITIONS) {
                lines.push(Line::from(Span::styled(
                    format!(
                        "    {:<8} {:>10} acc. · promedio {}",
                        position.symbol().as_str(),
                        position.quantity().to_string(),
                        position
                            .average_entry_price()
                            .map_or_else(|| format::MISSING.to_owned(), format::decimal_price)
                    ),
                    theme::muted(),
                )));
            }
            let hidden = snapshot.positions.len().saturating_sub(MAX_POSITIONS);
            if hidden > 0 {
                lines.push(Line::from(Span::styled(
                    format!("    … y {hidden} más"),
                    theme::muted(),
                )));
            }
            lines.push(detail(
                "Órdenes abiertas",
                snapshot.open_orders.len().to_string(),
            ));
        }
    }

    lines.push(Line::default());
    lines.extend(kill_switch_lines(&state.kill_switch));
    lines.push(Line::default());
    let hint = if state.portfolio.is_some() {
        "Enter continuar · q salir"
    } else {
        "q salir"
    };
    lines.push(Line::from(Span::styled(hint, theme::muted())));

    let danger = Style::new().fg(theme::NEGATIVE);
    let block = Block::bordered()
        .border_style(danger)
        .title(Span::styled(
            " ⚠ Sesión en cuenta real ",
            danger.add_modifier(Modifier::BOLD),
        ))
        .title(crate::ui::mode_title(true));

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

fn kill_switch_lines(kill_switch: &KillSwitch) -> Vec<Line<'static>> {
    match kill_switch {
        KillSwitch::Engaged { reason, .. } => vec![
            Line::from(Span::styled(
                format!("⛔ Órdenes nuevas BLOQUEADAS ({reason})."),
                theme::kill_switch_badge(),
            )),
            Line::from(Span::styled(
                "Cuando estés listo para operar, pulsa ! en la pantalla principal.",
                theme::muted(),
            )),
        ],
        KillSwitch::Released => vec![Line::from(Span::styled(
            "Las órdenes nuevas están permitidas.",
            Style::new().fg(theme::WARNING),
        ))],
    }
}

fn detail(label: &str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<20}"), theme::muted()),
        Span::raw(value),
    ])
}
