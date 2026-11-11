use async_trait::async_trait;
use rusqlite::params;
use tickforge_application::ports::{JournalEntry, JournalError, OrderJournal, ReconcileOutcome};
use tickforge_domain::trading::OrderRequest;

use crate::rows::{request_columns, request_from_columns, status_code, timestamp};
use crate::store::SqliteStore;

/// Diario de órdenes persistente. Cada entrada es una fila nueva: nunca se actualiza ni se
/// borra, para conservar la historia completa de lo que se intentó y de lo que pasó.
#[derive(Debug, Clone)]
pub struct SqliteOrderJournal {
    store: SqliteStore,
}

impl SqliteOrderJournal {
    pub fn new(store: SqliteStore) -> Self {
        Self { store }
    }
}

/// Fila a insertar; los campos ausentes quedan en `NULL`.
#[derive(Default)]
struct JournalRow {
    recorded_at: String,
    kind: &'static str,
    client_order_id: Option<String>,
    broker_order_id: Option<String>,
    symbol: Option<String>,
    side: Option<&'static str>,
    order_type: Option<&'static str>,
    limit_price: Option<String>,
    quantity: Option<String>,
    time_in_force: Option<&'static str>,
    status: Option<&'static str>,
    detail: Option<String>,
}

impl JournalRow {
    fn with_request(kind: &'static str, request: &OrderRequest, recorded_at: String) -> Self {
        let columns = request_columns(request);
        Self {
            recorded_at,
            kind,
            client_order_id: Some(columns.client_order_id),
            symbol: Some(columns.symbol),
            side: Some(columns.side),
            order_type: Some(columns.order_type),
            limit_price: columns.limit_price,
            quantity: Some(columns.quantity),
            time_in_force: Some(columns.time_in_force),
            ..Self::default()
        }
    }
}

fn to_row(entry: &JournalEntry) -> JournalRow {
    match entry {
        JournalEntry::Intent { request, at } => {
            JournalRow::with_request("intent", request, timestamp(*at))
        }
        JournalEntry::RiskRejected {
            request,
            violations,
            at,
        } => {
            let reasons: Vec<String> = violations.iter().map(ToString::to_string).collect();
            JournalRow {
                detail: Some(reasons.join("; ")),
                ..JournalRow::with_request("risk_rejected", request, timestamp(*at))
            }
        }
        JournalEntry::Submitted { order, at } => JournalRow {
            broker_order_id: Some(order.id().as_str().to_owned()),
            status: Some(status_code(order.status())),
            ..JournalRow::with_request("submitted", order.request(), timestamp(*at))
        },
        JournalEntry::SubmissionFailed {
            request,
            reason,
            at,
        } => JournalRow {
            detail: Some(reason.clone()),
            ..JournalRow::with_request("submission_failed", request, timestamp(*at))
        },
        JournalEntry::CancelRequested { order_id, at } => JournalRow {
            recorded_at: timestamp(*at),
            kind: "cancel_requested",
            broker_order_id: Some(order_id.as_str().to_owned()),
            ..JournalRow::default()
        },
        JournalEntry::Reconciled {
            client_order_id,
            outcome,
            at,
        } => {
            let (broker_order_id, status, detail) = match outcome {
                ReconcileOutcome::FoundAtBroker { order_id, status } => (
                    Some(order_id.as_str().to_owned()),
                    Some(status_code(*status)),
                    "found_at_broker",
                ),
                ReconcileOutcome::NotFoundAtBroker => (None, None, "not_found_at_broker"),
            };
            JournalRow {
                recorded_at: timestamp(*at),
                kind: "reconciled",
                client_order_id: Some(client_order_id.as_str().to_owned()),
                broker_order_id,
                status,
                detail: Some(detail.to_owned()),
                ..JournalRow::default()
            }
        }
        JournalEntry::KillSwitchEngaged { reason, at } => JournalRow {
            recorded_at: timestamp(*at),
            kind: "kill_switch_engaged",
            detail: Some(reason.clone()),
            ..JournalRow::default()
        },
        JournalEntry::KillSwitchReleased { at } => JournalRow {
            recorded_at: timestamp(*at),
            kind: "kill_switch_released",
            ..JournalRow::default()
        },
    }
}

/// Columnas de una intención leída de la base, antes de validarla con el dominio.
struct IntentColumns {
    client_order_id: String,
    symbol: String,
    side: String,
    order_type: String,
    limit_price: Option<String>,
    quantity: String,
    time_in_force: String,
}

#[async_trait]
impl OrderJournal for SqliteOrderJournal {
    async fn append(&self, entry: JournalEntry) -> Result<(), JournalError> {
        let row = to_row(&entry);
        self.store
            .with_connection(move |connection| {
                connection.execute(
                    "INSERT INTO order_journal (recorded_at, kind, client_order_id, \
                     broker_order_id, symbol, side, order_type, limit_price, quantity, \
                     time_in_force, status, detail) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    params![
                        row.recorded_at,
                        row.kind,
                        row.client_order_id,
                        row.broker_order_id,
                        row.symbol,
                        row.side,
                        row.order_type,
                        row.limit_price,
                        row.quantity,
                        row.time_in_force,
                        row.status,
                        row.detail,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|error| JournalError::Storage(error.to_string()))
    }

    async fn pending_intents(&self) -> Result<Vec<OrderRequest>, JournalError> {
        // Intenciones sin `submitted` ni `reconciled` para el mismo client_order_id
        let rows = self
            .store
            .with_connection(|connection| {
                let mut statement = connection.prepare(
                    "SELECT client_order_id, symbol, side, order_type, limit_price, quantity, \
                     time_in_force \
                     FROM order_journal AS intent \
                     WHERE intent.kind = 'intent' \
                     AND NOT EXISTS ( \
                         SELECT 1 FROM order_journal AS outcome \
                         WHERE outcome.client_order_id = intent.client_order_id \
                         AND outcome.kind IN ('submitted', 'reconciled') \
                     ) \
                     ORDER BY intent.id",
                )?;
                let rows = statement.query_map([], |row| {
                    Ok(IntentColumns {
                        client_order_id: row.get(0)?,
                        symbol: row.get(1)?,
                        side: row.get(2)?,
                        order_type: row.get(3)?,
                        limit_price: row.get(4)?,
                        quantity: row.get(5)?,
                        time_in_force: row.get(6)?,
                    })
                })?;
                rows.collect::<Result<Vec<_>, _>>()
            })
            .await
            .map_err(|error| JournalError::Storage(error.to_string()))?;

        rows.into_iter()
            .map(|columns| {
                request_from_columns(
                    &columns.client_order_id,
                    &columns.symbol,
                    &columns.side,
                    &columns.order_type,
                    columns.limit_price.as_deref(),
                    &columns.quantity,
                    &columns.time_in_force,
                )
                .map_err(JournalError::Corrupted)
            })
            .collect()
    }
}
