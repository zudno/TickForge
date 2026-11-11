use async_trait::async_trait;
use tickforge_application::ports::{JournalEntry, JournalError, OrderJournal};
use tickforge_domain::trading::OrderRequest;

/// Diario de órdenes provisional: escribe cada entrada en el archivo de logs con el
/// destino `tickforge::journal`, para poder auditarlo con un simple `grep`.
///
/// Se reemplazará por el diario en SQLite de la fase 6.
pub struct TracingOrderJournal;

const TARGET: &str = "tickforge::journal";

#[async_trait]
impl OrderJournal for TracingOrderJournal {
    async fn append(&self, entry: JournalEntry) -> Result<(), JournalError> {
        match entry {
            JournalEntry::Intent { request, at } => {
                tracing::info!(target: TARGET, %at, "INTENCIÓN {}", describe(&request));
            }
            JournalEntry::RiskRejected {
                request,
                violations,
                at,
            } => {
                let reasons: Vec<String> = violations.iter().map(ToString::to_string).collect();
                tracing::info!(
                    target: TARGET,
                    %at,
                    "RECHAZO DE RIESGO {} | {}",
                    describe(&request),
                    reasons.join("; ")
                );
            }
            JournalEntry::Submitted { order, at } => {
                tracing::info!(
                    target: TARGET,
                    %at,
                    "ENVIADA {} | id broker {} | estado {}",
                    describe(order.request()),
                    order.id(),
                    order.status()
                );
            }
            JournalEntry::SubmissionFailed {
                request,
                reason,
                at,
            } => {
                tracing::warn!(
                    target: TARGET,
                    %at,
                    "FALLO DE ENVÍO {} | {reason}",
                    describe(&request)
                );
            }
            JournalEntry::CancelRequested { order_id, at } => {
                tracing::info!(target: TARGET, %at, "CANCELACIÓN SOLICITADA id broker {order_id}");
            }
        }
        Ok(())
    }
}

fn describe(request: &OrderRequest) -> String {
    format!(
        "{} {} {} {} {} (cliente {})",
        request.side(),
        request.quantity(),
        request.symbol(),
        request.order_type(),
        request.time_in_force(),
        request.client_order_id()
    )
}
