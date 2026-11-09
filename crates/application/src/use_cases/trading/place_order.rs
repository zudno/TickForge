use tickforge_domain::risk::{RiskContext, RiskPolicy};
use tickforge_domain::shared::{Quantity, Symbol};
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

/// Caso de uso: colocar una orden aplicando la política de riesgo.
///
/// Es la **única** ruta por la que una orden llega al broker.
pub struct PlaceOrderUseCase {
    ports: TradingPorts,
    risk_policy: RiskPolicy,
}

impl PlaceOrderUseCase {
    pub fn new(ports: TradingPorts, risk_policy: RiskPolicy) -> Self {
        Self { ports, risk_policy }
    }

    pub async fn execute(&self, command: PlaceOrderCommand) -> Result<Order, ApplicationError> {
        let ports = &self.ports;

        // 1. Construir la solicitud validada por el dominio
        let request = OrderRequest::new(
            ports.client_order_ids.next_id()?,
            command.symbol,
            command.side,
            command.order_type,
            command.quantity,
            command.time_in_force,
        )?;

        // 2. Reunir el contexto de riesgo en paralelo
        let (account, position, quote) = futures::try_join!(
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
        )?;

        // 3. Validación pre-trade: si falla, la orden nunca sale del sistema
        let now = ports.clock.now();
        let assessment = self.risk_policy.evaluate(&RiskContext {
            request: &request,
            account: &account,
            position: position.as_ref(),
            quote: &quote,
            now,
        })?;

        if !assessment.is_approved() {
            let violations = assessment.into_violations();
            ports
                .append_best_effort(
                    JournalEntry::RiskRejected {
                        request: request.clone(),
                        violations: violations.clone(),
                        at: now,
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

        // 4. Registrar la intención ANTES de enviar: sin auditoría no se envía nada
        ports
            .journal
            .append(JournalEntry::Intent {
                request: request.clone(),
                at: now,
            })
            .await?;

        // 5. Enviar al broker
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

        // 6. La orden ya está viva: un fallo del diario no debe ocultarlo al usuario
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
}
