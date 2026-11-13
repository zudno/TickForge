use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use tickforge_application::ApplicationError;
use tickforge_application::ports::Clock;
use tickforge_application::ports::MarketEvent;
use tickforge_application::resilience::ExponentialBackoff;
use tickforge_application::use_cases::market::{
    GetMarketClockUseCase, ManageWatchlistUseCase, RecordMarketDataUseCase, WatchMarketUseCase,
};
use tickforge_application::use_cases::portfolio::GetPortfolioSnapshotUseCase;
use tickforge_application::use_cases::trading::{
    CancelOrderUseCase, DailyLossGuard, KillSwitchUseCase, PlaceOrderCommand, PlaceOrderUseCase,
    ReconcileOrdersUseCase, WatchOrderUpdatesUseCase,
};
use tickforge_domain::portfolio::Account;
use tickforge_domain::shared::Symbol;
use tickforge_domain::trading::{OrderId, OrderRequest};
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinHandle;
use tokio::time::Instant;

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
    LoadKillSwitch,
    EngageKillSwitch,
    ReleaseKillSwitch,
    /// Revisa la pérdida del día con la cuenta recién cargada; si alcanzó el máximo, el
    /// kill switch se activa solo.
    CheckDailyLoss(Account),
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
    pub kill_switch: Arc<KillSwitchUseCase>,
    pub daily_loss_guard: Arc<DailyLossGuard>,
    /// Guarda las velas del flujo en vivo en el histórico (materia prima para ML).
    pub record_market_data: Arc<RecordMarketDataUseCase>,
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
            Command::LoadKillSwitch => {
                let use_case = Arc::clone(&services.kill_switch);
                self.spawn(async move {
                    match use_case.status().await {
                        Ok(state) => Action::KillSwitchLoaded(state),
                        Err(error) => Action::KillSwitchFailed(describe(&error)),
                    }
                });
            }
            Command::EngageKillSwitch => {
                let use_case = Arc::clone(&services.kill_switch);
                self.spawn(async move {
                    let engaged = match use_case.engage("activado desde la TUI").await {
                        Ok(report) => report,
                        Err(error) => {
                            tracing::error!(%error, "no se pudo activar el kill switch");
                            return Action::KillSwitchFailed(describe(&error));
                        }
                    };
                    tracing::warn!(
                        canceled = engaged.canceled.len(),
                        failed = engaged.failed.len(),
                        "kill switch activado"
                    );
                    // Se relee el estado guardado para mostrar la hora exacta de activación
                    match use_case.status().await {
                        Ok(state) => Action::KillSwitchEngaged {
                            state,
                            report: engaged,
                        },
                        Err(error) => Action::KillSwitchFailed(describe(&error)),
                    }
                });
            }
            Command::CheckDailyLoss(account) => {
                let guard = Arc::clone(&services.daily_loss_guard);
                let kill_switch = Arc::clone(&services.kill_switch);
                self.spawn_optional(async move {
                    let report = match guard.check(&account).await {
                        Ok(Some(report)) => report,
                        Ok(None) => return None,
                        Err(error) => {
                            tracing::error!(%error, "no se pudo verificar la pérdida del día");
                            return Some(Action::DailyLossCheckFailed(describe(&error)));
                        }
                    };
                    tracing::warn!(
                        canceled = report.canceled.len(),
                        failed = report.failed.len(),
                        "kill switch activado por la pérdida del día"
                    );
                    Some(match kill_switch.status().await {
                        Ok(state) => Action::KillSwitchEngaged { state, report },
                        Err(error) => Action::KillSwitchFailed(describe(&error)),
                    })
                });
            }
            Command::ReleaseKillSwitch => {
                let use_case = Arc::clone(&services.kill_switch);
                self.spawn(async move {
                    match use_case.release().await {
                        Ok(()) => {
                            tracing::warn!("kill switch desactivado");
                            Action::KillSwitchReleased
                        }
                        Err(error) => Action::KillSwitchFailed(describe(&error)),
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
                let recorder = Arc::clone(&services.record_market_data);
                self.market_stream = Some(tokio::spawn(run_market_stream(
                    use_case,
                    recorder,
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
        self.spawn_optional(async move { Some(task.await) });
    }

    /// Como `spawn`, para tareas que no siempre tienen algo que informar.
    fn spawn_optional(&self, task: impl Future<Output = Option<Action>> + Send + 'static) {
        let actions = self.actions.clone();
        tokio::spawn(async move {
            if let Some(action) = task.await {
                // Si el receptor ya no existe, la TUI se está cerrando
                let _ = actions.send(action);
            }
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

/// Una conexión se considera sana si entregó datos o duró al menos esto. Solo entonces se
/// reinicia la cuenta de reintentos: un servidor que acepta y corta de inmediato sigue
/// recibiendo esperas cada vez más largas.
const HEALTHY_CONNECTION: Duration = Duration::from_secs(60);

/// Cómo terminó una conexión del flujo.
enum StreamEnd {
    /// La TUI se está cerrando: no hay que reconectar.
    Shutdown,
    /// Se perdió la conexión (o no se pudo abrir).
    Lost { reason: String, healthy: bool },
}

/// Flujo de mercado con reconexión automática y espera exponencial entre intentos.
/// Cada reconexión vuelve a cargar la foto inicial, así no quedan huecos de datos.
async fn run_market_stream(
    use_case: Arc<WatchMarketUseCase>,
    recorder: Arc<RecordMarketDataUseCase>,
    symbols: Vec<Symbol>,
    actions: UnboundedSender<Action>,
) {
    let backoff = ExponentialBackoff::streaming();
    let mut attempt: u32 = 0;
    loop {
        match market_connection(&use_case, &recorder, &symbols, &actions).await {
            StreamEnd::Shutdown => return,
            StreamEnd::Lost { reason, healthy } => {
                if healthy {
                    attempt = 0;
                }
                attempt = attempt.saturating_add(1);
                let delay = backoff.delay(attempt);
                tracing::warn!(%reason, attempt, ?delay, "flujo de mercado perdido; se reconectará");
                // Si la TUI se cerró, el canal ya no existe y no hay que reconectar
                if actions.send(Action::StreamDisconnected(reason)).is_err()
                    || actions
                        .send(Action::StreamReconnecting { attempt, delay })
                        .is_err()
                {
                    return;
                }
                tokio::time::sleep(delay).await;
            }
        }
    }
}

async fn market_connection(
    use_case: &WatchMarketUseCase,
    recorder: &Arc<RecordMarketDataUseCase>,
    symbols: &[Symbol],
    actions: &UnboundedSender<Action>,
) -> StreamEnd {
    let snapshot = use_case.snapshot(symbols).await;
    if actions.send(Action::WatchlistSeeded(snapshot)).is_err() {
        return StreamEnd::Shutdown;
    }
    let mut events = match use_case.stream(symbols).await {
        Ok(events) => events,
        Err(error) => {
            return StreamEnd::Lost {
                reason: describe(&error),
                healthy: false,
            };
        }
    };
    if actions.send(Action::StreamConnected).is_err() {
        return StreamEnd::Shutdown;
    }
    tracing::info!(symbols = symbols.len(), "flujo de mercado conectado");

    let connected_at = Instant::now();
    let mut received = false;
    while let Some(event) = events.next().await {
        received = true;
        if let MarketEvent::Bar(bar) = &event {
            record_bar(recorder, bar.clone());
        }
        if actions.send(Action::Market(event)).is_err() {
            return StreamEnd::Shutdown;
        }
    }
    StreamEnd::Lost {
        reason: "el servidor cerró la conexión".to_owned(),
        healthy: received || connected_at.elapsed() >= HEALTHY_CONNECTION,
    }
}

/// Guarda la vela en el histórico en segundo plano: escribir en disco nunca retrasa la
/// pantalla. Un fallo solo se registra en el log (el histórico es para ML, no para operar).
fn record_bar(recorder: &Arc<RecordMarketDataUseCase>, bar: tickforge_domain::market::Bar) {
    let recorder = Arc::clone(recorder);
    tokio::spawn(async move {
        if let Err(error) = recorder.record(&bar).await {
            tracing::warn!(%error, symbol = %bar.symbol(), "no se pudo guardar la vela");
        }
    });
}

/// Flujo de actualizaciones de órdenes con reconexión automática.
async fn run_order_stream(
    use_case: Arc<WatchOrderUpdatesUseCase>,
    actions: UnboundedSender<Action>,
) {
    let backoff = ExponentialBackoff::streaming();
    let mut attempt: u32 = 0;
    loop {
        match order_connection(&use_case, &actions).await {
            StreamEnd::Shutdown => return,
            StreamEnd::Lost { reason, healthy } => {
                if healthy {
                    attempt = 0;
                }
                attempt = attempt.saturating_add(1);
                let delay = backoff.delay(attempt);
                tracing::warn!(%reason, attempt, ?delay, "flujo de órdenes perdido; se reconectará");
                if actions
                    .send(Action::OrderStreamDisconnected(reason))
                    .is_err()
                    || actions
                        .send(Action::OrderStreamReconnecting { attempt, delay })
                        .is_err()
                {
                    return;
                }
                tokio::time::sleep(delay).await;
            }
        }
    }
}

async fn order_connection(
    use_case: &WatchOrderUpdatesUseCase,
    actions: &UnboundedSender<Action>,
) -> StreamEnd {
    let mut updates = match use_case.stream().await {
        Ok(updates) => updates,
        Err(error) => {
            return StreamEnd::Lost {
                reason: describe(&error),
                healthy: false,
            };
        }
    };
    if actions.send(Action::OrderStreamConnected).is_err() {
        return StreamEnd::Shutdown;
    }
    tracing::info!("flujo de actualizaciones de órdenes conectado");

    // Las órdenes pueden pasar horas sin eventos: la conexión cuenta como sana por duración
    let connected_at = Instant::now();
    let mut received = false;
    while let Some(update) = updates.next().await {
        received = true;
        tracing::info!(order_id = %update.order_id, kind = ?update.kind, "actualización de orden");
        if actions.send(Action::OrderUpdateReceived(update)).is_err() {
            return StreamEnd::Shutdown;
        }
    }
    StreamEnd::Lost {
        reason: "el servidor cerró la conexión".to_owned(),
        healthy: received || connected_at.elapsed() >= HEALTHY_CONNECTION,
    }
}

#[cfg(test)]
mod tests;
