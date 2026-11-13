//! Tercera llave del modo real: escribir la frase de confirmación en la consola, antes de
//! que la TUI tome la terminal.

use std::io::{BufRead, Write};

use anyhow::Context;
use tickforge_adapter_alpaca::LiveTradingAuthorization;

use crate::settings::Settings;

/// Muestra a qué se está entrando y pide la frase. Cualquier otra respuesta cancela.
pub fn ask(settings: &Settings) -> anyhow::Result<LiveTradingAuthorization> {
    let limits = &settings.risk_limits;
    let mut out = std::io::stdout().lock();
    writeln!(out)?;
    writeln!(
        out,
        "  ╔════════════════════════════════════════════════════╗"
    )?;
    writeln!(
        out,
        "  ║   MODO REAL (LIVE): LAS ÓRDENES USAN DINERO REAL   ║"
    )?;
    writeln!(
        out,
        "  ╚════════════════════════════════════════════════════╝"
    )?;
    writeln!(out)?;
    writeln!(
        out,
        "  Límite por orden:    {} USD",
        limits.max_order_notional().amount()
    )?;
    writeln!(
        out,
        "  Límite por posición: {} USD",
        limits.max_position_notional().amount()
    )?;
    writeln!(
        out,
        "  Base de datos:       {}",
        settings.database_path.display()
    )?;
    writeln!(out)?;
    writeln!(
        out,
        "  La sesión empieza con el kill switch ACTIVO: revisa la cuenta y pulsa ! para"
    )?;
    writeln!(out, "  permitir órdenes. Las órdenes abiertas no se tocan.")?;
    writeln!(out)?;
    write!(
        out,
        "  Escribe {} para continuar (cualquier otra cosa cancela): ",
        LiveTradingAuthorization::CONFIRMATION_PHRASE
    )?;
    out.flush()?;
    drop(out);

    let mut typed = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut typed)
        .context("no se pudo leer la confirmación desde la consola")?;
    LiveTradingAuthorization::confirm(&typed).context("modo real cancelado: TickForge no se abrió")
}
