//! Pruebas del diario de órdenes y la watchlist sobre SQLite real (en memoria y en archivo).

// Crate de pruebas: unwrap/expect también en funciones auxiliares sin #[test]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use rust_decimal_macros::dec;
use tickforge_adapter_sqlite::{
    SqliteBarRepository, SqliteOrderJournal, SqliteStore, SqliteWatchlistRepository,
};
use tickforge_application::ports::{
    BarRepository, JournalEntry, OrderJournal, ReconcileOutcome, WatchlistRepository,
};
use tickforge_domain::shared::{Price, Quantity, Symbol};
use tickforge_domain::trading::{
    ClientOrderId, Order, OrderId, OrderRequest, OrderSide, OrderStatus, OrderType, TimeInForce,
};

fn at(seconds: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(seconds, 0).unwrap()
}

fn request(client_order_id: &str) -> OrderRequest {
    OrderRequest::new(
        ClientOrderId::new(client_order_id).unwrap(),
        Symbol::new("AAPL").unwrap(),
        OrderSide::Buy,
        OrderType::Limit {
            limit_price: Price::new(dec!(185.25)).unwrap(),
        },
        Quantity::new(dec!(10)).unwrap(),
        TimeInForce::Day,
    )
    .unwrap()
}

fn intent(client_order_id: &str) -> JournalEntry {
    JournalEntry::Intent {
        request: request(client_order_id),
        at: at(1_000),
    }
}

fn submitted(client_order_id: &str) -> JournalEntry {
    JournalEntry::Submitted {
        order: Order::new(
            OrderId::new(&format!("broker-{client_order_id}")).unwrap(),
            request(client_order_id),
            at(1_001),
        ),
        at: at(1_001),
    }
}

/// Archivo temporal único para probar la persistencia real en disco.
fn temp_database() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir()
        .join(format!("tickforge-test-{nanos}"))
        .join("tickforge.db")
}

#[tokio::test]
async fn intents_without_outcome_are_pending() {
    let journal = SqliteOrderJournal::new(SqliteStore::in_memory().await.unwrap());

    journal.append(intent("tf-1")).await.unwrap();
    journal.append(submitted("tf-1")).await.unwrap();
    journal.append(intent("tf-2")).await.unwrap();

    let pending = journal.pending_intents().await.unwrap();
    assert_eq!(pending, vec![request("tf-2")]);
}

#[tokio::test]
async fn failed_submissions_stay_pending_until_reconciled() {
    // Un error de red puede ocurrir después de que el broker recibió la orden
    let journal = SqliteOrderJournal::new(SqliteStore::in_memory().await.unwrap());
    journal.append(intent("tf-1")).await.unwrap();
    journal
        .append(JournalEntry::SubmissionFailed {
            request: request("tf-1"),
            reason: "timeout".to_owned(),
            at: at(1_002),
        })
        .await
        .unwrap();
    assert_eq!(journal.pending_intents().await.unwrap().len(), 1);

    journal
        .append(JournalEntry::Reconciled {
            client_order_id: ClientOrderId::new("tf-1").unwrap(),
            outcome: ReconcileOutcome::FoundAtBroker {
                order_id: OrderId::new("broker-tf-1").unwrap(),
                status: OrderStatus::New,
            },
            at: at(2_000),
        })
        .await
        .unwrap();
    assert!(journal.pending_intents().await.unwrap().is_empty());
}

#[tokio::test]
async fn every_entry_kind_can_be_written() {
    let journal = SqliteOrderJournal::new(SqliteStore::in_memory().await.unwrap());
    let entries = [
        intent("tf-1"),
        JournalEntry::RiskRejected {
            request: request("tf-2"),
            violations: Vec::new(),
            at: at(1_000),
        },
        submitted("tf-1"),
        JournalEntry::CancelRequested {
            order_id: OrderId::new("broker-tf-1").unwrap(),
            at: at(1_003),
        },
        JournalEntry::Reconciled {
            client_order_id: ClientOrderId::new("tf-3").unwrap(),
            outcome: ReconcileOutcome::NotFoundAtBroker,
            at: at(1_004),
        },
    ];
    for entry in entries {
        journal.append(entry).await.unwrap();
    }
    // Una intención rechazada por riesgo nunca se registra como intención: nada pendiente
    assert!(journal.pending_intents().await.unwrap().is_empty());
}

#[tokio::test]
async fn watchlist_is_empty_until_saved_and_keeps_order() {
    let repository = SqliteWatchlistRepository::new(SqliteStore::in_memory().await.unwrap());
    assert_eq!(repository.load().await.unwrap(), None);

    let symbols: Vec<Symbol> = ["SPY", "AAPL", "BRK.B"]
        .iter()
        .map(|raw| Symbol::new(raw).unwrap())
        .collect();
    repository.save(&symbols).await.unwrap();
    assert_eq!(repository.load().await.unwrap(), Some(symbols.clone()));

    // Guardar reemplaza la lista completa
    repository.save(&symbols[..1]).await.unwrap();
    assert_eq!(
        repository.load().await.unwrap(),
        Some(symbols[..1].to_vec())
    );
}

#[tokio::test]
async fn data_survives_closing_and_reopening_the_file() {
    let path = temp_database();

    {
        let store = SqliteStore::open(&path).await.unwrap();
        SqliteOrderJournal::new(store.clone())
            .append(intent("tf-crash"))
            .await
            .unwrap();
        SqliteWatchlistRepository::new(store.clone())
            .save(&[Symbol::new("NVDA").unwrap()])
            .await
            .unwrap();
        store.close().await;
    }

    // Reabrir: las migraciones ya aplicadas no se repiten y los datos siguen ahí
    let store = SqliteStore::open(&path).await.unwrap();
    let pending = SqliteOrderJournal::new(store.clone())
        .pending_intents()
        .await
        .unwrap();
    let watchlist = SqliteWatchlistRepository::new(store.clone())
        .load()
        .await
        .unwrap();
    store.close().await;

    assert_eq!(pending, vec![request("tf-crash")]);
    assert_eq!(watchlist, Some(vec![Symbol::new("NVDA").unwrap()]));

    if let Some(directory) = path.parent() {
        let _ = std::fs::remove_dir_all(directory);
    }
}

// --- Kill switch (fase 7) ---

#[tokio::test]
async fn kill_switch_is_released_until_engaged_and_survives_reopening() {
    use tickforge_adapter_sqlite::SqliteKillSwitchRepository;
    use tickforge_application::ports::KillSwitchRepository;
    use tickforge_domain::risk::KillSwitch;

    let path = temp_database();
    let engaged = KillSwitch::engaged(at(1_700_000_000), "pánico de prueba");

    {
        let store = SqliteStore::open(&path).await.unwrap();
        let repository = SqliteKillSwitchRepository::new(store.clone());
        assert_eq!(repository.load().await.unwrap(), KillSwitch::Released);
        repository.save(&engaged).await.unwrap();
        // El diario acepta las entradas del kill switch
        SqliteOrderJournal::new(store.clone())
            .append(JournalEntry::KillSwitchEngaged {
                reason: "pánico de prueba".to_owned(),
                at: at(1_700_000_000),
            })
            .await
            .unwrap();
        store.close().await;
    }

    // Tras "reiniciar" la aplicación, el bloqueo sigue activo
    let store = SqliteStore::open(&path).await.unwrap();
    let repository = SqliteKillSwitchRepository::new(store.clone());
    assert_eq!(repository.load().await.unwrap(), engaged);

    repository.save(&KillSwitch::Released).await.unwrap();
    assert_eq!(repository.load().await.unwrap(), KillSwitch::Released);
    store.close().await;

    if let Some(directory) = path.parent() {
        let _ = std::fs::remove_dir_all(directory);
    }
}

#[tokio::test]
async fn intents_are_counted_from_a_point_in_time_and_survive_reopening() {
    let path = temp_database();
    let store = SqliteStore::open(&path).await.unwrap();
    let journal = SqliteOrderJournal::new(store.clone());

    for (id, seconds) in [("tf-1", 1_000), ("tf-2", 1_059), ("tf-3", 1_060)] {
        journal
            .append(JournalEntry::Intent {
                request: request(id),
                at: at(seconds),
            })
            .await
            .unwrap();
    }
    // Las demás entradas no cuentan como órdenes enviadas
    journal.append(submitted("tf-1")).await.unwrap();

    assert_eq!(journal.count_intents_since(at(0)).await.unwrap(), 3);
    assert_eq!(journal.count_intents_since(at(1_059)).await.unwrap(), 2);
    assert_eq!(journal.count_intents_since(at(1_061)).await.unwrap(), 0);

    // El conteo sale del disco: reiniciar la aplicación no reinicia el tope
    store.close().await;
    let reopened = SqliteStore::open(&path).await.unwrap();
    let journal = SqliteOrderJournal::new(reopened.clone());
    assert_eq!(journal.count_intents_since(at(1_000)).await.unwrap(), 3);
    reopened.close().await;
    if let Some(directory) = path.parent() {
        let _ = std::fs::remove_dir_all(directory);
    }
}

fn minute_bar(
    symbol: &str,
    seconds: i64,
    close: rust_decimal::Decimal,
) -> tickforge_domain::market::Bar {
    let price = Price::new(close).unwrap();
    tickforge_domain::market::Bar::new(
        Symbol::new(symbol).unwrap(),
        tickforge_domain::market::Timeframe::Minute1,
        at(seconds),
        tickforge_domain::market::Ohlc {
            open: price,
            high: price,
            low: price,
            close: price,
        },
        Quantity::new(dec!(1250)).unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn bars_are_upserted_and_the_latest_start_is_known_per_symbol() {
    use tickforge_domain::market::Timeframe;

    let store = SqliteStore::in_memory_market_data().await.unwrap();
    let bars = SqliteBarRepository::new(store);
    let aapl = Symbol::new("AAPL").unwrap();

    assert_eq!(
        bars.latest_bar_start(&aapl, Timeframe::Minute1)
            .await
            .unwrap(),
        None
    );
    bars.save_bars(&[
        minute_bar("AAPL", 60, dec!(180.25)),
        minute_bar("AAPL", 120, dec!(180.50)),
        minute_bar("MSFT", 600, dec!(410)),
    ])
    .await
    .unwrap();
    // Repetir una vela la reemplaza (no falla ni duplica)
    bars.save_bars(&[minute_bar("AAPL", 120, dec!(181))])
        .await
        .unwrap();

    assert_eq!(
        bars.latest_bar_start(&aapl, Timeframe::Minute1)
            .await
            .unwrap(),
        Some(at(120))
    );
    assert_eq!(
        bars.latest_bar_start(&aapl, Timeframe::Day1).await.unwrap(),
        None
    );
}

#[tokio::test]
async fn the_market_database_survives_reopening_and_is_never_mixed_with_trading() {
    use tickforge_domain::market::Timeframe;

    let path = temp_database().with_file_name("market.db");
    let store = SqliteStore::open_market_data(&path).await.unwrap();
    SqliteBarRepository::new(store.clone())
        .save_bars(&[minute_bar("AAPL", 60, dec!(180))])
        .await
        .unwrap();
    store.close().await;

    let reopened = SqliteStore::open_market_data(&path).await.unwrap();
    assert_eq!(
        SqliteBarRepository::new(reopened.clone())
            .latest_bar_start(&Symbol::new("AAPL").unwrap(), Timeframe::Minute1)
            .await
            .unwrap(),
        Some(at(60))
    );
    reopened.close().await;

    // La base de mercado no se puede abrir como base de trading, ni al revés
    assert!(SqliteStore::open(&path).await.is_err());
    let trading_path = path.with_file_name("tickforge.db");
    let trading = SqliteStore::open(&trading_path).await.unwrap();
    trading.close().await;
    assert!(SqliteStore::open_market_data(&trading_path).await.is_err());

    if let Some(directory) = path.parent() {
        let _ = std::fs::remove_dir_all(directory);
    }
}
