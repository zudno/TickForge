use std::sync::Arc;

use futures::StreamExt;
use tickforge_application::ports::Clock;
use tickforge_application::use_cases::market::{GetMarketClockUseCase, WatchMarketUseCase};
use tickforge_application::use_cases::portfolio::GetPortfolioSnapshotUseCase;
use tickforge_domain::shared::Symbol;
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinHandle;

use crate::action::Action;

/// Efecto secundario pedido por el reducer. Se ejecuta fuera del bucle de dibujo para que
/// la interfaz nunca se bloquee esperando a la red.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    LoadPortfolio,
    LoadMarketClock,
    StartMarketStream(Vec<Symbol>),
}

/// Casos de uso que la TUI necesita. Los construye el composition root.
#[derive(Clone)]
pub struct TuiServices {
    pub portfolio: Arc<GetPortfolioSnapshotUseCase>,
    pub watch_market: Arc<WatchMarketUseCase>,
    pub market_clock: Arc<GetMarketClockUseCase>,
    pub clock: Arc<dyn Clock>,
}

/// Ejecuta comandos en tareas de tokio y devuelve sus resultados como `Action`.
pub(crate) struct CommandExecutor {
    services: TuiServices,
    actions: UnboundedSender<Action>,
    /// Tarea del flujo de mercado; solo hay una a la vez (límite de conexiones de Alpaca).
    market_stream: Option<JoinHandle<()>>,
}

impl CommandExecutor {
    pub(crate) fn new(services: TuiServices, actions: UnboundedSender<Action>) -> Self {
        Self {
            services,
            actions,
            market_stream: None,
        }
    }

    pub(crate) fn execute(&mut self, command: Command) {
        let actions = self.actions.clone();
        match command {
            Command::LoadPortfolio => {
                let use_case = Arc::clone(&self.services.portfolio);
                tokio::spawn(async move {
                    let action = match use_case.execute().await {
                        Ok(snapshot) => Action::PortfolioLoaded(snapshot),
                        Err(error) => {
                            tracing::warn!(%error, "no se pudo cargar el portafolio");
                            Action::PortfolioFailed(error.to_string())
                        }
                    };
                    // Si el receptor ya no existe, la TUI se está cerrando
                    let _ = actions.send(action);
                });
            }
            Command::LoadMarketClock => {
                let use_case = Arc::clone(&self.services.market_clock);
                tokio::spawn(async move {
                    let action = match use_case.execute().await {
                        Ok(clock) => Action::MarketClockLoaded(clock),
                        Err(error) => {
                            tracing::warn!(%error, "no se pudo consultar el reloj de mercado");
                            Action::MarketClockFailed(error.to_string())
                        }
                    };
                    let _ = actions.send(action);
                });
            }
            Command::StartMarketStream(symbols) => {
                // Cerrar el flujo anterior antes de abrir otro
                if let Some(previous) = self.market_stream.take() {
                    previous.abort();
                }
                let use_case = Arc::clone(&self.services.watch_market);
                self.market_stream =
                    Some(tokio::spawn(run_market_stream(use_case, symbols, actions)));
            }
        }
    }
}

impl Drop for CommandExecutor {
    fn drop(&mut self) {
        if let Some(stream) = self.market_stream.take() {
            stream.abort();
        }
    }
}

async fn run_market_stream(
    use_case: Arc<WatchMarketUseCase>,
    symbols: Vec<Symbol>,
    actions: UnboundedSender<Action>,
) {
    let snapshot = use_case.snapshot(&symbols).await;
    if actions.send(Action::WatchlistSeeded(snapshot)).is_err() {
        return;
    }

    let mut events = match use_case.stream(&symbols).await {
        Ok(events) => events,
        Err(error) => {
            tracing::warn!(%error, "no se pudo abrir el flujo de mercado");
            let _ = actions.send(Action::StreamDisconnected(error.to_string()));
            return;
        }
    };
    if actions.send(Action::StreamConnected).is_err() {
        return;
    }
    tracing::info!(symbols = symbols.len(), "flujo de mercado conectado");

    while let Some(event) = events.next().await {
        if actions.send(Action::Market(event)).is_err() {
            return;
        }
    }
    tracing::warn!("el flujo de mercado se cerró");
    let _ = actions.send(Action::StreamDisconnected(
        "el servidor cerró la conexión".to_owned(),
    ));
}
