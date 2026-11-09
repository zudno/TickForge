use std::fmt;
use std::str::FromStr;

use crate::error::DomainError;

/// Ticker de un instrumento listado en EE. UU. (por ejemplo `AAPL` o `BRK.B`).
///
/// Se normaliza a mayúsculas y sin espacios. La traducción al formato particular de cada
/// broker (por ejemplo `BRK B` en IBKR) es responsabilidad del adaptador, no del dominio.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Symbol(String);

impl Symbol {
    /// Longitud máxima aceptada; cubre tickers con sufijo de clase (`BRK.B`, `BF-B`).
    pub const MAX_LEN: usize = 10;

    pub fn new(raw: &str) -> Result<Self, DomainError> {
        let normalized = raw.trim().to_ascii_uppercase();

        if !Self::is_valid(&normalized) {
            return Err(DomainError::InvalidSymbol(raw.to_owned()));
        }
        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Reglas: 1..=MAX_LEN caracteres, empieza con letra, solo `A-Z`, `0-9`, `.` o `-`,
    /// y no termina en separador.
    fn is_valid(candidate: &str) -> bool {
        let starts_with_letter = candidate.starts_with(|c: char| c.is_ascii_uppercase());
        let allowed_chars = candidate
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '.' || c == '-');
        let ends_cleanly = !candidate.ends_with(['.', '-']);

        starts_with_letter && allowed_chars && ends_cleanly && candidate.len() <= Self::MAX_LEN
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for Symbol {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl AsRef<str> for Symbol {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn normalizes_case_and_whitespace() {
        let symbol = Symbol::new("  aapl ").unwrap();
        assert_eq!(symbol.as_str(), "AAPL");
    }

    #[test]
    fn accepts_share_class_suffixes() {
        assert!(Symbol::new("BRK.B").is_ok());
        assert!(Symbol::new("BF-B").is_ok());
    }

    #[test]
    fn rejects_invalid_symbols() {
        for raw in [
            "",
            "   ",
            "1ABC",
            "AA PL",
            "AAPL$",
            "BRK.",
            "-X",
            "ABCDEFGHIJK",
        ] {
            assert!(
                matches!(Symbol::new(raw), Err(DomainError::InvalidSymbol(_))),
                "debió rechazar {raw:?}"
            );
        }
    }

    #[test]
    fn parses_from_str() {
        let symbol: Symbol = "msft".parse().unwrap();
        assert_eq!(symbol.to_string(), "MSFT");
    }

    proptest! {
        // Normalizar un símbolo ya válido no lo cambia (idempotencia)
        #[test]
        fn normalization_is_idempotent(raw in "[A-Za-z][A-Za-z0-9]{0,4}") {
            let first = Symbol::new(&raw).unwrap();
            let second = Symbol::new(first.as_str()).unwrap();
            prop_assert_eq!(first, second);
        }
    }
}
