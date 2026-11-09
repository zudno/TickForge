use std::sync::Mutex;

use tickforge_application::ports::{ApplicationEvent, EventPublisher};

use crate::lock::lock;

/// Publicador que solo guarda los eventos, para inspeccionarlos en pruebas.
#[derive(Debug, Default)]
pub struct RecordingEventPublisher {
    events: Mutex<Vec<ApplicationEvent>>,
}

impl RecordingEventPublisher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Copia de todos los eventos publicados, en orden.
    pub fn events(&self) -> Vec<ApplicationEvent> {
        lock(&self.events).clone()
    }
}

impl EventPublisher for RecordingEventPublisher {
    fn publish(&self, event: ApplicationEvent) {
        lock(&self.events).push(event);
    }
}
