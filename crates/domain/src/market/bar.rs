use chrono::{DateTime, Utc};

use crate::error::DomainError;
use crate::market::Timeframe;
use crate::shared::{Price, Quantity, Symbol};

/// Precios de apertura, máximo, mínimo y cierre de una vela.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ohlc {
    pub open: Price,
    pub high: Price,
    pub low: Price,
    pub close: Price,
}

/// Vela (barra) de precios para un instrumento y una resolución temporal.
///
/// Es la unidad básica para los gráficos de la TUI y para el cálculo de features de ML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bar {
    symbol: Symbol,
    timeframe: Timeframe,
    starts_at: DateTime<Utc>,
    ohlc: Ohlc,
    volume: Quantity,
}

impl Bar {
    pub fn new(
        symbol: Symbol,
        timeframe: Timeframe,
        starts_at: DateTime<Utc>,
        ohlc: Ohlc,
        volume: Quantity,
    ) -> Result<Self, DomainError> {
        let Ohlc {
            open,
            high,
            low,
            close,
        } = ohlc;

        // El máximo y el mínimo deben contener a la apertura y al cierre
        if low > high {
            return Err(DomainError::InvalidBar("el mínimo supera al máximo"));
        }
        if open > high || close > high {
            return Err(DomainError::InvalidBar(
                "la apertura o el cierre superan al máximo",
            ));
        }
        if open < low || close < low {
            return Err(DomainError::InvalidBar(
                "la apertura o el cierre están por debajo del mínimo",
            ));
        }

        Ok(Self {
            symbol,
            timeframe,
            starts_at,
            ohlc,
            volume,
        })
    }

    pub fn symbol(&self) -> &Symbol {
        &self.symbol
    }

    pub fn timeframe(&self) -> Timeframe {
        self.timeframe
    }

    pub fn starts_at(&self) -> DateTime<Utc> {
        self.starts_at
    }

    /// Inicio de la siguiente vela (límite exclusivo de esta).
    pub fn ends_at(&self) -> DateTime<Utc> {
        // Suma saturada: evita un `panic` por desbordamiento en fechas extremas
        self.starts_at
            .checked_add_signed(self.timeframe.duration())
            .unwrap_or(DateTime::<Utc>::MAX_UTC)
    }

    pub fn ohlc(&self) -> Ohlc {
        self.ohlc
    }

    pub fn open(&self) -> Price {
        self.ohlc.open
    }

    pub fn high(&self) -> Price {
        self.ohlc.high
    }

    pub fn low(&self) -> Price {
        self.ohlc.low
    }

    pub fn close(&self) -> Price {
        self.ohlc.close
    }

    pub fn volume(&self) -> Quantity {
        self.volume
    }

    /// Vela alcista: cierra por encima de la apertura.
    pub fn is_bullish(&self) -> bool {
        self.ohlc.close > self.ohlc.open
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;

    fn ohlc(open: Decimal, high: Decimal, low: Decimal, close: Decimal) -> Ohlc {
        Ohlc {
            open: Price::new(open).unwrap(),
            high: Price::new(high).unwrap(),
            low: Price::new(low).unwrap(),
            close: Price::new(close).unwrap(),
        }
    }

    fn bar(ohlc: Ohlc) -> Result<Bar, DomainError> {
        Bar::new(
            Symbol::new("AAPL").unwrap(),
            Timeframe::Minute5,
            DateTime::from_timestamp(0, 0).unwrap(),
            ohlc,
            Quantity::new(dec!(1500)).unwrap(),
        )
    }

    #[test]
    fn accepts_consistent_bar() {
        let bar = bar(ohlc(dec!(10), dec!(12), dec!(9), dec!(11))).unwrap();
        assert!(bar.is_bullish());
        assert_eq!(bar.ends_at(), DateTime::from_timestamp(300, 0).unwrap());
    }

    #[test]
    fn rejects_inconsistent_bars() {
        let invalid = [
            ohlc(dec!(10), dec!(9), dec!(11), dec!(10)),
            ohlc(dec!(13), dec!(12), dec!(9), dec!(11)),
            ohlc(dec!(10), dec!(12), dec!(9), dec!(8)),
        ];
        for case in invalid {
            assert!(matches!(bar(case), Err(DomainError::InvalidBar(_))));
        }
    }
}
