use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use rust_decimal::Decimal;

use crate::format;
use crate::state::AppState;
use crate::theme;

/// Resumen de la cuenta: equity, efectivo, poder de compra y P&L del día.
pub(super) fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let block = Block::bordered()
        .title(Span::styled(" TickForge ", theme::title()))
        .title(Span::styled(
            format!(" {} ", state.broker_name),
            theme::muted(),
        ));

    let line = match &state.portfolio {
        None => Line::from(Span::styled("Cargando cuenta…", theme::muted())),
        Some(snapshot) => {
            let account = &snapshot.account;
            let day_pnl = account.day_pnl().ok();
            let day_pnl_pct = account.day_pnl_pct().ok().flatten();
            let pnl_style = day_pnl.map_or_else(ratatui::style::Style::new, |pnl| {
                theme::signed(pnl.amount() > Decimal::ZERO, pnl.is_negative())
            });

            Line::from(vec![
                label("Equity "),
                Span::raw(format::money(account.equity())),
                separator(),
                label("Efectivo "),
                Span::raw(format::money(account.cash())),
                separator(),
                label("Poder de compra "),
                Span::raw(format::money(account.buying_power())),
                separator(),
                label("P&L día "),
                Span::styled(
                    format!(
                        "{} ({})",
                        day_pnl.map_or_else(|| format::MISSING.to_owned(), format::signed_money),
                        format::percent(day_pnl_pct)
                    ),
                    pnl_style,
                ),
            ])
        }
    };

    frame.render_widget(Paragraph::new(line).block(block), area);
}

fn label(text: &'static str) -> Span<'static> {
    Span::styled(text, theme::muted())
}

fn separator() -> Span<'static> {
    Span::styled(" │ ", theme::muted())
}
