use std::fmt;

use chrono::TimeDelta;

/// Resolución temporal de una vela.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Timeframe {
    Minute1,
    Minute5,
    Minute15,
    Hour1,
    Day1,
}

impl Timeframe {
    pub const ALL: [Timeframe; 5] = [
        Self::Minute1,
        Self::Minute5,
        Self::Minute15,
        Self::Hour1,
        Self::Day1,
    ];

    /// Duración nominal de la vela. La vela diaria cubre un día calendario aunque
    /// la sesión regular dure 6.5 horas.
    pub fn duration(self) -> TimeDelta {
        match self {
            Self::Minute1 => TimeDelta::minutes(1),
            Self::Minute5 => TimeDelta::minutes(5),
            Self::Minute15 => TimeDelta::minutes(15),
            Self::Hour1 => TimeDelta::hours(1),
            Self::Day1 => TimeDelta::days(1),
        }
    }

    /// Código estable para logs, persistencia y la TUI.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Minute1 => "1m",
            Self::Minute5 => "5m",
            Self::Minute15 => "15m",
            Self::Hour1 => "1h",
            Self::Day1 => "1d",
        }
    }
}

impl fmt::Display for Timeframe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_are_strictly_increasing() {
        for pair in Timeframe::ALL.windows(2) {
            assert!(pair[0].duration() < pair[1].duration());
        }
    }
}
