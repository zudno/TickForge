//! Subdominio de riesgo: validaciones pre-trade que toda orden debe superar,
//! venga del usuario o de un modelo de ML.

mod kill_switch;
mod risk_limits;
mod risk_policy;
mod risk_violation;

pub use kill_switch::KillSwitch;
pub use risk_limits::RiskLimits;
pub use risk_policy::{RiskAssessment, RiskContext, RiskPolicy};
pub use risk_violation::RiskViolation;
