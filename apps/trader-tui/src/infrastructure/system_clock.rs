use std::time::SystemTime;

use chrono::{DateTime, Utc};
use tickforge_application::ports::Clock;

/// Reloj real del sistema. Es la única implementación de `Clock` que lee la hora del
/// sistema operativo; todo lo demás la recibe inyectada.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        DateTime::from(SystemTime::now())
    }
}
