use chrono::{DateTime, TimeDelta, Utc};
use tickforge_domain::market::Quote;
use tickforge_domain::portfolio::{Account, Position};
use tickforge_domain::risk::{KillSwitch, OrderActivity, RiskAssessment, RiskContext, RiskPolicy};
use tickforge_domain::shared::{Money, Price, Quantity, Symbol};
use tickforge_domain::trading::{Order, OrderRequest, OrderSide, OrderType, TimeInForce};

use crate::error::ApplicationError;
use crate::ports::{ApplicationEvent, JournalEntry};
use crate::use_cases::trading::TradingPorts;

/// Datos que captura la TUI (o el motor de decisiones de ML) para una orden nueva.
///
/// No incluye el `ClientOrderId`: lo genera el caso de uso para garantizar su unicidad.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceOrderCommand {
    pub symbol: Symbol,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub quantity: Quantity,
    pub time_in_force: TimeInForce,
}

/// Resultado de evaluar una orden **sin enviarla**: lo que el usuario confirma.
///
/// Contiene la solicitud ya construida (con su `ClientOrderId`): al confirmar se envía
/// exactamente esa solicitud, así que confirmar dos veces nunca duplica la orden.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderPreview {
    pub request: OrderRequest,
    /// Precio usado para estimar el costo: el límite, o el ask/bid actual a mercado.
    pub reference_price: Option<Price>,
    pub estimated_notional: Option<Money>,
    pub buying_power: Money,
    /// Cantidad que ya se tiene del símbolo (para mostrar el efecto de la orden).
    pub position_quantity: Quantity,
    pub assessment: RiskAssessment,
}

/// Contexto que la política de riesgo necesita, consultado en el momento.
struct RiskInputs {
    account: Account,
    position: Option<Position>,
    quote: Quote,
    kill_switch: KillSwitch,
    /// Solo se consulta si la política lo necesita (ver `market_open_if_required`).
    market_open: Option<bool>,
    /// Solo se cuenta si los límites tienen tope de órdenes (ver `order_activity`).
    order_activity: OrderActivity,
    now: DateTime<Utc>,
}

/// Caso de uso: colocar una orden aplicando la política de riesgo.
///
/// Es la **única** ruta por la que una orden llega al broker. Ofrece dos pasos para la TUI
/// (`preview` y `submit`) y uno directo (`execute`) para el futuro motor de decisiones.
pub struct PlaceOrderUseCase {
    ports: TradingPorts,
    risk_policy: RiskPolicy,
}

impl PlaceOrderUseCase {
    pub fn new(ports: TradingPorts, risk_policy: RiskPolicy) -> Self {
        Self { ports, risk_policy }
    }

    /// Construye la solicitud y la evalúa contra la política de riesgo, sin enviarla.
    pub async fn preview(
        &self,
        command: PlaceOrderCommand,
    ) -> Result<OrderPreview, ApplicationError> {
        let request = self.build_request(command)?;
        let inputs = self.gather_inputs(&request).await?;
        let assessment = self.evaluate(&request, &inputs)?;

        let market_price = inputs.quote.executable_price(request.side());
        let reference_price = request.order_type().limit_price().or(market_price);
        let estimated_notional = reference_price
            .map(|price| request.estimated_notional(price))
            .transpose()?;

        Ok(OrderPreview {
            reference_price,
            estimated_notional,
            buying_power: inputs.account.buying_power(),
            position_quantity: inputs
                .position
                .as_ref()
                .map_or(Quantity::ZERO, Position::quantity),
            assessment,
            request,
        })
    }

    /// Envía una solicitud ya construida (normalmente la de un `OrderPreview` confirmado).
    ///
    /// La política de riesgo se vuelve a evaluar con datos frescos: entre la vista previa y
    /// la confirmación el precio o el saldo pueden haber cambiado.
    pub async fn submit(&self, request: OrderRequest) -> Result<Order, ApplicationError> {
        let ports = &self.ports;

        let inputs = self.gather_inputs(&request).await?;
        let assessment = self.evaluate(&request, &inputs)?;

        if !assessment.is_approved() {
            let violations = assessment.into_violations();
            ports
                .append_best_effort(
                    JournalEntry::RiskRejected {
                        request: request.clone(),
                        violations: violations.clone(),
                        at: inputs.now,
                    },
                    request.client_order_id(),
                )
                .await;
            ports.events.publish(ApplicationEvent::OrderRejectedByRisk {
                request,
                violations: violations.clone(),
            });
            return Err(ApplicationError::RiskRejected(violations));
        }

        // Registrar la intención ANTES de enviar: sin auditoría no se envía nada
        ports
            .journal
            .append(JournalEntry::Intent {
                request: request.clone(),
                at: inputs.now,
            })
            .await?;

        let order = match ports.broker.submit_order(&request).await {
            Ok(order) => order,
            Err(error) => {
                let reason = error.to_string();
                ports
                    .append_best_effort(
                        JournalEntry::SubmissionFailed {
                            request: request.clone(),
                            reason: reason.clone(),
                            at: ports.clock.now(),
                        },
                        request.client_order_id(),
                    )
                    .await;
                ports
                    .events
                    .publish(ApplicationEvent::OrderSubmissionFailed { request, reason });
                return Err(error.into());
            }
        };

        // La orden ya está viva: un fallo del diario no debe ocultarlo al usuario
        ports
            .append_best_effort(
                JournalEntry::Submitted {
                    order: order.clone(),
                    at: ports.clock.now(),
                },
                order.client_order_id(),
            )
            .await;
        ports
            .events
            .publish(ApplicationEvent::OrderSubmitted(order.clone()));

        Ok(order)
    }

    /// Construye y envía en un solo paso (sin confirmación humana).
    pub async fn execute(&self, command: PlaceOrderCommand) -> Result<Order, ApplicationError> {
        let request = self.build_request(command)?;
        self.submit(request).await
    }

    fn build_request(&self, command: PlaceOrderCommand) -> Result<OrderRequest, ApplicationError> {
        Ok(OrderRequest::new(
            self.ports.client_order_ids.next_id()?,
            command.symbol,
            command.side,
            command.order_type,
            command.quantity,
            command.time_in_force,
        )?)
    }

    /// Cuenta, posición, cotización, kill switch y (si hace falta) horario, en paralelo.
    async fn gather_inputs(&self, request: &OrderRequest) -> Result<RiskInputs, ApplicationError> {
        let ports = &self.ports;
        let now = ports.clock.now();
        let (account, position, quote, kill_switch, market_open, order_activity) = futures::try_join!(
            async { ports.broker.account().await.map_err(ApplicationError::from) },
            async {
                ports
                    .broker
                    .position(request.symbol())
                    .await
                    .map_err(ApplicationError::from)
            },
            async {
                ports
                    .market_data
                    .latest_quote(request.symbol())
                    .await
                    .map_err(ApplicationError::from)
            },
            // Se lee del almacenamiento en cada orden: si no se puede leer, la orden no sale
            async {
                ports
                    .kill_switch
                    .load()
                    .await
                    .map_err(ApplicationError::from)
            },
            async { Ok(self.market_open_if_required(request).await) },
            self.order_activity(now),
        )?;
        Ok(RiskInputs {
            account,
            position,
            quote,
            kill_switch,
            market_open,
            order_activity,
            now,
        })
    }

    /// Órdenes enviadas en los últimos 60 s y 24 h, contadas en el diario. Si el diario no
    /// responde, la orden no sale (el tope no se puede verificar).
    async fn order_activity(&self, now: DateTime<Utc>) -> Result<OrderActivity, ApplicationError> {
        if !self.risk_policy.limits().limits_order_rate() {
            return Ok(OrderActivity::default());
        }
        let since = |window: TimeDelta| now.checked_sub_signed(window).unwrap_or(now);
        let journal = &self.ports.journal;
        let (last_minute, last_day) = futures::try_join!(
            journal.count_intents_since(since(TimeDelta::minutes(1))),
            journal.count_intents_since(since(TimeDelta::days(1))),
        )?;
        Ok(OrderActivity {
            last_minute,
            last_day,
        })
    }

    /// El horario solo importa para órdenes a mercado cuando los límites las restringen; así
    /// no se gasta una petición en cada orden límite. Si el broker no responde, se devuelve
    /// `None` y la política lo trata como mercado cerrado (la orden no sale).
    async fn market_open_if_required(&self, request: &OrderRequest) -> Option<bool> {
        let required = self
            .risk_policy
            .limits()
            .market_orders_require_open_market()
            && request.order_type().limit_price().is_none();
        if !required {
            return None;
        }
        self.ports
            .broker
            .market_clock()
            .await
            .ok()
            .map(|clock| clock.is_open())
    }

    fn evaluate(
        &self,
        request: &OrderRequest,
        inputs: &RiskInputs,
    ) -> Result<RiskAssessment, ApplicationError> {
        Ok(self.risk_policy.evaluate(&RiskContext {
            request,
            account: &inputs.account,
            position: inputs.position.as_ref(),
            quote: &inputs.quote,
            now: inputs.now,
            kill_switch: &inputs.kill_switch,
            market_open: inputs.market_open,
            order_activity: inputs.order_activity,
        })?)
    }
}
