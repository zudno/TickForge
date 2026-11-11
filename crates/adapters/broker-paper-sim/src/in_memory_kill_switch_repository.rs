use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use tickforge_application::ports::{KillSwitchRepository, RepositoryError};
use tickforge_domain::risk::KillSwitch;

use crate::lock::lock;

/// Kill switch en memoria, para pruebas y demos. Permite simular fallos de escritura.
#[derive(Debug, Default)]
pub struct InMemoryKillSwitchRepository {
    state: Mutex<KillSwitch>,
    fail_writes: AtomicBool,
}

impl InMemoryKillSwitchRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn current(&self) -> KillSwitch {
        lock(&self.state).clone()
    }

    /// Si es `true`, toda escritura posterior falla con `RepositoryError::Storage`.
    pub fn set_fail_writes(&self, fail: bool) {
        self.fail_writes.store(fail, Ordering::Relaxed);
    }
}

#[async_trait]
impl KillSwitchRepository for InMemoryKillSwitchRepository {
    async fn load(&self) -> Result<KillSwitch, RepositoryError> {
        Ok(lock(&self.state).clone())
    }

    async fn save(&self, kill_switch: &KillSwitch) -> Result<(), RepositoryError> {
        if self.fail_writes.load(Ordering::Relaxed) {
            return Err(RepositoryError::Storage("fallo simulado".to_owned()));
        }
        *lock(&self.state) = kill_switch.clone();
        Ok(())
    }
}
