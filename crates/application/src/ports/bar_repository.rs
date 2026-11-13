use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tickforge_domain::market::{Bar, Timeframe};
use tickforge_domain::shared::Symbol;

use crate::ports::RepositoryError;

/// Histórico local de velas: la materia prima para entrenar modelos (fase 9).
///
/// Guardar es idempotente: una vela con el mismo símbolo, resolución e inicio reemplaza a la
/// anterior (por ejemplo, la del flujo en vivo por la del histórico del proveedor).
#[async_trait]
pub trait BarRepository: Send + Sync {
    async fn save_bars(&self, bars: &[Bar]) -> Result<(), RepositoryError>;

    /// Inicio de la vela más reciente guardada; `None` si no hay ninguna.
    async fn latest_bar_start(
        &self,
        symbol: &Symbol,
        timeframe: Timeframe,
    ) -> Result<Option<DateTime<Utc>>, RepositoryError>;
}
