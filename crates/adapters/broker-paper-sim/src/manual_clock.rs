use std::sync::Mutex;

use chrono::{DateTime, TimeDelta, Utc};
use tickforge_application::ports::Clock;

use crate::lock::lock;

/// Reloj controlado manualmente: el tiempo solo avanza cuando la prueba lo decide.
#[derive(Debug)]
pub struct ManualClock {
    now: Mutex<DateTime<Utc>>,
}

impl ManualClock {
    pub fn new(start: DateTime<Utc>) -> Self {
        Self {
            now: Mutex::new(start),
        }
    }

    pub fn set(&self, instant: DateTime<Utc>) {
        *lock(&self.now) = instant;
    }

    /// Avanza el reloj; satura en lugar de desbordar.
    pub fn advance(&self, delta: TimeDelta) {
        let mut now = lock(&self.now);
        *now = now
            .checked_add_signed(delta)
            .unwrap_or(DateTime::<Utc>::MAX_UTC);
    }
}

impl Clock for ManualClock {
    fn now(&self) -> DateTime<Utc> {
        *lock(&self.now)
    }
}
