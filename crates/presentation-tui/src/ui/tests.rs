//! Pruebas de render con `TestBackend` e `insta`: cada snapshot es el texto exacto de la
//! pantalla. Si un cambio visual es intencional, se revisa el `.snap.new` y se acepta.

use chrono::{DateTime, Utc};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tickforge_application::ports::MarketEvent;
use tickforge_application::use_cases::market::SymbolSnapshot;
use tickforge_application::use_cases::portfolio::PortfolioSnapshot;
use tickforge_domain::market::{MarketClock, Quote, TradeTick};
use tickforge_domain::portfolio::{Account, AccountBalances, Position};
use tickforge_domain::shared::{Money, Price, Quantity, Symbol};

use crate::action::Action;
use crate::state::{AppState, TradingMode, reduce};
use crate::ui::render;

fn at(rfc3339: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(rfc3339)
        .unwrap()
        .with_timezone(&Utc)
}

fn symbol(raw: &str) -> Symbol {
    Symbol::new(raw).unwrap()
}

fn price(value: Decimal) -> Price {
    Price::new(value).unwrap()
}

fn draw(state: &AppState) -> String {
    let mut terminal = Terminal::new(TestBackend::new(120, 24)).unwrap();
    terminal.draw(|frame| render(frame, state)).unwrap();
    buffer_text(terminal.backend().buffer())
}

fn buffer_text(buffer: &Buffer) -> String {
    let area = buffer.area;
    let mut text = String::new();
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            text.push_str(buffer[(x, y)].symbol());
        }
        text.push('\n');
    }
    text
}

fn loading_state() -> AppState {
    AppState::new(
        TradingMode::Paper,
        "Alpaca",
        vec![symbol("AAPL"), symbol("SPY")],
        at("2024-03-01T15:00:00Z"),
    )
}

fn loaded_state() -> AppState {
    let now = at("2024-03-01T15:00:00Z");
    let mut state = loading_state();

    reduce(
        &mut state,
        Action::PortfolioLoaded(PortfolioSnapshot {
            account: Account::new(
                AccountBalances {
                    equity: Money::usd(dec!(101250.75)),
                    cash: Money::usd(dec!(99355.75)),
                    buying_power: Money::usd(dec!(99355.75)),
                    last_equity: Money::usd(dec!(100000)),
                },
                false,
            )
            .unwrap(),
            positions: vec![
                Position::new(symbol("AAPL"), Quantity::new(dec!(10)).unwrap(), dec!(185)).unwrap(),
            ],
            open_orders: Vec::new(),
        }),
    );
    reduce(
        &mut state,
        Action::MarketClockLoaded(
            MarketClock::new(
                now,
                true,
                at("2024-03-04T14:30:00Z"),
                at("2024-03-01T21:00:00Z"),
            )
            .unwrap(),
        ),
    );
    reduce(
        &mut state,
        Action::WatchlistSeeded(vec![SymbolSnapshot {
            symbol: symbol("AAPL"),
            quote: Some(Quote::new(
                symbol("AAPL"),
                Some(price(dec!(189.10))),
                Quantity::new(dec!(1)).unwrap(),
                Some(price(dec!(189.12))),
                Quantity::new(dec!(2)).unwrap(),
                now,
            )),
            last_price: Some(price(dec!(189.11))),
            previous_close: Some(price(dec!(187.00))),
        }]),
    );
    reduce(&mut state, Action::StreamConnected);
    reduce(
        &mut state,
        Action::Market(MarketEvent::Trade(TradeTick::new(
            symbol("SPY"),
            price(dec!(512.40)),
            Quantity::new(dec!(100)).unwrap(),
            now,
        ))),
    );
    state
}

#[test]
fn renders_loading_screen() {
    insta::assert_snapshot!(draw(&loading_state()));
}

#[test]
fn renders_dashboard_with_live_data() {
    insta::assert_snapshot!(draw(&loaded_state()));
}

#[test]
fn renders_help_overlay() {
    let mut state = loaded_state();
    reduce(&mut state, Action::ToggleHelp);
    insta::assert_snapshot!(draw(&state));
}

#[test]
fn paper_badge_is_always_visible() {
    let screen = draw(&loading_state());
    assert!(screen.contains(" PAPER "));
}

/// Pantallas del flujo de órdenes.
mod orders {
    use super::*;
    use tickforge_application::use_cases::trading::OrderPreview;
    use tickforge_domain::risk::{RiskAssessment, RiskContext, RiskLimits, RiskPolicy};
    use tickforge_domain::trading::{
        ClientOrderId, Order, OrderId, OrderRequest, OrderSide, OrderType, TimeInForce,
    };

    use crate::state::{CancelConfirmation, Overlay, PreviewOverlay};

    fn limit_request() -> OrderRequest {
        OrderRequest::new(
            ClientOrderId::new("tf-1").unwrap(),
            symbol("AAPL"),
            OrderSide::Buy,
            OrderType::Limit {
                limit_price: price(dec!(185.50)),
            },
            Quantity::new(dec!(10)).unwrap(),
            TimeInForce::Day,
        )
        .unwrap()
    }

    fn open_order() -> Order {
        let mut order = Order::new(
            OrderId::new("ord-1").unwrap(),
            limit_request(),
            at("2024-03-01T15:00:00Z"),
        );
        order.mark_accepted(at("2024-03-01T15:00:00Z")).unwrap();
        order
    }

    fn with_open_order() -> AppState {
        let mut state = loaded_state();
        if let Some(portfolio) = state.portfolio.as_mut() {
            portfolio.open_orders = vec![open_order()];
        }
        state
    }

    fn preview(assessment: RiskAssessment) -> PreviewOverlay {
        let mut state = loaded_state();
        reduce(&mut state, Action::OpenTicket(OrderSide::Buy));
        let Overlay::Ticket(ticket) = state.overlay else {
            panic!("se esperaba el ticket");
        };
        PreviewOverlay {
            preview: OrderPreview {
                request: limit_request(),
                reference_price: Some(price(dec!(185.50))),
                estimated_notional: Some(Money::usd(dec!(1855))),
                buying_power: Money::usd(dec!(99355.75)),
                position_quantity: Quantity::new(dec!(10)).unwrap(),
                assessment,
            },
            ticket,
            submitting: false,
            error: None,
        }
    }

    /// Evaluación real de la política con un límite de 1,000 USD por orden.
    fn blocked_assessment() -> RiskAssessment {
        let state = loaded_state();
        let quote = Quote::new(
            symbol("AAPL"),
            Some(price(dec!(189.10))),
            Quantity::new(dec!(1)).unwrap(),
            Some(price(dec!(189.12))),
            Quantity::new(dec!(1)).unwrap(),
            at("2024-03-01T15:00:00Z"),
        );
        let policy = RiskPolicy::new(
            RiskLimits::new(
                Money::usd(dec!(1000)),
                Money::usd(dec!(50000)),
                dec!(5),
                chrono::TimeDelta::seconds(15),
            )
            .unwrap(),
        );
        policy
            .evaluate(&RiskContext {
                request: &limit_request(),
                account: &state.portfolio.as_ref().unwrap().account,
                position: None,
                quote: &quote,
                now: at("2024-03-01T15:00:00Z"),
            })
            .unwrap()
    }

    #[test]
    fn renders_order_ticket_with_limit_price() {
        let mut state = loaded_state();
        reduce(&mut state, Action::OpenTicket(OrderSide::Buy));
        for character in "10".chars() {
            reduce(&mut state, Action::TicketInput(character));
        }
        // Tipo → Límite (se propone el ask actual)
        reduce(&mut state, Action::TicketPreviousField);
        reduce(&mut state, Action::TicketToggle);
        insta::assert_snapshot!(draw(&state));
    }

    #[test]
    fn renders_approved_order_preview() {
        let mut state = loaded_state();
        state.overlay = Overlay::Preview(preview(RiskAssessment::default()));
        insta::assert_snapshot!(draw(&state));
    }

    #[test]
    fn renders_blocked_order_preview_with_reasons() {
        let mut state = loaded_state();
        state.overlay = Overlay::Preview(preview(blocked_assessment()));
        insta::assert_snapshot!(draw(&state));
    }

    #[test]
    fn renders_open_orders_with_focus_and_cancel_confirmation() {
        let mut state = with_open_order();
        reduce(&mut state, Action::FocusNextPanel);
        insta::assert_snapshot!("open_orders_focused", draw(&state));

        state.overlay = Overlay::CancelConfirm(CancelConfirmation {
            order: open_order(),
            submitting: false,
            error: None,
        });
        insta::assert_snapshot!("cancel_confirmation", draw(&state));
    }
}

#[test]
fn renders_add_symbol_dialog_with_error() {
    let mut state = loaded_state();
    reduce(&mut state, Action::OpenAddSymbol);
    for character in "zzzz".chars() {
        reduce(&mut state, Action::AddSymbolInput(character));
    }
    reduce(&mut state, Action::Confirm);
    reduce(
        &mut state,
        Action::WatchlistChangeFailed(
            "no hay datos de mercado para ZZZZ; revisa que el símbolo exista".to_owned(),
        ),
    );
    insta::assert_snapshot!(draw(&state));
}
