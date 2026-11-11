use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, params};
use tickforge_application::ports::{KillSwitchRepository, RepositoryError};
use tickforge_domain::risk::KillSwitch;

use crate::rows::timestamp;
use crate::store::SqliteStore;

/// Kill switch guardado en SQLite (una sola fila). Sobrevive a reinicios y caídas.
#[derive(Debug, Clone)]
pub struct SqliteKillSwitchRepository {
    store: SqliteStore,
}

impl SqliteKillSwitchRepository {
    pub fn new(store: SqliteStore) -> Self {
        Self { store }
    }
}

#[async_trait]
impl KillSwitchRepository for SqliteKillSwitchRepository {
    async fn load(&self) -> Result<KillSwitch, RepositoryError> {
        let row = self
            .store
            .with_connection(|connection| {
                connection
                    .query_row(
                        "SELECT engaged, since, reason FROM kill_switch WHERE id = 1",
                        [],
                        |row| {
                            Ok((
                                row.get::<_, bool>(0)?,
                                row.get::<_, Option<String>>(1)?,
                                row.get::<_, Option<String>>(2)?,
                            ))
                        },
                    )
                    .optional()
            })
            .await
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;

        match row {
            None | Some((false, _, _)) => Ok(KillSwitch::Released),
            Some((true, since, reason)) => {
                let since = since.ok_or_else(|| {
                    RepositoryError::Corrupted("kill switch activo sin fecha".to_owned())
                })?;
                let since = DateTime::parse_from_rfc3339(&since)
                    .map(|instant| instant.with_timezone(&Utc))
                    .map_err(|error| RepositoryError::Corrupted(error.to_string()))?;
                Ok(KillSwitch::Engaged {
                    since,
                    reason: reason.unwrap_or_default(),
                })
            }
        }
    }

    async fn save(&self, kill_switch: &KillSwitch) -> Result<(), RepositoryError> {
        let (engaged, since, reason) = match kill_switch {
            KillSwitch::Released => (false, None, None),
            KillSwitch::Engaged { since, reason } => {
                (true, Some(timestamp(*since)), Some(reason.clone()))
            }
        };
        self.store
            .with_connection(move |connection| {
                connection.execute(
                    "INSERT INTO kill_switch (id, engaged, since, reason) VALUES (1, ?1, ?2, ?3) \
                     ON CONFLICT (id) DO UPDATE SET \
                     engaged = excluded.engaged, since = excluded.since, reason = excluded.reason",
                    params![engaged, since, reason],
                )?;
                Ok(())
            })
            .await
            .map_err(|error| RepositoryError::Storage(error.to_string()))
    }
}
