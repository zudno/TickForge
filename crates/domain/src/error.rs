//! Errores del dominio.
//!
//! Los mensajes están en español porque pueden llegar a mostrarse en la TUI.

use rust_decimal::Decimal;
use thiserror::Error;

use crate::shared::{Currency, Quantity, Symbol};
use crate::trading::OrderStatus;

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

    #[error("la cantidad debe ser mayor que cero, se recibió {0}")]
    NonPositiveQuantity(Decimal),

    #[error("identificador de orden inválido: '{0}'")]
    InvalidOrderId(String),

    #[error("identificador de cliente inválido: '{0}'")]
    InvalidClientOrderId(String),

    #[error("identificador de ejecución inválido: '{0}'")]
    InvalidFillId(String),

    #[error("el precio {price} no respeta el incremento mínimo de {tick}")]
    InvalidTickSize { price: Decimal, tick: Decimal },

    #[error("transición de estado inválida: {from} -> {to}")]
    InvalidStatusTransition { from: OrderStatus, to: OrderStatus },

    #[error("una orden en estado {0} no admite ejecuciones")]
    FillNotAllowed(OrderStatus),

    #[error("la ejecución de {fill} excede la cantidad pendiente de {remaining}")]
    Overfill { fill: Quantity, remaining: Quantity },

    #[error("instantánea de orden inconsistente: {0}")]
    InconsistentOrderSnapshot(&'static str),

    #[error("no se pueden vender {requested} acciones: la posición solo tiene {available}")]
    InsufficientPosition {
        requested: Quantity,
        available: Quantity,
    },

    #[error("datos de cuenta inválidos: {0}")]
    InvalidAccount(&'static str),

    #[error("vela inválida: {0}")]
    InvalidBar(&'static str),

    #[error("reloj de mercado inválido: {0}")]
    InvalidMarketClock(&'static str),

    #[error("límites de riesgo inválidos: {0}")]
    InvalidRiskLimits(&'static str),

    #[error("se esperaba el símbolo {expected} pero se recibió {actual}")]
    SymbolMismatch { expected: Symbol, actual: Symbol },
}
