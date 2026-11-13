//! `tickforge data backfill`: descarga el histórico de velas de 1 minuto a `market.db`.
//!
//! Usa siempre las llaves de **paper** (`.env`): los datos de mercado son los mismos en ambos
//! modos y así nunca se necesita la cuenta real para descargar historia. Es incremental:
//! repetirlo solo baja lo que falte desde la última vela guardada (que se vuelve a pedir, por
//! si se guardó incompleta).

use std::sync::Arc;

use anyhow::{Context, bail};
use chrono::TimeDelta;
use tickforge_adapter_alpaca::{AlpacaConfig, AlpacaMarketData};
use tickforge_adapter_sqlite::{SqliteBarRepository, SqliteStore, SqliteWatchlistRepository};
use tickforge_application::ports::{Clock, WatchlistRepository};
use tickforge_application::use_cases::market::RecordMarketDataUseCase;
use tickforge_domain::market::Timeframe;
use tickforge_domain::shared::Symbol;

use crate::infrastructure::SystemClock;
use crate::settings::Settings;
use crate::{bootstrap, credentials, telemetry};

pub async fn run(days: u32, raw_symbols: Vec<String>) -> anyhow::Result<()> {
    let settings = Settings::load(false)?;
    let _log_guard = telemetry::init(&settings.log_dir)?;
    let symbols = symbols_to_download(&settings, &raw_symbols).await?;

    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let mut alpaca = AlpacaConfig::paper(credentials::paper_from_env()?);
    alpaca.data_feed = settings.data_feed;
    let market_data = Arc::new(
        AlpacaMarketData::new(&alpaca, Arc::clone(&clock))
            .context("no se pudo configurar los datos de mercado de Alpaca")?,
    );
    let market_store = bootstrap::open_market_store(&settings).await?;
    let recorder = RecordMarketDataUseCase::new(
        market_data,
        Arc::new(SqliteBarRepository::new(market_store.clone())),
        clock,
    );

    println!(
        "Descargando velas de 1 minuto de los últimos {days} días ({} símbolos)…",
        symbols.len()
    );
    let report = recorder
        .backfill(
            &symbols,
            Timeframe::Minute1,
            TimeDelta::days(i64::from(days)),
        )
        .await;
    market_store.close().await;

    for symbol in &report.symbols {
        match &symbol.result {
            Ok(saved) => println!("  {:<8} {saved:>8} velas", symbol.symbol.as_str()),
            Err(reason) => println!("  {:<8} ERROR: {reason}", symbol.symbol.as_str()),
        }
    }
    println!(
        "Total: {} velas guardadas en {}",
        report.saved(),
        settings.market_database_path.display()
    );
    tracing::info!(
        saved = report.saved(),
        failed = report.failed(),
        "histórico descargado"
    );

    if report.failed() > 0 {
        bail!("{} símbolos no se pudieron descargar", report.failed());
    }
    Ok(())
}

/// Los símbolos indicados o, si no hay, la watchlist guardada de paper (la de la TUI). Si
/// nunca se guardó una, la de `TICKFORGE_WATCHLIST`.
async fn symbols_to_download(
    settings: &Settings,
    raw_symbols: &[String],
) -> anyhow::Result<Vec<Symbol>> {
    if !raw_symbols.is_empty() {
        let mut symbols: Vec<Symbol> = Vec::new();
        for raw in raw_symbols {
            let symbol =
                Symbol::new(raw).with_context(|| format!("'{raw}' no es un símbolo válido"))?;
            if !symbols.contains(&symbol) {
                symbols.push(symbol);
            }
        }
        return Ok(symbols);
    }

    let store = SqliteStore::open(&settings.database_path)
        .await
        .with_context(|| {
            format!(
                "no se pudo abrir la base de datos '{}'",
                settings.database_path.display()
            )
        })?;
    let saved = SqliteWatchlistRepository::new(store.clone())
        .load()
        .await
        .context("no se pudo leer la watchlist guardada")?;
    store.close().await;
    Ok(saved.unwrap_or_else(|| settings.watchlist.clone()))
}
