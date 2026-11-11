use tickforge_domain::trading::{ClientOrderId, Order};

use crate::error::ApplicationError;
use crate::ports::{JournalEntry, ReconcileOutcome};
use crate::use_cases::trading::TradingPorts;

/// Resultado de una reconciliación, para informar al usuario.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReconciliationReport {
    /// Intenciones pendientes revisadas.
    pub checked: usize,
    /// Órdenes que sí llegaron al broker aunque la aplicación no lo registró.
    pub recovered: Vec<Order>,
    /// Intenciones que nunca llegaron al broker.
    pub not_submitted: Vec<ClientOrderId>,
    /// Intenciones que no se pudieron verificar (se reintentan en el próximo arranque).
    pub unresolved: Vec<(ClientOrderId, String)>,
}

impl ReconciliationReport {
    pub fn is_clean(&self) -> bool {
        self.checked == 0
    }
}

/// Caso de uso: al iniciar, compara las intenciones sin resultado con el broker.
///
/// Cubre el caso crítico de una caída (de la red o de la aplicación) entre "registré que voy a
/// enviar" y "el broker confirmó": la orden pudo haber llegado o no, y solo el broker lo sabe.
/// Gracias al `ClientOrderId` se puede preguntar por ella sin ambigüedad.
pub struct ReconcileOrdersUseCase {
    ports: TradingPorts,
}

impl ReconcileOrdersUseCase {
    pub fn new(ports: TradingPorts) -> Self {
        Self { ports }
    }

    pub async fn execute(&self) -> Result<ReconciliationReport, ApplicationError> {
        let ports = &self.ports;
        let pending = ports.journal.pending_intents().await?;
        let mut report = ReconciliationReport {
            checked: pending.len(),
            ..ReconciliationReport::default()
        };

        for request in pending {
            let client_order_id = request.client_order_id().clone();
            let outcome = match ports
                .broker
                .order_by_client_order_id(&client_order_id)
                .await
            {
                Ok(Some(order)) => {
                    let outcome = ReconcileOutcome::FoundAtBroker {
                        order_id: order.id().clone(),
                        status: order.status(),
                    };
                    report.recovered.push(order);
                    outcome
                }
                Ok(None) => {
                    report.not_submitted.push(client_order_id.clone());
                    ReconcileOutcome::NotFoundAtBroker
                }
                Err(error) => {
                    // Sin respuesta del broker no se puede concluir nada: queda pendiente
                    report.unresolved.push((client_order_id, error.to_string()));
                    continue;
                }
            };

            ports
                .journal
                .append(JournalEntry::Reconciled {
                    client_order_id,
                    outcome,
                    at: ports.clock.now(),
                })
                .await?;
        }

        Ok(report)
    }
}
