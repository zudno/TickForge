use std::time::Duration;

use crossterm::event::EventStream;
use futures::StreamExt;
use ratatui::DefaultTerminal;
use tickforge_domain::shared::Symbol;
use tokio::sync::mpsc;

use crate::action::Action;
use crate::command::{CommandExecutor, TuiServices};
use crate::keymap;
use crate::state::{self, AppState, TradingMode};
use crate::ui;

/// Intervalo del reloj interno: avanza la hora en pantalla y limita los redibujados
/// provocados por datos de mercado (que pueden llegar cientos de veces por segundo).
const TICK_INTERVAL: Duration = Duration::from_millis(100);

/// Configuración de la TUI que decide el composition root.
#[derive(Debug, Clone)]
pub struct TuiConfig {
    pub mode: TradingMode,
    pub broker_name: String,
    pub watchlist: Vec<Symbol>,
}

#[derive(Debug, thiserror::Error)]
pub enum TuiError {
    #[error("error de la terminal: {0}")]
    Terminal(#[from] std::io::Error),
}

/// Ejecuta la TUI hasta que el usuario salga.
///
/// Toma control de la terminal (pantalla alternativa, modo raw) y la restaura al terminar,
/// incluso si ocurre un pánico (`ratatui::try_init` instala el gancho correspondiente).
pub async fn run(services: TuiServices, config: TuiConfig) -> Result<(), TuiError> {
    let mut terminal = ratatui::try_init()?;
    let result = event_loop(&mut terminal, services, config).await;
    ratatui::try_restore()?;
    result
}

async fn event_loop(
    terminal: &mut DefaultTerminal,
    services: TuiServices,
    config: TuiConfig,
) -> Result<(), TuiError> {
    let clock = std::sync::Arc::clone(&services.clock);
    let (actions_tx, mut actions_rx) = mpsc::unbounded_channel::<Action>();
    let mut executor = CommandExecutor::new(services, actions_tx);

    let mut state = AppState::new(
        config.mode,
        config.broker_name,
        config.watchlist,
        clock.now(),
    );
    for command in state::initial_commands(&mut state) {
        executor.execute(command);
    }

    let mut input = EventStream::new();
    let mut ticker = tokio::time::interval(TICK_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut needs_redraw = true;

    loop {
        if needs_redraw {
            terminal.draw(|frame| ui::render(frame, &state))?;
            needs_redraw = false;
        }

        tokio::select! {
            maybe_event = input.next() => {
                match maybe_event {
                    Some(Ok(event)) => {
                        if let Some(action) = keymap::map_event(&event, &state) {
                            dispatch(&mut state, &mut executor, action);
                        }
                        // Teclado y cambio de tamaño se reflejan de inmediato
                        needs_redraw = true;
                    }
                    Some(Err(error)) => {
                        tracing::error!(%error, "error leyendo la terminal");
                        return Err(error.into());
                    }
                    // La terminal se cerró
                    None => state.should_quit = true,
                }
            }
            Some(action) = actions_rx.recv() => {
                dispatch(&mut state, &mut executor, action);
                // Se dibuja en el siguiente tick para agrupar ráfagas de datos
            }
            _ = ticker.tick() => {
                dispatch(&mut state, &mut executor, Action::Tick(clock.now()));
                needs_redraw = true;
            }
        }

        if state.should_quit {
            tracing::info!("el usuario cerró la TUI");
            return Ok(());
        }
    }
}

fn dispatch(state: &mut AppState, executor: &mut CommandExecutor, action: Action) {
    for command in state::reduce(state, action) {
        executor.execute(command);
    }
}
