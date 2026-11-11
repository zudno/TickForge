use chrono::TimeDelta;
use rust_decimal::Decimal;
use thiserror::Error;

use crate::shared::{Money, Quantity};
use crate::trading::OrderSide;

/// Motivo por el que la política de riesgo bloquea una orden.
///
/// Los mensajes están en español porque se muestran en la TUI.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RiskViolation {
    #[error("la cuenta tiene la operación bloqueada por el broker")]
    TradingBlocked,

    #[error("el kill switch está activo ({reason}): no se aceptan órdenes nuevas")]
    KillSwitchEngaged { reason: String },

    #[error("la cotización tiene {age_ms} ms de antigüedad (máximo {max_ms} ms)")]
    StaleQuote { age_ms: i64, max_ms: i64 },

    #[error("no hay precio de mercado para una orden de {side}")]
    NoExecutablePrice { side: OrderSide },

    #[error("el valor de la orden ({notional}) supera el máximo por orden ({limit})")]
    OrderNotionalExceeded { notional: Money, limit: Money },

    #[error("poder de compra insuficiente: se requieren {required}, hay {available}")]
    InsufficientBuyingPower { required: Money, available: Money },

    #[error("la posición resultante ({projected}) supera el máximo por símbolo ({limit})")]
    PositionLimitExceeded { projected: Money, limit: Money },

    #[error(
        "no se pueden vender {requested} acciones: la posición tiene {held} (ventas en corto no soportadas)"
    )]
    InsufficientPosition { requested: Quantity, held: Quantity },

    #[error("el precio límite se desvía {deviation_pct}% del mercado (máximo {limit_pct}%)")]
    PriceDeviationExceeded {
        deviation_pct: Decimal,
        limit_pct: Decimal,
    },
}

impl RiskViolation {
    pub(crate) fn stale_quote(age: TimeDelta, max_age: TimeDelta) -> Self {
        Self::StaleQuote {
            age_ms: age.num_milliseconds(),
            max_ms: max_age.num_milliseconds(),
        }
    }
}
