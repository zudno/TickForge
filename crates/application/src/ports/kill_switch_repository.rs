use async_trait::async_trait;
use tickforge_domain::risk::KillSwitch;

use crate::ports::RepositoryError;

/// Persistencia del kill switch. Debe sobrevivir a reinicios: si la aplicación se cae con el
/// interruptor activo, al volver tiene que seguir activo.
#[async_trait]
pub trait KillSwitchRepository: Send + Sync {
    /// Estado guardado; `Released` si nunca se activó.
    async fn load(&self) -> Result<KillSwitch, RepositoryError>;

    async fn save(&self, kill_switch: &KillSwitch) -> Result<(), RepositoryError>;
}
