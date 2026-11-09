//! Configuración desde variables de entorno (y el archivo `.env`, si existe).

use std::path::PathBuf;

use anyhow::{Context, bail};
use tickforge_adapter_alpaca::DataFeed;
use tickforge_domain::shared::Symbol;
use tickforge_presentation_tui::TradingMode;

/// Watchlist por defecto si no se define `TICKFORGE_WATCHLIST`.
const DEFAULT_WATCHLIST: &str = "AAPL,MSFT,NVDA,AMZN,SPY,QQQ";

pub struct Settings {
    pub mode: TradingMode,
    pub watchlist: Vec<Symbol>,
    pub data_feed: DataFeed,
    pub log_dir: PathBuf,
    pub alpaca_key_id: String,
    pub alpaca_secret_key: String,
}

impl Settings {
    pub fn load() -> anyhow::Result<Self> {
        // Busca `.env` en el directorio actual y sus padres; si no existe, se usan las
        // variables de entorno del sistema
        dotenvy::dotenv().ok();

        let mode = match optional("TICKFORGE_MODE").as_deref() {
            None | Some("paper") => TradingMode::Paper,
            Some("live") => bail!(
                "El modo real (TICKFORGE_MODE=live) no está habilitado en esta versión. \
                 Usa TICKFORGE_MODE=paper."
            ),
            Some(other) => bail!("TICKFORGE_MODE='{other}' no es válido; usa 'paper'"),
        };

        let data_feed = match optional("ALPACA_DATA_FEED").as_deref() {
            None | Some("iex") => DataFeed::Iex,
            Some("sip") => DataFeed::Sip,
            Some(other) => bail!("ALPACA_DATA_FEED='{other}' no es válido; usa 'iex' o 'sip'"),
        };

        let watchlist = parse_watchlist(
            optional("TICKFORGE_WATCHLIST")
                .as_deref()
                .unwrap_or(DEFAULT_WATCHLIST),
        )?;

        Ok(Self {
            mode,
            watchlist,
            data_feed,
            log_dir: optional("TICKFORGE_LOG_DIR")
                .map_or_else(|| PathBuf::from("logs"), PathBuf::from),
            alpaca_key_id: required("ALPACA_API_KEY_ID")?,
            alpaca_secret_key: required("ALPACA_API_SECRET_KEY")?,
        })
    }
}

fn optional(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn required(name: &str) -> anyhow::Result<String> {
    optional(name).with_context(|| {
        format!("falta la variable {name}: cópiala en el archivo .env (ver .env.example)")
    })
}

/// Lista de símbolos separada por comas, sin duplicados y en el orden dado.
fn parse_watchlist(raw: &str) -> anyhow::Result<Vec<Symbol>> {
    let mut symbols: Vec<Symbol> = Vec::new();
    for item in raw
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        let symbol = Symbol::new(item).with_context(|| {
            format!("TICKFORGE_WATCHLIST contiene un símbolo inválido: '{item}'")
        })?;
        if !symbols.contains(&symbol) {
            symbols.push(symbol);
        }
    }
    if symbols.is_empty() {
        bail!("TICKFORGE_WATCHLIST no contiene ningún símbolo");
    }
    Ok(symbols)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watchlist_is_normalized_and_deduplicated() {
        let symbols = parse_watchlist(" aapl, MSFT ,,AAPL,brk.b ").unwrap();
        let names: Vec<&str> = symbols.iter().map(Symbol::as_str).collect();
        assert_eq!(names, ["AAPL", "MSFT", "BRK.B"]);
    }

    #[test]
    fn invalid_or_empty_watchlists_are_rejected() {
        assert!(parse_watchlist("AAPL,$$$").is_err());
        assert!(parse_watchlist(" , ").is_err());
    }
}
