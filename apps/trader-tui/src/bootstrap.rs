//! Cableado de dependencias: el único lugar que conoce las implementaciones concretas.

use std::sync::Arc;

use anyhow::Context;
use tickforge_adapter_alpaca::{
    AlpacaBrokerGateway, AlpacaConfig, AlpacaCredentials, AlpacaMarketData,
};
use tickforge_adapter_sqlite::{SqliteOrderJournal, SqliteStore, SqliteWatchlistRepository};
use tickforge_application::ports::{BrokerGateway, Clock, MarketDataFeed};
use tickforge_application::use_cases::market::{
    GetMarketClockUseCase, ManageWatchlistUseCase, WatchMarketUseCase,
};
use tickforge_application::use_cases::portfolio::GetPortfolioSnapshotUseCase;
use tickforge_application::use_cases::trading::{
    CancelOrderUseCase, PlaceOrderUseCase, ReconcileOrdersUseCase, TradingPorts,
    WatchOrderUpdatesUseCase,
};
use tickforge_domain::risk::RiskPolicy;
use tickforge_presentation_tui::{TuiConfig, TuiServices};

use crate::infrastructure::{SystemClock, TimestampClientOrderIdGenerator, TracingEventPublisher};
use crate::settings::Settings;

/// Todo lo que el binario necesita para correr la TUI y cerrarse ordenadamente.
pub struct Application {
    pub services: TuiServices,
    pub config: TuiConfig,
    /// Se cierra al salir para dejar la base de datos consistente en disco.
    pub store: SqliteStore,
}

pub async fn build(settings: &Settings) -> anyhow::Result<Application> {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);

    // Solo se construye la configuración de paper trading (ver `Settings::load`)
    let mut alpaca = AlpacaConfig::paper(AlpacaCredentials::new(
        settings.alpaca_key_id.clone(),
        settings.alpaca_secret_key.clone(),
    ));
    alpaca.data_feed = settings.data_feed;

    let broker: Arc<dyn BrokerGateway> = Arc::new(
        AlpacaBrokerGateway::new(&alpaca).context("no se pudo configurar el cliente de Alpaca")?,
    );
    let market_data: Arc<dyn MarketDataFeed> = Arc::new(
        AlpacaMarketData::new(&alpaca, Arc::clone(&clock))
            .context("no se pudo configurar los datos de mercado de Alpaca")?,
    );

    let store = SqliteStore::open(&settings.database_path)
        .await
        .with_context(|| {
            format!(
                "no se pudo abrir la base de datos '{}'",
                settings.database_path.display()
            )
        })?;
    tracing::info!(path = %settings.database_path.display(), "base de datos abierta");

    // La watchlist guardada manda; `TICKFORGE_WATCHLIST` solo se usa en el primer arranque
    let watchlist = Arc::new(ManageWatchlistUseCase::new(
        Arc::new(SqliteWatchlistRepository::new(store.clone())),
        Arc::clone(&market_data),
    ));
    let symbols = watchlist
        .load_or_seed(settings.watchlist.clone())
        .await
        .context("no se pudo cargar la watchlist guardada")?;

    let trading_ports = TradingPorts {
        broker: Arc::clone(&broker),
        market_data: Arc::clone(&market_data),
        journal: Arc::new(SqliteOrderJournal::new(store.clone())),
        events: Arc::new(TracingEventPublisher),
        clock: Arc::clone(&clock),
        client_order_ids: Arc::new(TimestampClientOrderIdGenerator::new(Arc::clone(&clock))),
    };

    let services = TuiServices {
        portfolio: Arc::new(GetPortfolioSnapshotUseCase::new(Arc::clone(&broker))),
        watch_market: Arc::new(WatchMarketUseCase::new(market_data, Arc::clone(&clock))),
        market_clock: Arc::new(GetMarketClockUseCase::new(Arc::clone(&broker))),
        watchlist,
        place_order: Arc::new(PlaceOrderUseCase::new(
            trading_ports.clone(),
            RiskPolicy::new(settings.risk_limits),
        )),
        cancel_order: Arc::new(CancelOrderUseCase::new(trading_ports.clone())),
        order_updates: Arc::new(WatchOrderUpdatesUseCase::new(broker)),
        reconcile_orders: Arc::new(ReconcileOrdersUseCase::new(trading_ports)),
        clock,
    };
    let config = TuiConfig {
        mode: settings.mode,
        broker_name: "Alpaca".to_owned(),
        watchlist: symbols,
    };
    Ok(Application {
        services,
        config,
        store,
    })
}
