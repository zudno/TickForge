use chrono::{DateTime, TimeDelta, Utc};

use crate::action::Action;
use crate::command::Command;
use crate::state::{AppState, NotificationLevel, StreamStatus};

/// Cada cuánto se refresca el portafolio por REST (posiciones, saldos).
const PORTFOLIO_REFRESH: TimeDelta = TimeDelta::seconds(30);

/// Cada cuánto se consulta el reloj de mercado.
const CLOCK_REFRESH: TimeDelta = TimeDelta::seconds(60);

/// Comandos a ejecutar al arrancar.
pub fn initial_commands(state: &mut AppState) -> Vec<Command> {
    state.last_portfolio_request = Some(state.now);
    state.last_clock_request = Some(state.now);
    vec![
        Command::LoadPortfolio,
        Command::LoadMarketClock,
        Command::StartMarketStream(state.watchlist.symbols()),
    ]
}

/// Aplica una acción al estado y devuelve los efectos secundarios a ejecutar.
///
/// Es una función pura respecto al mundo exterior: no hace I/O ni lee el reloj, lo que
/// permite probar todo el comportamiento de la TUI sin terminal ni red.
pub fn reduce(state: &mut AppState, action: Action) -> Vec<Command> {
    let now = state.now;
    match action {
        Action::Quit => state.should_quit = true,
        Action::SelectNext => state.watchlist.select_next(),
        Action::SelectPrevious => state.watchlist.select_previous(),
        Action::ToggleHelp => state.show_help = !state.show_help,
        Action::CloseOverlay => state.show_help = false,
        Action::Refresh => return refresh(state),

        Action::Tick(instant) => return tick(state, instant),

        Action::WatchlistSeeded(snapshots) => {
            for snapshot in snapshots {
                state.watchlist.apply_snapshot(snapshot);
            }
        }
        Action::Market(event) => {
            state.watchlist.apply_event(&event);
        }
        Action::StreamConnected => {
            state.stream_status = StreamStatus::Live;
            state.notifications.push(
                now,
                NotificationLevel::Info,
                "Datos en tiempo real conectados",
            );
        }
        Action::StreamDisconnected(reason) => {
            state.notifications.push(
                now,
                NotificationLevel::Warning,
                format!("Datos en tiempo real desconectados: {reason}. Pulsa r para reconectar"),
            );
            state.stream_status = StreamStatus::Disconnected(reason);
        }

        Action::PortfolioLoaded(snapshot) => state.portfolio = Some(snapshot),
        Action::PortfolioFailed(reason) => state.notifications.push(
            now,
            NotificationLevel::Error,
            format!("No se pudo cargar la cuenta: {reason}"),
        ),
        Action::MarketClockLoaded(clock) => state.market_clock = Some(clock),
        Action::MarketClockFailed(reason) => state.notifications.push(
            now,
            NotificationLevel::Warning,
            format!("No se pudo consultar el horario del mercado: {reason}"),
        ),
    }
    Vec::new()
}

fn refresh(state: &mut AppState) -> Vec<Command> {
    state.last_portfolio_request = Some(state.now);
    state.last_clock_request = Some(state.now);
    let mut commands = vec![Command::LoadPortfolio, Command::LoadMarketClock];

    if matches!(state.stream_status, StreamStatus::Disconnected(_)) {
        state.stream_status = StreamStatus::Connecting;
        commands.push(Command::StartMarketStream(state.watchlist.symbols()));
    }
    state
        .notifications
        .push(state.now, NotificationLevel::Info, "Actualizando…");
    commands
}

/// Avanza el reloj y programa los refrescos periódicos que correspondan.
fn tick(state: &mut AppState, instant: DateTime<Utc>) -> Vec<Command> {
    state.now = instant;
    let mut commands = Vec::new();

    if is_due(state.last_portfolio_request, instant, PORTFOLIO_REFRESH) {
        state.last_portfolio_request = Some(instant);
        commands.push(Command::LoadPortfolio);
    }

    // El reloj se consulta periódicamente y también justo después de abrir o cerrar el mercado
    let session_changed = state
        .market_clock
        .is_some_and(|clock| clock.time_until_next_change() <= instant - clock.as_of());
    let clock_due = is_due(state.last_clock_request, instant, CLOCK_REFRESH);
    if clock_due
        || (session_changed && is_due(state.last_clock_request, instant, TimeDelta::seconds(5)))
    {
        state.last_clock_request = Some(instant);
        commands.push(Command::LoadMarketClock);
    }
    commands
}

fn is_due(last: Option<DateTime<Utc>>, now: DateTime<Utc>, interval: TimeDelta) -> bool {
    last.is_none_or(|last| now.signed_duration_since(last) >= interval)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::TradingMode;
    use tickforge_domain::shared::Symbol;

    fn at(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(seconds, 0).unwrap()
    }

    fn state() -> AppState {
        AppState::new(
            TradingMode::Paper,
            "Alpaca",
            vec![Symbol::new("AAPL").unwrap(), Symbol::new("SPY").unwrap()],
            at(1_000),
        )
    }

    #[test]
    fn startup_loads_everything_once() {
        let mut state = state();
        let commands = initial_commands(&mut state);

        assert_eq!(commands.len(), 3);
        assert!(matches!(&commands[2], Command::StartMarketStream(symbols) if symbols.len() == 2));
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
        assert_eq!(state.now, at(1_060));
    }

    #[test]
    fn refresh_reconnects_only_a_disconnected_stream() {
        let mut state = state();
        state.stream_status = StreamStatus::Live;
        assert_eq!(reduce(&mut state, Action::Refresh).len(), 2);

        reduce(&mut state, Action::StreamDisconnected("timeout".to_owned()));
        let commands = reduce(&mut state, Action::Refresh);
        assert!(
            commands
                .iter()
                .any(|c| matches!(c, Command::StartMarketStream(_)))
        );
        assert_eq!(state.stream_status, StreamStatus::Connecting);
    }

    #[test]
    fn disconnection_is_reported_to_the_user() {
        let mut state = state();
        reduce(&mut state, Action::StreamDisconnected("timeout".to_owned()));

        let latest = state.notifications.latest().unwrap();
        assert_eq!(latest.level, NotificationLevel::Warning);
        assert!(latest.message.contains("Pulsa r"));
    }

    #[test]
    fn help_toggles_and_escape_closes_it() {
        let mut state = state();
        reduce(&mut state, Action::ToggleHelp);
        assert!(state.show_help);
        reduce(&mut state, Action::CloseOverlay);
        assert!(!state.show_help);
    }

    #[test]
    fn quit_sets_the_exit_flag() {
        let mut state = state();
        reduce(&mut state, Action::Quit);
        assert!(state.should_quit);
    }
}
