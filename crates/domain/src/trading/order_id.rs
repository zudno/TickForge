use std::fmt;

use crate::error::DomainError;
use crate::trading::identifier::{is_visible_ascii, sanitize};

/// Identificador asignado por el broker (UUID en Alpaca, entero en IBKR).
///
/// Es opaco para el dominio: solo se valida que sea un texto ASCII visible y acotado.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OrderId(String);

impl OrderId {
    pub const MAX_LEN: usize = 128;

    pub fn new(raw: &str) -> Result<Self, DomainError> {
        sanitize(raw, Self::MAX_LEN, is_visible_ascii)
            .map(Self)
            .ok_or_else(|| DomainError::InvalidOrderId(raw.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OrderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Identificador que genera TickForge para cada orden antes de enviarla.
///
/// Garantiza idempotencia: si un envío falla por red y se reintenta, el broker reconoce el
/// mismo `ClientOrderId` y no crea una orden duplicada. Su generación es responsabilidad de
/// la capa de aplicación (puerto de generación de ids); el dominio solo valida el formato.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClientOrderId(String);

impl ClientOrderId {
    /// Límite conservador que aceptan todos los brokers previstos.
    pub const MAX_LEN: usize = 48;

    pub fn new(raw: &str) -> Result<Self, DomainError> {
        sanitize(raw, Self::MAX_LEN, |c| {
            c.is_ascii_alphanumeric() || c == '-' || c == '_'
        })
        .map(Self)
        .ok_or_else(|| DomainError::InvalidClientOrderId(raw.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ClientOrderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_id_accepts_uuid_and_numeric_ids() {
        assert!(OrderId::new("61e69015-8549-4bfd-b9c3-01e75843f47d").is_ok());
        assert!(OrderId::new("1024").is_ok());
    }

    #[test]
    fn order_id_rejects_blank_or_spaced_values() {
        for raw in ["", "   ", "abc def"] {
            assert!(matches!(
                OrderId::new(raw),
                Err(DomainError::InvalidOrderId(_))
            ));
        }
    }

    #[test]
    fn client_order_id_restricts_charset_and_length() {
        assert!(ClientOrderId::new("tf-20261109-0001").is_ok());
        assert!(ClientOrderId::new("tf.0001").is_err());
        assert!(ClientOrderId::new(&"a".repeat(ClientOrderId::MAX_LEN + 1)).is_err());
    }
}
