use std::collections::VecDeque;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::time::Instant;

/// Limitador de peticiones con ventana deslizante: como máximo `limit` peticiones en cualquier
/// periodo de `window`.
///
/// Alpaca limita cada API a unas 200 peticiones por minuto y responde 429 al excederlas.
/// Limitar del lado del cliente evita llegar a ese punto (por ejemplo, cuando en el futuro un
/// modelo de ML consulte datos con frecuencia). Usa solo aritmética entera.
#[derive(Debug)]
pub(crate) struct RateLimiter {
    limit: usize,
    window: Duration,
    recent: Mutex<VecDeque<Instant>>,
}

impl RateLimiter {
    pub(crate) fn new(limit: u32, window: Duration) -> Self {
        // Un límite de 0 bloquearía para siempre: se usa al menos 1
        let limit = usize::try_from(limit.max(1)).unwrap_or(usize::MAX);
        Self {
            limit,
            window,
            recent: Mutex::new(VecDeque::with_capacity(limit.min(1_024))),
        }
    }

    pub(crate) fn per_minute(limit: u32) -> Self {
        Self::new(limit, Duration::from_secs(60))
    }

    /// Espera, si hace falta, hasta que haya cupo para una petición más y la registra.
    pub(crate) async fn acquire(&self) {
        loop {
            let wait = {
                let mut recent = self.recent.lock().await;
                let now = Instant::now();
                while recent
                    .front()
                    .is_some_and(|sent| now.duration_since(*sent) >= self.window)
                {
                    recent.pop_front();
                }
                if recent.len() < self.limit {
                    recent.push_back(now);
                    return;
                }
                // Sin cupo: esperar a que la petición más antigua salga de la ventana
                recent.front().map_or(Duration::ZERO, |oldest| {
                    self.window.saturating_sub(now.duration_since(*oldest))
                })
            };
            tokio::time::sleep(wait).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn waits_when_the_window_is_full() {
        let limiter = RateLimiter::new(2, Duration::from_secs(60));
        let start = Instant::now();

        limiter.acquire().await;
        limiter.acquire().await;
        assert_eq!(start.elapsed(), Duration::ZERO);

        // La tercera debe esperar a que la primera cumpla 60 s
        limiter.acquire().await;
        assert_eq!(start.elapsed(), Duration::from_secs(60));
    }

    #[tokio::test(start_paused = true)]
    async fn spaced_requests_never_wait() {
        let limiter = RateLimiter::new(2, Duration::from_secs(60));
        let start = Instant::now();
        for _ in 0..5 {
            limiter.acquire().await;
            tokio::time::advance(Duration::from_secs(30)).await;
        }
        // Solo avanzó el tiempo simulado de las pausas, nunca una espera del limitador
        assert_eq!(start.elapsed(), Duration::from_secs(150));
    }
}
