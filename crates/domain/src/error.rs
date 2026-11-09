//! Errores del dominio.
//!
//! Los mensajes están en español porque pueden llegar a mostrarse en la TUI.

use rust_decimal::Decimal;
use thiserror::Error;

use crate::shared::Currency;

/// Violación de una regla o invariante del dominio.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DomainError {
    #[error("símbolo inválido: '{0}'")]
    InvalidSymbol(String),

    #[error("precio inválido: {0} (debe ser mayor que cero)")]
    InvalidPrice(Decimal),

    #[error("cantidad inválida: {0} (no puede ser negativa)")]
    InvalidQuantity(Decimal),

    #[error("{value} excede la precisión permitida de {max_scale} decimales")]
    PrecisionExceeded { value: Decimal, max_scale: u32 },

    #[error("no se pueden operar montos en monedas distintas: {left} y {right}")]
    CurrencyMismatch { left: Currency, right: Currency },

    #[error("desbordamiento aritmético en la operación '{0}'")]
    ArithmeticOverflow(&'static str),
}
