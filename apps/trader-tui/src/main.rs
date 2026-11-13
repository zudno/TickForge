//! Punto de entrada de TickForge.
//!
//! Este binario es el composition root: aquí, y solo aquí, se eligen las
//! implementaciones concretas de cada puerto y se cablean con la TUI.

mod backfill;
mod bootstrap;
mod cli;
mod credentials;
mod infrastructure;
mod live_confirmation;
mod settings;
mod telemetry;

use anyhow::Context;
use tickforge_adapter_alpaca::AlpacaConfig;
use tickforge_presentation_tui::TradingMode;

use crate::cli::CliCommand;
use crate::settings::Settings;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let live_flag = match cli::parse(std::env::args().skip(1))? {
        CliCommand::Run { live } => live,
        CliCommand::SetLiveCredentials => return credentials::set_live_interactive(),
        CliCommand::DeleteLiveCredentials => return credentials::delete_live(),
        CliCommand::Backfill { days, symbols } => return backfill::run(days, symbols).await,
        CliCommand::Help => {
            println!("{}", cli::USAGE);
            return Ok(());
        }
    };

    // La configuración se valida antes de tomar la terminal, para que los errores se lean
    // como texto normal en la consola
    let settings = Settings::load(live_flag)?;
    let _log_guard = telemetry::init(&settings.log_dir)?;
    tracing::info!(mode = ?settings.mode, "iniciando TickForge");

    let alpaca = match settings.mode {
        TradingMode::Paper => AlpacaConfig::paper(credentials::paper_from_env()?),
        TradingMode::Live => {
            // Las llaves se leen antes de la frase: si faltan, no tiene sentido pedirla
            let credentials = credentials::load_live()?;
            let authorization = live_confirmation::ask(&settings)?;
            tracing::warn!("modo real confirmado por la persona usuaria");
            AlpacaConfig::live(credentials, authorization)
        }
    };

    let application = bootstrap::build(&settings, alpaca).await?;
    let result = tickforge_presentation_tui::run(application.services, application.config)
        .await
        .context("la interfaz de terminal terminó con un error");

    // Cerrar las bases aunque la TUI haya fallado, para que el diario quede completo en disco
    application.store.close().await;
    application.market_store.close().await;
    tracing::info!("TickForge cerrado");
    result
}
