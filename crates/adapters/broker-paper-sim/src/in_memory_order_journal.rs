use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use tickforge_application::ports::{JournalEntry, JournalError, OrderJournal};

use crate::lock::lock;

/// Diario de órdenes en memoria. Permite simular fallos de escritura en pruebas.
#[derive(Debug, Default)]
pub struct InMemoryOrderJournal {
    entries: Mutex<Vec<JournalEntry>>,
    fail_writes: AtomicBool,
}

impl InMemoryOrderJournal {
    pub fn new() -> Self {
        Self::default()
    }

    /// Copia de todas las entradas registradas, en orden.
    pub fn entries(&self) -> Vec<JournalEntry> {
        lock(&self.entries).clone()
    }

    /// Si es `true`, toda escritura posterior falla con `JournalError::Storage`.
    pub fn set_fail_writes(&self, fail: bool) {
        self.fail_writes.store(fail, Ordering::Relaxed);
    }
}

#[async_trait]
impl OrderJournal for InMemoryOrderJournal {
    async fn append(&self, entry: JournalEntry) -> Result<(), JournalError> {
        if self.fail_writes.load(Ordering::Relaxed) {
            return Err(JournalError::Storage("fallo simulado".to_owned()));
        }
        lock(&self.entries).push(entry);
        Ok(())
    }
}
