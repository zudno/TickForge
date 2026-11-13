//! Pruebas de la reconexión automática con tiempo simulado (`start_paused`): las esperas
//! exponenciales se verifican sin esperar de verdad.

use std::sync::atomic::{AtomicU32, Ordering};

use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::stream::{self, BoxStream};
use rust_decimal_macros::dec;
use tickforge_application::ports::{
    BarRepository, MarketDataError, MarketDataFeed, MarketEvent, RepositoryError,
};
use tickforge_domain::market::{Bar, Ohlc, Quote, Timeframe};
use tickforge_domain::shared::{Price, Quantity};
use tokio::sync::mpsc;

use super::*;

/// Proveedor que falla a propósito: la primera conexión se corta al instante, la segunda no
/// se puede abrir y la tercera queda estable.
struct FlakyFeed {
    subscriptions: AtomicU32,
}

#[async_trait]
impl MarketDataFeed for FlakyFeed {
    async fn latest_quote(&self, symbol: &Symbol) -> Result<Quote, MarketDataError> {
        Err(MarketDataError::NoData(symbol.clone()))
    }

    async fn bars(
        &self,
        symbol: &Symbol,
        _timeframe: Timeframe,
        _limit: usize,
    ) -> Result<Vec<Bar>, MarketDataError> {
        Err(MarketDataError::NoData(symbol.clone()))
    }

    async fn bars_between(
        &self,
        symbol: &Symbol,
        _timeframe: Timeframe,
        _start: DateTime<Utc>,
        _end: DateTime<Utc>,
    ) -> Result<Vec<Bar>, MarketDataError> {
        Err(MarketDataError::NoData(symbol.clone()))
    }

    async fn subscribe(
        &self,
        _symbols: &[Symbol],
    ) -> Result<BoxStream<'static, MarketEvent>, MarketDataError> {
        match self.subscriptions.fetch_add(1, Ordering::SeqCst) {
            0 => Ok(stream::empty().boxed()),
            1 => Err(MarketDataError::Unavailable("servidor caído".to_owned())),
            _ => Ok(stream::pending().boxed()),
        }
    }
}

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        DateTime::UNIX_EPOCH
    }
}

fn watch_market() -> Arc<WatchMarketUseCase> {
    let feed = Arc::new(FlakyFeed {
        subscriptions: AtomicU32::new(0),
    });
    Arc::new(WatchMarketUseCase::new(feed, Arc::new(FixedClock)))
}

/// Histórico de velas en memoria para verificar qué se grabó.
#[derive(Default)]
struct RecordedBars(Mutex<Vec<Bar>>);

#[async_trait]
impl BarRepository for RecordedBars {
    async fn save_bars(&self, bars: &[Bar]) -> Result<(), RepositoryError> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .extend_from_slice(bars);
        Ok(())
    }

    async fn latest_bar_start(
        &self,
        _symbol: &Symbol,
        _timeframe: Timeframe,
    ) -> Result<Option<DateTime<Utc>>, RepositoryError> {
        Ok(None)
    }
}

fn recorder_with(
    feed: Arc<dyn MarketDataFeed>,
    bars: Arc<RecordedBars>,
) -> Arc<RecordMarketDataUseCase> {
    Arc::new(RecordMarketDataUseCase::new(
        feed,
        bars,
        Arc::new(FixedClock),
    ))
}

fn recorder() -> Arc<RecordMarketDataUseCase> {
    let feed = Arc::new(FlakyFeed {
        subscriptions: AtomicU32::new(0),
    });
    recorder_with(feed, Arc::new(RecordedBars::default()))
}

/// Versión legible de una acción, para comparar la secuencia completa.
fn label(action: &Action) -> String {
    match action {
        Action::WatchlistSeeded(_) => "foto".to_owned(),
        Action::StreamConnected => "conectado".to_owned(),
        Action::StreamDisconnected(_) => "desconectado".to_owned(),
        Action::StreamReconnecting { attempt, delay } => {
            format!("reintento {attempt} en {}s", delay.as_secs())
        }
        other => format!("{other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn market_stream_reconnects_with_growing_delays() {
    let (sender, mut receiver) = mpsc::unbounded_channel();
    let task = tokio::spawn(run_market_stream(
        watch_market(),
        recorder(),
        vec![Symbol::new("AAPL").unwrap()],
        sender,
    ));

    let start = Instant::now();
    let mut sequence = Vec::new();
    let mut connections = 0;
    while connections < 2 {
        let action = receiver.recv().await.unwrap();
        if action == Action::StreamConnected {
            connections += 1;
        }
        sequence.push(label(&action));
    }
    task.abort();

    assert_eq!(
        sequence,
        [
            "foto",
            "conectado",
            "desconectado",
            "reintento 1 en 1s",
            "foto",
            "desconectado",
            "reintento 2 en 2s",
            "foto",
            "conectado",
        ]
    );
    // Solo pasó el tiempo de las dos esperas (1 s + 2 s), simulado
    assert_eq!(start.elapsed(), Duration::from_secs(3));
}

#[tokio::test(start_paused = true)]
async fn closing_the_tui_stops_the_reconnection_loop() {
    let (sender, receiver) = mpsc::unbounded_channel();
    // Sin receptor (la TUI se cerró): la tarea debe terminar sola, no reintentar para siempre
    drop(receiver);

    let task = tokio::spawn(run_market_stream(
        watch_market(),
        recorder(),
        vec![Symbol::new("AAPL").unwrap()],
        sender,
    ));
    let finished = tokio::time::timeout(Duration::from_secs(600), task).await;
    assert!(finished.is_ok());
}

/// Proveedor que entrega una vela y luego se queda en silencio.
struct OneBarFeed;

fn sample_bar() -> Bar {
    let price = Price::new(dec!(180)).unwrap();
    Bar::new(
        Symbol::new("AAPL").unwrap(),
        Timeframe::Minute1,
        DateTime::UNIX_EPOCH,
        Ohlc {
            open: price,
            high: price,
            low: price,
            close: price,
        },
        Quantity::new(dec!(10)).unwrap(),
    )
    .unwrap()
}

#[async_trait]
impl MarketDataFeed for OneBarFeed {
    async fn latest_quote(&self, symbol: &Symbol) -> Result<Quote, MarketDataError> {
        Err(MarketDataError::NoData(symbol.clone()))
    }

    async fn bars(
        &self,
        symbol: &Symbol,
        _timeframe: Timeframe,
        _limit: usize,
    ) -> Result<Vec<Bar>, MarketDataError> {
        Err(MarketDataError::NoData(symbol.clone()))
    }

    async fn bars_between(
        &self,
        symbol: &Symbol,
        _timeframe: Timeframe,
        _start: DateTime<Utc>,
        _end: DateTime<Utc>,
    ) -> Result<Vec<Bar>, MarketDataError> {
        Err(MarketDataError::NoData(symbol.clone()))
    }

    async fn subscribe(
        &self,
        _symbols: &[Symbol],
    ) -> Result<BoxStream<'static, MarketEvent>, MarketDataError> {
        Ok(stream::iter([MarketEvent::Bar(sample_bar())])
            .chain(stream::pending())
            .boxed())
    }
}

#[tokio::test(start_paused = true)]
async fn live_bars_reach_the_screen_and_the_history() {
    let feed = Arc::new(OneBarFeed);
    let bars = Arc::new(RecordedBars::default());
    let (sender, mut receiver) = mpsc::unbounded_channel();
    let task = tokio::spawn(run_market_stream(
        Arc::new(WatchMarketUseCase::new(feed.clone(), Arc::new(FixedClock))),
        recorder_with(feed, bars.clone()),
        vec![Symbol::new("AAPL").unwrap()],
        sender,
    ));

    // La vela llega a la pantalla…
    loop {
        if let Action::Market(MarketEvent::Bar(bar)) = receiver.recv().await.unwrap() {
            assert_eq!(bar, sample_bar());
            break;
        }
    }
    // …y se guarda en segundo plano
    for _ in 0..100 {
        if !bars.0.lock().unwrap().is_empty() {
            break;
        }
        tokio::task::yield_now().await;
    }
    task.abort();
    assert_eq!(*bars.0.lock().unwrap(), vec![sample_bar()]);
}
