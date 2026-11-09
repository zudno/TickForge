//! Cableado de dependencias: el único lugar que conoce las implementaciones concretas.

use std::sync::Arc;

use anyhow::Context;
use tickforge_adapter_alpaca::{
    AlpacaBrokerGateway, AlpacaConfig, AlpacaCredentials, AlpacaMarketData,
};
use tickforge_application::ports::{BrokerGateway, Clock, MarketDataFeed};
use tickforge_application::use_cases::market::{GetMarketClockUseCase, WatchMarketUseCase};
use tickforge_application::use_cases::portfolio::GetPortfolioSnapshotUseCase;
use tickforge_presentation_tui::{TuiConfig, TuiServices};

use crate::settings::Settings;
use crate::system_clock::SystemClock;

pub fn build(settings: &Settings) -> anyhow::Result<(TuiServices, TuiConfig)> {
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

    let services = TuiServices {
        portfolio: Arc::new(GetPortfolioSnapshotUseCase::new(Arc::clone(&broker))),
        watch_market: Arc::new(WatchMarketUseCase::new(market_data, Arc::clone(&clock))),
        market_clock: Arc::new(GetMarketClockUseCase::new(broker)),
        clock,
    };
    let config = TuiConfig {
        mode: settings.mode,
        broker_name: "Alpaca".to_owned(),
        watchlist: settings.watchlist.clone(),
    };
    Ok((services, config))
}
