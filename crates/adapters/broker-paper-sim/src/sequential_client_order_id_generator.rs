use std::sync::atomic::{AtomicU64, Ordering};

use tickforge_application::ports::ClientOrderIdGenerator;
use tickforge_domain::DomainError;
use tickforge_domain::trading::ClientOrderId;

/// Genera ids predecibles (`<prefix>-000001`, `<prefix>-000002`, ...) para pruebas.
#[derive(Debug)]
pub struct SequentialClientOrderIdGenerator {
    prefix: String,
    counter: AtomicU64,
}

impl SequentialClientOrderIdGenerator {
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            counter: AtomicU64::new(0),
        }
    }
}

impl ClientOrderIdGenerator for SequentialClientOrderIdGenerator {
    fn next_id(&self) -> Result<ClientOrderId, DomainError> {
        let sequence = self
            .counter
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1);
        ClientOrderId::new(&format!("{}-{sequence:06}", self.prefix))
    }
}
