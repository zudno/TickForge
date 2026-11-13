//! Cableado de dependencias: el único lugar que conoce las implementaciones concretas.

use std::sync::Arc;

use anyhow::Context;
use tickforge_adapter_alpaca::{AlpacaBrokerGateway, AlpacaConfig, AlpacaMarketData};
use tickforge_adapter_sqlite::{
    SqliteKillSwitchRepository, SqliteOrderJournal, SqliteStore, SqliteWatchlistRepository,
};
use tickforge_application::ports::{BrokerGateway, Clock, MarketDataFeed};
use tickforge_application::use_cases::market::{
    GetMarketClockUseCase, ManageWatchlistUseCase, WatchMarketUseCase,
};
use tickforge_application::use_cases::portfolio::GetPortfolioSnapshotUseCase;
use tickforge_application::use_cases::trading::{
    CancelOrderUseCase, DailyLossGuard, KillSwitchUseCase, PlaceOrderUseCase,
    ReconcileOrdersUseCase, TradingPorts, WatchOrderUpdatesUseCase,
};
use tickforge_domain::risk::RiskPolicy;
use tickforge_presentation_tui::{TradingMode, TuiConfig, TuiServices};

use crate::infrastructure::{SystemClock, TimestampClientOrderIdGenerator, TracingEventPublisher};
use crate::settings::Settings;

/// Motivo con el que se bloquean las órdenes al iniciar una sesión en modo real.
const LIVE_SESSION_HOLD_REASON: &str = "inicio de sesión en modo real";

/// Todo lo que el binario necesita para correr la TUI y cerrarse ordenadamente.
pub struct Application {
    pub services: TuiServices,
    pub config: TuiConfig,
    /// Se cierra al salir para dejar la base de datos consistente en disco.
    pub store: SqliteStore,
}

/// `alpaca` ya viene construida para el modo de `settings`: la de la cuenta real solo existe
/// si la persona escribió la frase de confirmación (ver `main`).
pub async fn build(settings: &Settings, mut alpaca: AlpacaConfig) -> anyhow::Result<Application> {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
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
        kill_switch: Arc::new(SqliteKillSwitchRepository::new(store.clone())),
    };

    // En modo real cada sesión empieza con las órdenes nuevas bloqueadas (sin cancelar las
    // abiertas): la persona revisa la cuenta y desbloquea a propósito con `!`
    let kill_switch = Arc::new(KillSwitchUseCase::new(trading_ports.clone()));
    if settings.mode == TradingMode::Live {
        let held = kill_switch
            .hold(LIVE_SESSION_HOLD_REASON)
            .await
            .context("no se pudo bloquear las órdenes al iniciar el modo real")?;
        tracing::info!(?held, "modo real: órdenes nuevas bloqueadas al iniciar");
    }

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
        reconcile_orders: Arc::new(ReconcileOrdersUseCase::new(trading_ports.clone())),
        kill_switch,
        // Misma regla de pérdida diaria que la política de riesgo
        daily_loss_guard: Arc::new(DailyLossGuard::new(trading_ports, settings.risk_limits)),
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
