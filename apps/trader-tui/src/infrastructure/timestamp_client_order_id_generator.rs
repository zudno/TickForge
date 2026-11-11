use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tickforge_application::ports::{ClientOrderIdGenerator, Clock};
use tickforge_domain::DomainError;
use tickforge_domain::trading::ClientOrderId;

/// Genera ids `tf-<milisegundos>-<secuencia>`, únicos entre reinicios de la aplicación
/// (la marca de tiempo cambia) y dentro de una misma sesión (la secuencia avanza).
pub struct TimestampClientOrderIdGenerator {
    clock: Arc<dyn Clock>,
    sequence: AtomicU64,
}

impl TimestampClientOrderIdGenerator {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            clock,
            sequence: AtomicU64::new(0),
        }
    }
}

impl ClientOrderIdGenerator for TimestampClientOrderIdGenerator {
    fn next_id(&self) -> Result<ClientOrderId, DomainError> {
        let millis = self.clock.now().timestamp_millis();
        let sequence = self
            .sequence
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1);
        ClientOrderId::new(&format!("tf-{millis}-{sequence}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};

    struct FixedClock;

    impl Clock for FixedClock {
        fn now(&self) -> DateTime<Utc> {
            DateTime::from_timestamp(1_700_000_000, 0).unwrap()
        }
    }

    #[test]
    fn ids_are_unique_even_within_the_same_millisecond() {
        let generator = TimestampClientOrderIdGenerator::new(Arc::new(FixedClock));
        let first = generator.next_id().unwrap();
        let second = generator.next_id().unwrap();

        assert_eq!(first.as_str(), "tf-1700000000000-1");
        assert_ne!(first, second);
    }
}
