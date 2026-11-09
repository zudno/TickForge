use tickforge_application::ports::{ApplicationEvent, EventPublisher};

/// Publicador de eventos provisional: los registra en el log.
///
/// La TUI recibe los resultados de sus propios comandos directamente; un bus de eventos real
/// (para el diario, métricas o modelos de ML) llegará cuando haya más de un consumidor.
pub struct TracingEventPublisher;

impl EventPublisher for TracingEventPublisher {
    fn publish(&self, event: ApplicationEvent) {
        match &event {
            ApplicationEvent::JournalWriteFailed {
                client_order_id,
                reason,
            } => {
                tracing::error!(%client_order_id, %reason, "no se pudo escribir en el diario");
            }
            other => tracing::debug!(event = ?other, "evento de aplicación"),
        }
    }
}
