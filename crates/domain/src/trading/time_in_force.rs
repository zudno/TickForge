use std::fmt;

/// Vigencia de una orden.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimeInForce {
    /// Expira al cierre de la sesión regular si no se ejecuta.
    Day,
    /// Permanece activa hasta ejecutarse o cancelarse (el broker puede imponer un límite de días).
    Gtc,
}

impl TimeInForce {
    /// Código estable para logs y persistencia.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Gtc => "gtc",
        }
    }
}

impl fmt::Display for TimeInForce {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
