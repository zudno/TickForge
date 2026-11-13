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
  tickforge --help                  Muestra esta ayuda";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliCommand {
    /// Abrir la TUI. `live` indica si se pasó `--live`.
    Run {
        live: bool,
    },
    SetLiveCredentials,
    DeleteLiveCredentials,
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
        _ => bail!("argumentos no reconocidos: '{}'\n\n{USAGE}", args.join(" ")),
    };
    Ok(command)
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
