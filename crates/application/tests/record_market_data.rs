//! Grabación del histórico de velas (fase 9a) contra el simulador.

// Crate de pruebas: unwrap/expect también en funciones auxiliares sin #[test]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, TimeDelta, Utc};
use futures::stream::BoxStream;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tickforge_adapter_paper_sim::{InMemoryBarRepository, ManualClock, PaperBroker};
use tickforge_application::ports::{MarketDataError, MarketDataFeed, MarketEvent};
use tickforge_application::use_cases::market::RecordMarketDataUseCase;
use tickforge_domain::market::{Bar, Ohlc, Quote, Timeframe};
use tickforge_domain::shared::{Money, Price, Quantity, Symbol};

/// 2024-03-01 15:00 UTC.
const NOW: i64 = 1_709_305_200;

fn at(seconds: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(seconds, 0).unwrap()
}

fn symbol(raw: &str) -> Symbol {
    Symbol::new(raw).unwrap()
}

fn bar(symbol: &Symbol, starts_at: DateTime<Utc>, close: Decimal) -> Bar {
    let price = Price::new(close).unwrap();
    Bar::new(
        symbol.clone(),
        Timeframe::Minute1,
        starts_at,
        Ohlc {
            open: price,
            high: price,
            low: price,
            close: price,
        },
        Quantity::new(dec!(100)).unwrap(),
    )
    .unwrap()
}

/// Una vela por minuto durante las últimas `minutes`, terminando en `NOW`.
fn minute_bars(symbol: &Symbol, minutes: i64) -> Vec<Bar> {
    (1..=minutes)
        .rev()
        .map(|ago| bar(symbol, at(NOW - ago * 60), dec!(180)))
        .collect()
}

struct Setup {
    clock: Arc<ManualClock>,
    broker: Arc<PaperBroker>,
    repository: Arc<InMemoryBarRepository>,
    recorder: RecordMarketDataUseCase,
}

fn setup() -> Setup {
    let clock = Arc::new(ManualClock::new(at(NOW)));
    let broker = Arc::new(PaperBroker::new(clock.clone(), Money::usd(dec!(10000))));
    let repository = Arc::new(InMemoryBarRepository::new());
    let recorder = RecordMarketDataUseCase::new(broker.clone(), repository.clone(), clock.clone());
    Setup {
        clock,
        broker,
        repository,
        recorder,
    }
}

#[tokio::test]
async fn live_bars_are_stored_and_replaced_by_later_versions() {
    let setup = setup();
    let aapl = symbol("AAPL");
    let starts_at = at(NOW - 60);

    setup
        .recorder
        .record(&bar(&aapl, starts_at, dec!(180)))
        .await
        .unwrap();
    // La misma vela corregida reemplaza a la anterior en lugar de duplicarse
    setup
        .recorder
        .record(&bar(&aapl, starts_at, dec!(181)))
        .await
        .unwrap();

    let stored = setup.repository.bars(&aapl, Timeframe::Minute1);
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].close().value(), dec!(181));
}

#[tokio::test]
async fn backfill_downloads_the_window_then_only_what_is_new() {
    let setup = setup();
    let aapl = symbol("AAPL");
    setup
        .broker
        .set_bars(aapl.clone(), Timeframe::Minute1, minute_bars(&aapl, 120));

    // Primera descarga: solo la última hora pedida
    let report = setup
        .recorder
        .backfill(
            std::slice::from_ref(&aapl),
            Timeframe::Minute1,
            TimeDelta::hours(1),
        )
        .await;
    assert_eq!(report.saved(), 60);
    assert_eq!(report.failed(), 0);

    // Diez minutos después hay diez velas nuevas: se bajan esas y la última ya guardada (que
    // pudo estar incompleta) se vuelve a pedir y se reemplaza
    setup.clock.advance(TimeDelta::minutes(10));
    let mut bars = minute_bars(&aapl, 120);
    bars.extend((0..10).map(|minute| bar(&aapl, at(NOW + minute * 60), dec!(182))));
    setup
        .broker
        .set_bars(aapl.clone(), Timeframe::Minute1, bars);

    let report = setup
        .recorder
        .backfill(
            std::slice::from_ref(&aapl),
            Timeframe::Minute1,
            TimeDelta::hours(1),
        )
        .await;
    assert_eq!(report.saved(), 11);
    assert_eq!(setup.repository.bars(&aapl, Timeframe::Minute1).len(), 70);
}

/// Proveedor que delega en el simulador pero falla siempre con MSFT.
struct FailsForMsft(Arc<PaperBroker>);

#[async_trait]
impl MarketDataFeed for FailsForMsft {
    async fn latest_quote(&self, symbol: &Symbol) -> Result<Quote, MarketDataError> {
        self.0.latest_quote(symbol).await
    }

    async fn bars(
        &self,
        symbol: &Symbol,
        timeframe: Timeframe,
        limit: usize,
    ) -> Result<Vec<Bar>, MarketDataError> {
        self.0.bars(symbol, timeframe, limit).await
    }

    async fn bars_between(
        &self,
        symbol: &Symbol,
        timeframe: Timeframe,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Bar>, MarketDataError> {
        if symbol.as_str() == "MSFT" {
            return Err(MarketDataError::RateLimited);
        }
        self.0.bars_between(symbol, timeframe, start, end).await
    }

    async fn subscribe(
        &self,
        symbols: &[Symbol],
    ) -> Result<BoxStream<'static, MarketEvent>, MarketDataError> {
        self.0.subscribe(symbols).await
    }
}

#[tokio::test]
async fn a_failing_symbol_does_not_stop_the_others() {
    let setup = setup();
    let aapl = symbol("AAPL");
    setup
        .broker
        .set_bars(aapl.clone(), Timeframe::Minute1, minute_bars(&aapl, 5));
    let recorder = RecordMarketDataUseCase::new(
        Arc::new(FailsForMsft(setup.broker.clone())),
        setup.repository.clone(),
        setup.clock.clone(),
    );

    let report = recorder
        .backfill(
            &[symbol("MSFT"), aapl.clone()],
            Timeframe::Minute1,
            TimeDelta::hours(1),
        )
        .await;

    assert_eq!(report.failed(), 1);
    assert!(report.symbols[0].result.is_err());
    assert_eq!(report.symbols[1].result, Ok(5));
    assert_eq!(report.saved(), 5);
}
