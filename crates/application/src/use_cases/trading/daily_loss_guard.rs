use tickforge_domain::portfolio::Account;
use tickforge_domain::risk::RiskLimits;

use crate::error::ApplicationError;
use crate::use_cases::trading::{KillSwitchReport, KillSwitchUseCase, TradingPorts};

/// Vigila la pérdida del día y, si alcanza el máximo, activa el kill switch por su cuenta:
/// bloquea las órdenes nuevas y cancela las abiertas.
///
/// La política de riesgo ya rechaza órdenes nuevas con la pérdida alcanzada; esta vigilancia
/// además cancela lo que quedó abierto (que podría ejecutarse y aumentar la exposición) y deja
/// el bloqueo visible y registrado en el diario. Usa la misma regla que la política
/// (`RiskLimits::daily_loss_breach`), así ambas deciden igual.
pub struct DailyLossGuard {
    kill_switch: KillSwitchUseCase,
    limits: RiskLimits,
}

impl DailyLossGuard {
    pub fn new(ports: TradingPorts, limits: RiskLimits) -> Self {
        Self {
            kill_switch: KillSwitchUseCase::new(ports),
            limits,
        }
    }

    /// Revisa la cuenta (normalmente recién cargada). Devuelve el reporte si activó el kill
    /// switch; `None` si no hacía falta o si ya estaba activo (no se repiten cancelaciones).
    pub async fn check(
        &self,
        account: &Account,
    ) -> Result<Option<KillSwitchReport>, ApplicationError> {
        let Some(loss) = self.limits.daily_loss_breach(account)? else {
            return Ok(None);
        };
        if self.kill_switch.status().await?.is_engaged() {
            return Ok(None);
        }
        let limit = self
            .limits
            .max_daily_loss()
            .map_or_else(String::new, |limit| format!(" (máximo {limit})"));
        let reason = format!("pérdida del día de {loss}{limit}");
        Ok(Some(self.kill_switch.engage(&reason).await?))
    }
}
