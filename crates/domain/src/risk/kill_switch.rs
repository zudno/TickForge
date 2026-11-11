use chrono::{DateTime, Utc};

/// Interruptor de emergencia: mientras está activo, la política de riesgo rechaza **toda**
/// orden nueva, venga del usuario o de un modelo.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum KillSwitch {
    #[default]
    Released,
    Engaged {
        since: DateTime<Utc>,
        reason: String,
    },
}

impl KillSwitch {
    pub fn engaged(since: DateTime<Utc>, reason: impl Into<String>) -> Self {
        Self::Engaged {
            since,
            reason: reason.into(),
        }
    }

    pub fn is_engaged(&self) -> bool {
        matches!(self, Self::Engaged { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn released_by_default() {
        assert!(!KillSwitch::default().is_engaged());
        assert!(KillSwitch::engaged(DateTime::UNIX_EPOCH, "prueba").is_engaged());
    }
}
