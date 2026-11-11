//! Punto de entrada de TickForge.
//!
//! Este binario es el composition root: aquí, y solo aquí, se eligen las
//! implementaciones concretas de cada puerto y se cablean con la TUI.

mod bootstrap;
mod infrastructure;
mod settings;
mod telemetry;

use anyhow::Context;

use crate::settings::Settings;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // La configuración se valida antes de tomar la terminal, para que los errores se lean
    // como texto normal en la consola
    let settings = Settings::load()?;
    let _log_guard = telemetry::init(&settings.log_dir)?;
    tracing::info!(mode = ?settings.mode, "iniciando TickForge");

    let application = bootstrap::build(&settings).await?;
    let result = tickforge_presentation_tui::run(application.services, application.config)
        .await
        .context("la interfaz de terminal terminó con un error");

    // Cerrar la base aunque la TUI haya fallado, para que el diario quede completo en disco
    application.store.close().await;
    tracing::info!("TickForge cerrado");
    result
}
