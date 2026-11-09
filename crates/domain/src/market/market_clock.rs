use chrono::{DateTime, TimeDelta, Utc};

use crate::error::DomainError;

/// Estado de la sesión regular del mercado de EE. UU. en un instante dado.
///
/// Lo calcula el broker (que conoce feriados y cierres anticipados); el dominio solo valida
/// que los datos sean coherentes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarketClock {
    as_of: DateTime<Utc>,
    is_open: bool,
    next_open: DateTime<Utc>,
    next_close: DateTime<Utc>,
}

impl MarketClock {
    pub fn new(
        as_of: DateTime<Utc>,
        is_open: bool,
        next_open: DateTime<Utc>,
        next_close: DateTime<Utc>,
    ) -> Result<Self, DomainError> {
        if next_open < as_of || next_close < as_of {
            return Err(DomainError::InvalidMarketClock(
                "la próxima apertura y el próximo cierre deben ser futuros",
            ));
        }
        // Abierto: el próximo evento es el cierre. Cerrado: el próximo evento es la apertura.
        let next_is_close = next_close < next_open;
        if is_open != next_is_close {
            return Err(DomainError::InvalidMarketClock(
                "el estado abierto/cerrado no coincide con el próximo evento",
            ));
        }

        Ok(Self {
            as_of,
            is_open,
            next_open,
            next_close,
        })
    }

    pub fn as_of(&self) -> DateTime<Utc> {
        self.as_of
    }

    pub fn is_open(&self) -> bool {
        self.is_open
    }

    pub fn next_open(&self) -> DateTime<Utc> {
        self.next_open
    }

    pub fn next_close(&self) -> DateTime<Utc> {
        self.next_close
    }

    /// Tiempo restante hasta el próximo cambio de estado (cierre si está abierto, apertura si no).
    pub fn time_until_next_change(&self) -> TimeDelta {
        let next_change = if self.is_open {
            self.next_close
        } else {
            self.next_open
        };
        next_change.signed_duration_since(self.as_of)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(seconds, 0).unwrap()
    }

    #[test]
    fn open_market_counts_down_to_close() {
        let clock = MarketClock::new(at(100), true, at(10_000), at(400)).unwrap();
        assert_eq!(clock.time_until_next_change(), TimeDelta::seconds(300));
    }

    #[test]
    fn closed_market_counts_down_to_open() {
        let clock = MarketClock::new(at(100), false, at(700), at(5_000)).unwrap();
        assert_eq!(clock.time_until_next_change(), TimeDelta::seconds(600));
    }

    #[test]
    fn rejects_incoherent_clocks() {
        // Dice abierto, pero la próxima apertura llega antes que el cierre
        assert!(MarketClock::new(at(100), true, at(200), at(300)).is_err());
        // Eventos en el pasado
        assert!(MarketClock::new(at(100), false, at(50), at(300)).is_err());
    }
}
