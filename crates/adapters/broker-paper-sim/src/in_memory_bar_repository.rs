use std::collections::BTreeMap;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tickforge_application::ports::{BarRepository, RepositoryError};
use tickforge_domain::market::{Bar, Timeframe};
use tickforge_domain::shared::Symbol;

use crate::lock::lock;

/// Clave de una vela: símbolo, resolución e inicio (el mismo orden que la tabla SQLite).
type BarKey = (Symbol, Timeframe, DateTime<Utc>);

/// Histórico de velas en memoria, ordenado por símbolo, resolución e inicio.
#[derive(Debug, Default)]
pub struct InMemoryBarRepository {
    bars: Mutex<BTreeMap<BarKey, Bar>>,
}

impl InMemoryBarRepository {
    pub fn new() -> Self {
        Self::default()
    }

    /// Velas guardadas de un símbolo y resolución, en orden cronológico.
    pub fn bars(&self, symbol: &Symbol, timeframe: Timeframe) -> Vec<Bar> {
        lock(&self.bars)
            .values()
            .filter(|bar| bar.symbol() == symbol && bar.timeframe() == timeframe)
            .cloned()
            .collect()
    }
}

#[async_trait]
impl BarRepository for InMemoryBarRepository {
    async fn save_bars(&self, bars: &[Bar]) -> Result<(), RepositoryError> {
        let mut stored = lock(&self.bars);
        for bar in bars {
            stored.insert(
                (bar.symbol().clone(), bar.timeframe(), bar.starts_at()),
                bar.clone(),
            );
        }
        Ok(())
    }

    async fn latest_bar_start(
        &self,
        symbol: &Symbol,
        timeframe: Timeframe,
    ) -> Result<Option<DateTime<Utc>>, RepositoryError> {
        Ok(lock(&self.bars)
            .values()
            .filter(|bar| bar.symbol() == symbol && bar.timeframe() == timeframe)
            .map(Bar::starts_at)
            .max())
    }
}
