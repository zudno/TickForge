//! Punto de entrada de TickForge.
//!
//! Este binario es el composition root: aquí, y solo aquí, se eligen las
//! implementaciones concretas de cada puerto y se cablean con la TUI.

mod bootstrap;
mod settings;
mod system_clock;
mod telemetry;

use anyhow::Context;

use crate::settings::Settings;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // La configuración se valida antes de tomar la terminal, para que los errores se lean
    // como texto normal en la consola
    let settings = Settings::load()?;
    let _log_guard = telemetry::init(&settings.log_dir)?;
    tracing::info!(
        mode = ?settings.mode,
        symbols = settings.watchlist.len(),
        "iniciando TickForge"
    );

    let (services, config) = bootstrap::build(&settings)?;
    tickforge_presentation_tui::run(services, config)
        .await
        .context("la interfaz de terminal terminó con un error")?;

    tracing::info!("TickForge cerrado");
    Ok(())
}
