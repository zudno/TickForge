use std::path::Path;

use anyhow::Context;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;

/// Filtro por defecto si no se define `RUST_LOG`.
const DEFAULT_FILTER: &str = "info";

/// Configura los logs en un archivo diario (`<dir>/tickforge.log.AAAA-MM-DD`).
///
/// Nunca se escribe en la terminal, porque la ocupa la TUI. El `WorkerGuard` devuelto debe
/// vivir hasta el final del programa para que se vacíen los logs pendientes.
pub fn init(log_dir: &Path) -> anyhow::Result<WorkerGuard> {
    std::fs::create_dir_all(log_dir).with_context(|| {
        format!(
            "no se pudo crear la carpeta de logs '{}'",
            log_dir.display()
        )
    })?;

    let appender = tracing_appender::rolling::daily(log_dir, "tickforge.log");
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));

    tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_env_filter(filter)
        .try_init()
        .map_err(|error| anyhow::anyhow!("no se pudo iniciar el registro de logs: {error}"))?;

    Ok(guard)
}
