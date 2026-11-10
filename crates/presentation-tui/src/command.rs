use std::future::Future;
use std::sync::Arc;

use futures::StreamExt;
use tickforge_application::ApplicationError;
use tickforge_application::ports::Clock;
use tickforge_application::use_cases::market::{
    GetMarketClockUseCase, ManageWatchlistUseCase, WatchMarketUseCase,
};
use tickforge_application::use_cases::portfolio::GetPortfolioSnapshotUseCase;
use tickforge_application::use_cases::trading::{
    CancelOrderUseCase, PlaceOrderCommand, PlaceOrderUseCase, ReconcileOrdersUseCase,
    WatchOrderUpdatesUseCase,
};
use tickforge_domain::shared::Symbol;
use tickforge_domain::trading::{OrderId, OrderRequest};
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
    StartOrderStream,
    PreviewOrder(PlaceOrderCommand),
    SubmitOrder(OrderRequest),
    CancelOrder(OrderId),
    ReconcileOrders,
    AddSymbol {
        current: Vec<Symbol>,
        symbol: Symbol,
    },
    RemoveSymbol {
        current: Vec<Symbol>,
        symbol: Symbol,
    },
}

/// Casos de uso que la TUI necesita. Los construye el composition root.
#[derive(Clone)]
pub struct TuiServices {
    pub portfolio: Arc<GetPortfolioSnapshotUseCase>,
    pub watch_market: Arc<WatchMarketUseCase>,
    pub market_clock: Arc<GetMarketClockUseCase>,
    pub watchlist: Arc<ManageWatchlistUseCase>,
    pub place_order: Arc<PlaceOrderUseCase>,
    pub cancel_order: Arc<CancelOrderUseCase>,
    pub order_updates: Arc<WatchOrderUpdatesUseCase>,
    pub reconcile_orders: Arc<ReconcileOrdersUseCase>,
    pub clock: Arc<dyn Clock>,
}

/// Ejecuta comandos en tareas de tokio y devuelve sus resultados como `Action`.
pub(crate) struct CommandExecutor {
    services: TuiServices,
    actions: UnboundedSender<Action>,
    /// Tarea del flujo de mercado; solo hay una a la vez (límite de conexiones de Alpaca).
    market_stream: Option<JoinHandle<()>>,
    /// Tarea del flujo de actualizaciones de órdenes; también una sola.
    order_stream: Option<JoinHandle<()>>,
}

impl CommandExecutor {
    pub(crate) fn new(services: TuiServices, actions: UnboundedSender<Action>) -> Self {
        Self {
            services,
            actions,
            market_stream: None,
            order_stream: None,
        }
    }

    pub(crate) fn execute(&mut self, command: Command) {
        let services = &self.services;
        match command {
            Command::LoadPortfolio => {
                let use_case = Arc::clone(&services.portfolio);
                self.spawn(async move {
                    match use_case.execute().await {
                        Ok(snapshot) => Action::PortfolioLoaded(snapshot),
                        Err(error) => {
                            tracing::warn!(%error, "no se pudo cargar el portafolio");
                            Action::PortfolioFailed(describe(&error))
                        }
                    }
                });
            }
            Command::LoadMarketClock => {
                let use_case = Arc::clone(&services.market_clock);
                self.spawn(async move {
                    match use_case.execute().await {
                        Ok(clock) => Action::MarketClockLoaded(clock),
                        Err(error) => {
                            tracing::warn!(%error, "no se pudo consultar el reloj de mercado");
                            Action::MarketClockFailed(describe(&error))
                        }
                    }
                });
            }
            Command::PreviewOrder(command) => {
                let use_case = Arc::clone(&services.place_order);
                self.spawn(async move {
                    match use_case.preview(command).await {
                        Ok(preview) => Action::OrderPreviewReady(preview),
                        Err(error) => Action::OrderPreviewFailed(describe(&error)),
                    }
                });
            }
            Command::SubmitOrder(request) => {
                let use_case = Arc::clone(&services.place_order);
                self.spawn(async move {
                    match use_case.submit(request).await {
                        Ok(order) => {
                            tracing::info!(order_id = %order.id(), "orden enviada");
                            Action::OrderSubmitted(order)
                        }
                        Err(error) => {
                            tracing::warn!(%error, "no se pudo enviar la orden");
                            Action::OrderSubmitFailed(describe(&error))
                        }
                    }
                });
            }
            Command::CancelOrder(order_id) => {
                let use_case = Arc::clone(&services.cancel_order);
                self.spawn(async move {
                    match use_case.execute(&order_id).await {
                        Ok(()) => Action::OrderCancelRequested(order_id),
                        Err(error) => {
                            tracing::warn!(%error, %order_id, "no se pudo cancelar la orden");
                            Action::OrderCancelFailed(describe(&error))
                        }
                    }
                });
            }
            Command::ReconcileOrders => {
                let use_case = Arc::clone(&services.reconcile_orders);
                self.spawn(async move {
                    match use_case.execute().await {
                        Ok(report) => {
                            tracing::info!(
                                checked = report.checked,
                                recovered = report.recovered.len(),
                                not_submitted = report.not_submitted.len(),
                                unresolved = report.unresolved.len(),
                                "reconciliación terminada"
                            );
                            Action::ReconciliationFinished(report)
                        }
                        Err(error) => {
                            tracing::warn!(%error, "no se pudo reconciliar el diario");
                            Action::ReconciliationFailed(describe(&error))
                        }
                    }
                });
            }
            Command::AddSymbol { current, symbol } => {
                let use_case = Arc::clone(&services.watchlist);
                self.spawn(async move {
                    match use_case.add(&current, symbol).await {
                        Ok(symbols) => Action::WatchlistChanged(symbols),
                        Err(error) => Action::WatchlistChangeFailed(describe(&error)),
                    }
                });
            }
            Command::RemoveSymbol { current, symbol } => {
                let use_case = Arc::clone(&services.watchlist);
                self.spawn(async move {
                    match use_case.remove(&current, &symbol).await {
                        Ok(symbols) => Action::WatchlistChanged(symbols),
                        Err(error) => Action::WatchlistChangeFailed(describe(&error)),
                    }
                });
            }
            Command::StartMarketStream(symbols) => {
                abort(&mut self.market_stream);
                let use_case = Arc::clone(&services.watch_market);
                self.market_stream = Some(tokio::spawn(run_market_stream(
                    use_case,
                    symbols,
                    self.actions.clone(),
                )));
            }
            Command::StartOrderStream => {
                abort(&mut self.order_stream);
                let use_case = Arc::clone(&services.order_updates);
                self.order_stream = Some(tokio::spawn(run_order_stream(
                    use_case,
                    self.actions.clone(),
                )));
            }
        }
    }

    /// Ejecuta una tarea que produce una sola acción.
    fn spawn(&self, task: impl Future<Output = Action> + Send + 'static) {
        let actions = self.actions.clone();
        tokio::spawn(async move {
            // Si el receptor ya no existe, la TUI se está cerrando
            let _ = actions.send(task.await);
        });
    }
}

impl Drop for CommandExecutor {
    fn drop(&mut self) {
        abort(&mut self.market_stream);
        abort(&mut self.order_stream);
    }
}

fn abort(task: &mut Option<JoinHandle<()>>) {
    if let Some(task) = task.take() {
        task.abort();
    }
}

/// Mensaje para el usuario. Un rechazo de riesgo lista cada motivo, no solo cuántos son.
fn describe(error: &ApplicationError) -> String {
    match error {
        ApplicationError::RiskRejected(violations) => {
            let reasons: Vec<String> = violations.iter().map(ToString::to_string).collect();
            format!("Bloqueada por riesgo: {}", reasons.join("; "))
        }
        other => other.to_string(),
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
            let _ = actions.send(Action::StreamDisconnected(describe(&error)));
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

async fn run_order_stream(
    use_case: Arc<WatchOrderUpdatesUseCase>,
    actions: UnboundedSender<Action>,
) {
    let mut updates = match use_case.stream().await {
        Ok(updates) => updates,
        Err(error) => {
            tracing::warn!(%error, "no se pudo abrir el flujo de órdenes");
            let _ = actions.send(Action::OrderStreamDisconnected(describe(&error)));
            return;
        }
    };
    if actions.send(Action::OrderStreamConnected).is_err() {
        return;
    }
    tracing::info!("flujo de actualizaciones de órdenes conectado");

    while let Some(update) = updates.next().await {
        tracing::info!(order_id = %update.order_id, kind = ?update.kind, "actualización de orden");
        if actions.send(Action::OrderUpdateReceived(update)).is_err() {
            return;
        }
    }
    tracing::warn!("el flujo de órdenes se cerró");
    let _ = actions.send(Action::OrderStreamDisconnected(
        "el servidor cerró la conexión".to_owned(),
    ));
}
