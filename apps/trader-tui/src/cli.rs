//! Argumentos de la línea de comandos. Son pocos y fijos, así que se leen a mano.

use anyhow::bail;

/// Ayuda que se muestra con `--help` o ante un argumento desconocido.
pub const USAGE: &str = "\
Uso:
  tickforge                         Abre la TUI en modo paper (por defecto)
  tickforge --live                  Abre la TUI con la cuenta REAL (requiere TICKFORGE_MODE=live)
  tickforge credentials set-live    Guarda las llaves de la cuenta real en el almacén de
                                    credenciales del sistema
  tickforge credentials delete-live Borra las llaves de la cuenta real
  tickforge data backfill [--days N] [SÍMBOLO ...]
                                    Descarga el histórico de velas de 1 minuto (por defecto,
                                    30 días de la watchlist guardada) en data/market.db
  tickforge --help                  Muestra esta ayuda";

/// Días de histórico que descarga `data backfill` si no se indica `--days`.
pub const DEFAULT_BACKFILL_DAYS: u32 = 30;
/// Máximo de días de `--days` (unos 10 años).
const MAX_BACKFILL_DAYS: u32 = 3_650;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliCommand {
    /// Abrir la TUI. `live` indica si se pasó `--live`.
    Run {
        live: bool,
    },
    SetLiveCredentials,
    DeleteLiveCredentials,
    /// Descargar histórico de velas. `symbols` vacío: la watchlist guardada.
    Backfill {
        days: u32,
        symbols: Vec<String>,
    },
    Help,
}

/// Interpreta los argumentos (sin el nombre del programa).
pub fn parse<I>(args: I) -> anyhow::Result<CliCommand>
where
    I: IntoIterator<Item = String>,
{
    let args: Vec<String> = args.into_iter().collect();
    let as_str: Vec<&str> = args.iter().map(String::as_str).collect();
    let command = match as_str.as_slice() {
        [] => CliCommand::Run { live: false },
        ["--live"] => CliCommand::Run { live: true },
        ["credentials", "set-live"] => CliCommand::SetLiveCredentials,
        ["credentials", "delete-live"] => CliCommand::DeleteLiveCredentials,
        ["--help" | "-h"] => CliCommand::Help,
        ["data", "backfill", rest @ ..] => parse_backfill(rest)?,
        _ => bail!("argumentos no reconocidos: '{}'\n\n{USAGE}", args.join(" ")),
    };
    Ok(command)
}

/// `[--days N] [SÍMBOLO ...]`, en cualquier orden.
fn parse_backfill(args: &[&str]) -> anyhow::Result<CliCommand> {
    let mut days = DEFAULT_BACKFILL_DAYS;
    let mut symbols = Vec::new();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match *arg {
            "--days" => {
                let Some(raw) = rest.next() else {
                    bail!("falta el número de días después de --days");
                };
                days = raw
                    .parse()
                    .ok()
                    .filter(|days| (1..=MAX_BACKFILL_DAYS).contains(days))
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "--days debe ser un número entre 1 y {MAX_BACKFILL_DAYS} (se recibió '{raw}')"
                        )
                    })?;
            }
            flag if flag.starts_with('-') => {
                bail!("opción no reconocida para data backfill: '{flag}'\n\n{USAGE}")
            }
            symbol => symbols.push(symbol.to_owned()),
        }
    }
    Ok(CliCommand::Backfill { days, symbols })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_args(args: &[&str]) -> anyhow::Result<CliCommand> {
        parse(args.iter().map(|arg| (*arg).to_owned()))
    }

    #[test]
    fn recognizes_every_command() {
        assert_eq!(parse_args(&[]).unwrap(), CliCommand::Run { live: false });
        assert_eq!(
            parse_args(&["--live"]).unwrap(),
            CliCommand::Run { live: true }
        );
        assert_eq!(
            parse_args(&["credentials", "set-live"]).unwrap(),
            CliCommand::SetLiveCredentials
        );
        assert_eq!(
            parse_args(&["credentials", "delete-live"]).unwrap(),
            CliCommand::DeleteLiveCredentials
        );
        assert_eq!(parse_args(&["-h"]).unwrap(), CliCommand::Help);
    }

    #[test]
    fn backfill_takes_optional_days_and_symbols() {
        assert_eq!(
            parse_args(&["data", "backfill"]).unwrap(),
            CliCommand::Backfill {
                days: DEFAULT_BACKFILL_DAYS,
                symbols: Vec::new()
            }
        );
        assert_eq!(
            parse_args(&["data", "backfill", "aapl", "--days", "90", "MSFT"]).unwrap(),
            CliCommand::Backfill {
                days: 90,
                symbols: vec!["aapl".to_owned(), "MSFT".to_owned()]
            }
        );
        for args in [
            &["data", "backfill", "--days"][..],
            &["data", "backfill", "--days", "0"],
            &["data", "backfill", "--days", "99999"],
            &["data", "backfill", "--days", "diez"],
            &["data", "backfill", "--force"],
            &["data"],
        ] {
            assert!(parse_args(args).is_err(), "{args:?} debería rechazarse");
        }
    }

    #[test]
    fn anything_else_is_rejected_instead_of_guessed() {
        for args in [
            &["--LIVE"][..],
            &["live"],
            &["--live", "--live"],
            &["--live=true"],
            &["credentials"],
        ] {
            assert!(parse_args(args).is_err(), "{args:?} debería rechazarse");
        }
    }
}
