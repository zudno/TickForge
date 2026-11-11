use std::time::Duration;

/// Espera exponencial entre reintentos: `initial`, `2·initial`, `4·initial`… hasta `max`.
///
/// Evita martillar a un servicio caído (y agotar sus límites de peticiones) mientras se
/// recupera rápido de cortes breves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExponentialBackoff {
    initial: Duration,
    max: Duration,
}

impl ExponentialBackoff {
    pub const fn new(initial: Duration, max: Duration) -> Self {
        Self { initial, max }
    }

    /// Reconexión de flujos en tiempo real: 1 s, 2 s, 4 s… hasta 1 minuto.
    pub const fn streaming() -> Self {
        Self::new(Duration::from_secs(1), Duration::from_secs(60))
    }

    /// Reintentos de peticiones REST: 250 ms, 500 ms, 1 s… hasta 4 s.
    pub const fn requests() -> Self {
        Self::new(Duration::from_millis(250), Duration::from_secs(4))
    }

    /// Espera antes del intento número `attempt` (el primer reintento es `1`).
    pub fn delay(&self, attempt: u32) -> Duration {
        let exponent = attempt.saturating_sub(1).min(31);
        let factor = 1_u32 << exponent;
        self.initial
            .checked_mul(factor)
            .map_or(self.max, |delay| delay.min(self.max))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doubles_until_the_maximum() {
        let backoff = ExponentialBackoff::streaming();
        let delays: Vec<u64> = (1..=8).map(|n| backoff.delay(n).as_secs()).collect();
        assert_eq!(delays, [1, 2, 4, 8, 16, 32, 60, 60]);
    }

    #[test]
    fn huge_attempt_numbers_never_overflow() {
        let backoff = ExponentialBackoff::requests();
        assert_eq!(backoff.delay(u32::MAX), Duration::from_secs(4));
        assert_eq!(backoff.delay(0), Duration::from_millis(250));
    }
}
