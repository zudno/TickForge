use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde_json::Number;
use tickforge_domain::DomainError;

/// Dato de Alpaca que no se pudo traducir al dominio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MappingError(pub String);

impl fmt::Display for MappingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<DomainError> for MappingError {
    fn from(error: DomainError) -> Self {
        Self(error.to_string())
    }
}

/// Decimal desde texto, aceptando también notación científica (`1e-5`).
pub(crate) fn decimal(field: &str, raw: &str) -> Result<Decimal, MappingError> {
    Decimal::from_str(raw)
        .or_else(|_| Decimal::from_scientific(raw))
        .map_err(|_| {
            MappingError(format!(
                "el campo '{field}' no es un decimal válido: '{raw}'"
            ))
        })
}

/// Decimal desde un número JSON.
///
/// `serde_json` guarda el número como `f64`, pero su representación textual es la más corta
/// que reproduce el valor exacto, así que precios como `189.12` se recuperan sin error.
pub(crate) fn number(field: &str, value: &Number) -> Result<Decimal, MappingError> {
    decimal(field, &value.to_string())
}

/// Marca de tiempo RFC 3339 (con o sin zona horaria explícita) convertida a UTC.
pub(crate) fn timestamp(field: &str, raw: &str) -> Result<DateTime<Utc>, MappingError> {
    DateTime::parse_from_rfc3339(raw)
        .map(|instant| instant.with_timezone(&Utc))
        .map_err(|_| {
            MappingError(format!(
                "el campo '{field}' no es una fecha RFC 3339: '{raw}'"
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn parses_plain_and_scientific_decimals() {
        assert_eq!(decimal("x", "189.25").unwrap(), dec!(189.25));
        assert_eq!(decimal("x", "1e-5").unwrap(), dec!(0.00001));
        assert!(decimal("x", "abc").is_err());
    }

    #[test]
    fn json_floats_keep_their_decimal_representation() {
        let value: Number = serde_json::from_str("189.12").unwrap();
        assert_eq!(number("x", &value).unwrap(), dec!(189.12));
    }

    #[test]
    fn converts_offsets_to_utc() {
        let parsed = timestamp("t", "2024-01-02T09:30:00-05:00").unwrap();
        assert_eq!(parsed.to_rfc3339(), "2024-01-02T14:30:00+00:00");
        assert!(timestamp("t", "yesterday").is_err());
    }
}
