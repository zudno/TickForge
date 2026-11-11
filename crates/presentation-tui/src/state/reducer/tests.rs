use chrono::{DateTime, Utc};
use rust_decimal_macros::dec;
use tickforge_application::ports::{OrderUpdate, OrderUpdateKind};
use tickforge_application::use_cases::market::SymbolSnapshot;
use tickforge_application::use_cases::portfolio::PortfolioSnapshot;
use tickforge_application::use_cases::trading::OrderPreview;
use tickforge_domain::market::Quote;
use tickforge_domain::portfolio::{Account, AccountBalances};
use tickforge_domain::risk::{RiskAssessment, RiskViolation};
use tickforge_domain::shared::{Money, Price, Quantity, Symbol};
use tickforge_domain::trading::{
    ClientOrderId, Fill, FillId, Order, OrderId, OrderRequest, OrderSide, OrderType, TimeInForce,
};

use super::*;
use crate::state::{TicketField, TradingMode};

fn at(seconds: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(seconds, 0).unwrap()
}

fn aapl() -> Symbol {
    Symbol::new("AAPL").unwrap()
}

fn state() -> AppState {
    let mut state = AppState::new(
        TradingMode::Paper,
        "Alpaca",
        vec![aapl(), Symbol::new("SPY").unwrap()],
        at(1_000),
    );
    reduce(
        &mut state,
        Action::WatchlistSeeded(vec![SymbolSnapshot {
            symbol: aapl(),
            quote: Some(Quote::new(
                aapl(),
                Some(Price::new(dec!(99.99)).unwrap()),
                Quantity::new(dec!(1)).unwrap(),
                Some(Price::new(dec!(100)).unwrap()),
                Quantity::new(dec!(1)).unwrap(),
                at(1_000),
            )),
            last_price: None,
            previous_close: None,
        }]),
    );
    state
}

fn request() -> OrderRequest {
    OrderRequest::new(
        ClientOrderId::new("tf-1").unwrap(),
        aapl(),
        OrderSide::Buy,
        OrderType::Limit {
            limit_price: Price::new(dec!(95)).unwrap(),
        },
        Quantity::new(dec!(10)).unwrap(),
        TimeInForce::Day,
    )
    .unwrap()
}

fn open_order() -> Order {
    let mut order = Order::new(OrderId::new("ord-1").unwrap(), request(), at(1_000));
    order.mark_accepted(at(1_000)).unwrap();
    order
}

fn preview(assessment: RiskAssessment) -> OrderPreview {
    OrderPreview {
        request: request(),
        reference_price: Some(Price::new(dec!(95)).unwrap()),
        estimated_notional: Some(Money::usd(dec!(950))),
        buying_power: Money::usd(dec!(10000)),
        position_quantity: Quantity::ZERO,
        assessment,
    }
}

fn portfolio_with(orders: Vec<Order>) -> PortfolioSnapshot {
    PortfolioSnapshot {
        account: Account::new(
            AccountBalances {
                equity: Money::usd(dec!(10000)),
                cash: Money::usd(dec!(10000)),
                buying_power: Money::usd(dec!(10000)),
                last_equity: Money::usd(dec!(10000)),
            },
            false,
        )
        .unwrap(),
        positions: Vec::new(),
        open_orders: orders,
    }
}

/// Abre el ticket de compra, escribe la cantidad y pide la vista previa.
fn ticket_awaiting_preview(state: &mut AppState) -> Vec<Command> {
    reduce(state, Action::OpenTicket(OrderSide::Buy));
    for character in "10".chars() {
        reduce(state, Action::TicketInput(character));
    }
    reduce(state, Action::Confirm)
}

#[test]
fn startup_opens_both_streams() {
    let mut state = state();
    let commands = initial_commands(&mut state);

    assert_eq!(commands.len(), 6);
    assert!(commands.contains(&Command::StartOrderStream));
}

#[test]
fn ticks_schedule_periodic_refreshes() {
    let mut state = state();
    initial_commands(&mut state);

    assert!(reduce(&mut state, Action::Tick(at(1_010))).is_empty());
    assert_eq!(
        reduce(&mut state, Action::Tick(at(1_030))),
        vec![Command::LoadPortfolio]
    );
    assert_eq!(
        reduce(&mut state, Action::Tick(at(1_060))),
        vec![Command::LoadPortfolio, Command::LoadMarketClock]
    );
}

#[test]
fn ticket_opens_for_the_selected_symbol_with_its_quote() {
    let mut state = state();
    reduce(&mut state, Action::OpenTicket(OrderSide::Sell));

    let Overlay::Ticket(ticket) = &state.overlay else {
        panic!("se esperaba el ticket");
    };
    assert_eq!(ticket.symbol, aapl());
    assert_eq!(ticket.side, OrderSide::Sell);
    assert_eq!(ticket.focused, TicketField::Quantity);
    assert!(ticket.ask.is_some());
}

#[test]
fn invalid_ticket_shows_an_error_and_sends_nothing() {
    let mut state = state();
    reduce(&mut state, Action::OpenTicket(OrderSide::Buy));

    assert!(reduce(&mut state, Action::Confirm).is_empty());
    let Overlay::Ticket(ticket) = &state.overlay else {
        panic!("se esperaba el ticket");
    };
    assert_eq!(
        ticket.error.as_deref(),
        Some("Escribe la cantidad de acciones")
    );
}

#[test]
fn full_order_flow_from_ticket_to_submission() {
    let mut state = state();

    // Ticket → vista previa
    let commands = ticket_awaiting_preview(&mut state);
    assert!(
        matches!(commands.as_slice(), [Command::PreviewOrder(c)] if c.quantity == Quantity::new(dec!(10)).unwrap())
    );

    // Llega la vista previa aprobada
    reduce(
        &mut state,
        Action::OrderPreviewReady(preview(RiskAssessment::default())),
    );
    assert!(matches!(state.overlay, Overlay::Preview(_)));

    // Enter envía exactamente la solicitud evaluada
    let commands = reduce(&mut state, Action::Confirm);
    assert_eq!(commands, vec![Command::SubmitOrder(request())]);

    // Un segundo Enter mientras se espera no envía otra vez
    assert!(reduce(&mut state, Action::Confirm).is_empty());

    // El broker confirma: se cierra la ventana y se refresca la cuenta
    let commands = reduce(&mut state, Action::OrderSubmitted(open_order()));
    assert_eq!(state.overlay, Overlay::None);
    assert_eq!(commands, vec![Command::LoadPortfolio]);
    assert!(
        state
            .notifications
            .latest()
            .unwrap()
            .message
            .starts_with("Orden enviada: COMPRA 10 AAPL")
    );
}

#[test]
fn a_preview_with_violations_cannot_be_submitted() {
    let mut state = state();
    ticket_awaiting_preview(&mut state);
    reduce(
        &mut state,
        Action::OrderPreviewReady(preview(rejected_assessment())),
    );

    assert!(reduce(&mut state, Action::Confirm).is_empty());
    assert!(matches!(state.overlay, Overlay::Preview(_)));
}

/// Evaluación rechazada por la política real (solo ella crea violaciones): límites de 100 USD
/// para una orden de 950 USD.
fn rejected_assessment() -> RiskAssessment {
    use tickforge_domain::portfolio::Position;
    use tickforge_domain::risk::{RiskContext, RiskLimits, RiskPolicy};

    let policy = RiskPolicy::new(
        RiskLimits::new(
            Money::usd(dec!(100)),
            Money::usd(dec!(100)),
            dec!(5),
            chrono::TimeDelta::seconds(5),
        )
        .unwrap(),
    );
    let account = portfolio_with(Vec::new()).account;
    let quote = Quote::new(
        aapl(),
        Some(Price::new(dec!(95)).unwrap()),
        Quantity::new(dec!(1)).unwrap(),
        Some(Price::new(dec!(95)).unwrap()),
        Quantity::new(dec!(1)).unwrap(),
        at(1_000),
    );
    let assessment = policy
        .evaluate(&RiskContext {
            request: &request(),
            account: &account,
            position: None::<&Position>,
            quote: &quote,
            now: at(1_000),
            kill_switch: &tickforge_domain::risk::KillSwitch::Released,
        })
        .unwrap();
    assert!(
        assessment
            .violations()
            .iter()
            .any(|v| matches!(v, RiskViolation::OrderNotionalExceeded { .. }))
    );
    assessment
}

#[test]
fn escape_from_the_preview_returns_to_the_ticket_for_editing() {
    let mut state = state();
    ticket_awaiting_preview(&mut state);
    reduce(
        &mut state,
        Action::OrderPreviewReady(preview(RiskAssessment::default())),
    );

    reduce(&mut state, Action::CloseOverlay);
    let Overlay::Ticket(ticket) = &state.overlay else {
        panic!("se esperaba volver al ticket");
    };
    assert_eq!(ticket.quantity, "10");
    assert!(!ticket.awaiting_preview);
}

#[test]
fn late_preview_after_closing_the_ticket_is_ignored() {
    let mut state = state();
    ticket_awaiting_preview(&mut state);
    // El ticket está esperando: Esc no lo cierra
    reduce(&mut state, Action::CloseOverlay);
    assert!(matches!(state.overlay, Overlay::Ticket(_)));

    // Si el usuario sale con otro flujo (por ejemplo, el ticket ya no existe), se ignora
    state.overlay = Overlay::None;
    reduce(
        &mut state,
        Action::OrderPreviewReady(preview(RiskAssessment::default())),
    );
    assert_eq!(state.overlay, Overlay::None);
}

#[test]
fn submit_failure_keeps_the_preview_with_the_reason() {
    let mut state = state();
    ticket_awaiting_preview(&mut state);
    reduce(
        &mut state,
        Action::OrderPreviewReady(preview(RiskAssessment::default())),
    );
    reduce(&mut state, Action::Confirm);

    reduce(
        &mut state,
        Action::OrderSubmitFailed("el broker no está disponible".to_owned()),
    );
    let Overlay::Preview(preview) = &state.overlay else {
        panic!("se esperaba la vista previa");
    };
    assert!(!preview.submitting);
    assert_eq!(
        preview.error.as_deref(),
        Some("el broker no está disponible")
    );
}

#[test]
fn cancel_flow_from_the_orders_panel() {
    let mut state = state();
    reduce(
        &mut state,
        Action::PortfolioLoaded(portfolio_with(vec![open_order()])),
    );

    // `c` solo actúa con el panel de órdenes enfocado
    reduce(&mut state, Action::RequestCancelSelected);
    assert_eq!(state.overlay, Overlay::None);

    reduce(&mut state, Action::FocusNextPanel);
    reduce(&mut state, Action::RequestCancelSelected);
    assert!(matches!(state.overlay, Overlay::CancelConfirm(_)));

    let commands = reduce(&mut state, Action::Confirm);
    assert_eq!(
        commands,
        vec![Command::CancelOrder(OrderId::new("ord-1").unwrap())]
    );

    let commands = reduce(
        &mut state,
        Action::OrderCancelRequested(OrderId::new("ord-1").unwrap()),
    );
    assert_eq!(state.overlay, Overlay::None);
    assert_eq!(commands, vec![Command::LoadPortfolio]);
}

#[test]
fn order_fills_are_announced_and_refresh_the_account_once_per_burst() {
    let mut state = state();
    initial_commands(&mut state);
    reduce(
        &mut state,
        Action::PortfolioLoaded(portfolio_with(vec![open_order()])),
    );
    reduce(&mut state, Action::Tick(at(1_005)));

    let fill = |id: &str| {
        Action::OrderUpdateReceived(OrderUpdate {
            order_id: OrderId::new("ord-1").unwrap(),
            kind: OrderUpdateKind::Filled(
                Fill::new(
                    FillId::new(id).unwrap(),
                    Quantity::new(dec!(5)).unwrap(),
                    Price::new(dec!(95)).unwrap(),
                    at(1_005),
                )
                .unwrap(),
            ),
            occurred_at: at(1_005),
        })
    };

    assert_eq!(reduce(&mut state, fill("e1")), vec![Command::LoadPortfolio]);
    // Segundo fill en el mismo segundo: se agrupa con el anterior
    assert!(reduce(&mut state, fill("e2")).is_empty());
    assert!(
        state
            .notifications
            .latest()
            .unwrap()
            .message
            .contains("Ejecución: 5 acciones a 95.00 (COMPRA 10 AAPL")
    );
    // El refresco pendiente sale en el siguiente tick, pasado el intervalo mínimo
    assert_eq!(
        reduce(&mut state, Action::Tick(at(1_007))),
        vec![Command::LoadPortfolio]
    );
}

#[test]
fn refresh_reconnects_only_the_streams_that_are_down() {
    let mut state = state();
    state.stream_status = StreamStatus::Live;
    reduce(
        &mut state,
        Action::OrderStreamDisconnected("timeout".to_owned()),
    );

    let commands = reduce(&mut state, Action::Refresh);
    assert!(commands.contains(&Command::StartOrderStream));
    assert!(
        !commands
            .iter()
            .any(|c| matches!(c, Command::StartMarketStream(_)))
    );
}

#[test]
fn help_toggles_and_escape_closes_it() {
    let mut state = state();
    reduce(&mut state, Action::ToggleHelp);
    assert_eq!(state.overlay, Overlay::Help);
    reduce(&mut state, Action::CloseOverlay);
    assert_eq!(state.overlay, Overlay::None);
}

#[test]
fn quit_sets_the_exit_flag() {
    let mut state = state();
    reduce(&mut state, Action::Quit);
    assert!(state.should_quit);
}

// --- Watchlist y reconciliación ---

fn report(
    recovered: Vec<Order>,
    not_submitted: usize,
) -> tickforge_application::use_cases::trading::ReconciliationReport {
    tickforge_application::use_cases::trading::ReconciliationReport {
        checked: recovered.len() + not_submitted,
        recovered,
        not_submitted: (0..not_submitted)
            .map(|index| ClientOrderId::new(&format!("tf-{index}")).unwrap())
            .collect(),
        unresolved: Vec::new(),
    }
}

#[test]
fn startup_reconciles_before_anything_else() {
    let mut state = state();
    assert_eq!(
        initial_commands(&mut state).first(),
        Some(&Command::ReconcileOrders)
    );
}

#[test]
fn adding_a_symbol_validates_text_then_asks_the_use_case() {
    let mut state = state();
    reduce(&mut state, Action::OpenAddSymbol);

    // Vacío: no se envía nada
    assert!(reduce(&mut state, Action::Confirm).is_empty());

    for character in "tsla".chars() {
        reduce(&mut state, Action::AddSymbolInput(character));
    }
    let commands = reduce(&mut state, Action::Confirm);
    assert_eq!(
        commands,
        vec![Command::AddSymbol {
            current: vec![aapl(), Symbol::new("SPY").unwrap()],
            symbol: Symbol::new("TSLA").unwrap(),
        }]
    );

    // Confirmado por el caso de uso: se cierra, se agrega y se reabre el flujo en vivo
    let updated = vec![
        aapl(),
        Symbol::new("SPY").unwrap(),
        Symbol::new("TSLA").unwrap(),
    ];
    let commands = reduce(&mut state, Action::WatchlistChanged(updated.clone()));
    assert_eq!(state.overlay, Overlay::None);
    assert_eq!(state.watchlist.symbols(), updated);
    assert_eq!(commands, vec![Command::StartMarketStream(updated)]);
}

#[test]
fn a_rejected_symbol_keeps_the_input_open_with_the_reason() {
    let mut state = state();
    reduce(&mut state, Action::OpenAddSymbol);
    for character in "zzzz".chars() {
        reduce(&mut state, Action::AddSymbolInput(character));
    }
    reduce(&mut state, Action::Confirm);

    reduce(
        &mut state,
        Action::WatchlistChangeFailed("no hay datos de mercado para ZZZZ".to_owned()),
    );
    let Overlay::AddSymbol(input) = &state.overlay else {
        panic!("se esperaba la ventana de agregar símbolo");
    };
    assert!(!input.awaiting);
    assert_eq!(input.text, "ZZZZ");
    assert!(input.error.as_deref().unwrap().contains("ZZZZ"));
}

#[test]
fn removing_requires_the_watchlist_to_be_focused() {
    let mut state = state();
    assert!(matches!(
        reduce(&mut state, Action::RemoveSelectedSymbol).as_slice(),
        [Command::RemoveSymbol { symbol, .. }] if *symbol == aapl()
    ));

    reduce(&mut state, Action::FocusNextPanel);
    assert!(reduce(&mut state, Action::RemoveSelectedSymbol).is_empty());
}

#[test]
fn reconciliation_results_are_reported_only_when_something_was_pending() {
    let mut state = state();
    reduce(
        &mut state,
        Action::ReconciliationFinished(report(Vec::new(), 0)),
    );
    assert!(state.notifications.latest().is_none());

    let commands = reduce(
        &mut state,
        Action::ReconciliationFinished(report(vec![open_order()], 1)),
    );
    assert_eq!(
        state.notifications.latest().unwrap().message,
        "Reconciliación: 1 recuperadas del broker, 1 nunca llegaron"
    );
    // Hay órdenes recuperadas: se refresca la cuenta para mostrarlas
    assert_eq!(commands, vec![Command::LoadPortfolio]);
}

// --- Reconexión y datos congelados (fase 7) ---

fn open_market(state: &mut AppState) {
    reduce(
        state,
        Action::MarketClockLoaded(
            tickforge_domain::market::MarketClock::new(at(1_000), true, at(100_000), at(20_000))
                .unwrap(),
        ),
    );
}

#[test]
fn a_lost_stream_shows_the_retry_countdown_and_warns_only_once() {
    let mut state = state();
    reduce(&mut state, Action::StreamConnected);

    reduce(&mut state, Action::StreamDisconnected("timeout".to_owned()));
    reduce(
        &mut state,
        Action::StreamReconnecting {
            attempt: 1,
            delay: std::time::Duration::from_secs(2),
        },
    );
    assert_eq!(
        state.stream_status,
        StreamStatus::Reconnecting {
            attempt: 1,
            retry_at: at(1_002)
        }
    );
    let warning = state.notifications.latest().unwrap().message.clone();
    assert!(warning.contains("Reconectando"));

    // Un intento fallido más no repite el aviso
    reduce(&mut state, Action::StreamDisconnected("timeout".to_owned()));
    assert_eq!(state.notifications.latest().unwrap().message, warning);

    reduce(&mut state, Action::StreamConnected);
    assert_eq!(state.stream_status, StreamStatus::Live);
    assert_eq!(
        state.notifications.latest().unwrap().message,
        "Datos en tiempo real reconectados"
    );
}

#[test]
fn order_stream_reconnection_resyncs_the_account() {
    let mut state = state();
    // Primera conexión: nada que sincronizar
    assert!(reduce(&mut state, Action::OrderStreamConnected).is_empty());

    reduce(
        &mut state,
        Action::OrderStreamDisconnected("cerrado".to_owned()),
    );
    // Reconexión: pudo haber ejecuciones mientras estuvo caído
    assert_eq!(
        reduce(&mut state, Action::OrderStreamConnected),
        vec![Command::LoadPortfolio]
    );
}

#[test]
fn refresh_retries_a_reconnecting_stream_immediately() {
    let mut state = state();
    state.stream_status = StreamStatus::Reconnecting {
        attempt: 4,
        retry_at: at(1_060),
    };

    let commands = reduce(&mut state, Action::Refresh);
    assert!(
        commands
            .iter()
            .any(|command| matches!(command, Command::StartMarketStream(_)))
    );
    assert_eq!(state.stream_status, StreamStatus::Connecting);
}

#[test]
fn a_silent_stream_during_market_hours_is_restarted() {
    let mut state = state();
    initial_commands(&mut state);
    open_market(&mut state);
    reduce(&mut state, Action::StreamConnected);

    // Dos minutos sin datos aún no bastan
    let commands = reduce(&mut state, Action::Tick(at(1_110)));
    assert!(
        !commands
            .iter()
            .any(|c| matches!(c, Command::StartMarketStream(_)))
    );

    let commands = reduce(&mut state, Action::Tick(at(1_121)));
    assert!(
        commands
            .iter()
            .any(|c| matches!(c, Command::StartMarketStream(_)))
    );
    assert_eq!(state.stream_status, StreamStatus::Connecting);
    assert!(
        state
            .notifications
            .latest()
            .unwrap()
            .message
            .contains("Sin datos de mercado")
    );
}

#[test]
fn silence_with_the_market_closed_is_normal() {
    let mut state = state();
    initial_commands(&mut state);
    reduce(&mut state, Action::StreamConnected);

    // Sin reloj de mercado abierto (cerrado o desconocido) no se fuerza la reconexión
    let commands = reduce(&mut state, Action::Tick(at(5_000)));
    assert!(
        !commands
            .iter()
            .any(|c| matches!(c, Command::StartMarketStream(_)))
    );
    assert_eq!(state.stream_status, StreamStatus::Live);
}

// --- Kill switch ---

fn engaged() -> tickforge_domain::risk::KillSwitch {
    tickforge_domain::risk::KillSwitch::engaged(at(1_000), "activado desde la TUI")
}

#[test]
fn startup_loads_the_saved_kill_switch() {
    let mut state = state();
    assert!(initial_commands(&mut state).contains(&Command::LoadKillSwitch));

    reduce(&mut state, Action::KillSwitchLoaded(engaged()));
    assert!(state.kill_switch.is_engaged());
    assert!(
        state
            .notifications
            .latest()
            .unwrap()
            .message
            .contains("sigue activo")
    );
}

#[test]
fn engaging_asks_for_confirmation_then_reports_the_result() {
    let mut state = state();
    reduce(
        &mut state,
        Action::PortfolioLoaded(portfolio_with(vec![open_order()])),
    );

    reduce(&mut state, Action::OpenKillSwitch);
    let Overlay::KillSwitch(dialog) = &state.overlay else {
        panic!("se esperaba la confirmación del kill switch");
    };
    assert!(dialog.engage);
    assert_eq!(dialog.open_orders, 1);

    assert_eq!(
        reduce(&mut state, Action::Confirm),
        vec![Command::EngageKillSwitch]
    );
    // Mientras se ejecuta no se puede cerrar ni repetir
    reduce(&mut state, Action::CloseOverlay);
    assert!(matches!(state.overlay, Overlay::KillSwitch(_)));
    assert!(reduce(&mut state, Action::Confirm).is_empty());

    let commands = reduce(
        &mut state,
        Action::KillSwitchEngaged {
            state: engaged(),
            report: tickforge_application::use_cases::trading::KillSwitchReport {
                canceled: vec![OrderId::new("ord-1").unwrap()],
                failed: Vec::new(),
                journal_warning: None,
            },
        },
    );
    assert_eq!(state.overlay, Overlay::None);
    assert!(state.kill_switch.is_engaged());
    assert_eq!(commands, vec![Command::LoadPortfolio]);
    assert!(
        state
            .notifications
            .latest()
            .unwrap()
            .message
            .starts_with("KILL SWITCH ACTIVO: 1 órdenes canceladas")
    );
}

#[test]
fn partial_failures_are_reported_as_errors() {
    let mut state = state();
    reduce(
        &mut state,
        Action::KillSwitchEngaged {
            state: engaged(),
            report: tickforge_application::use_cases::trading::KillSwitchReport {
                canceled: Vec::new(),
                failed: vec![(OrderId::new("ord-1").unwrap(), "timeout".to_owned())],
                journal_warning: None,
            },
        },
    );
    let latest = state.notifications.latest().unwrap();
    assert_eq!(latest.level, NotificationLevel::Error);
    assert!(latest.message.contains("no se pudieron cancelar"));
}

#[test]
fn while_engaged_tickets_do_not_open_and_bang_offers_to_release() {
    let mut state = state();
    reduce(&mut state, Action::KillSwitchLoaded(engaged()));

    reduce(&mut state, Action::OpenTicket(OrderSide::Buy));
    assert_eq!(state.overlay, Overlay::None);
    assert!(
        state
            .notifications
            .latest()
            .unwrap()
            .message
            .contains("Kill switch activo")
    );

    reduce(&mut state, Action::OpenKillSwitch);
    let Overlay::KillSwitch(dialog) = &state.overlay else {
        panic!("se esperaba la confirmación");
    };
    assert!(!dialog.engage);
    assert_eq!(
        reduce(&mut state, Action::Confirm),
        vec![Command::ReleaseKillSwitch]
    );

    reduce(&mut state, Action::KillSwitchReleased);
    assert!(!state.kill_switch.is_engaged());
    assert_eq!(state.overlay, Overlay::None);
}
