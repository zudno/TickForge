//! Configuración desde variables de entorno (y el archivo `.env`, si existe).

use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{Context, bail};
use chrono::TimeDelta;
use rust_decimal::Decimal;
use tickforge_adapter_alpaca::DataFeed;
use tickforge_domain::risk::RiskLimits;
use tickforge_domain::shared::{Money, Symbol};
use tickforge_presentation_tui::TradingMode;

/// Watchlist por defecto si no se define `TICKFORGE_WATCHLIST`.
const DEFAULT_WATCHLIST: &str = "AAPL,MSFT,NVDA,AMZN,SPY,QQQ";

/// Límites de riesgo por defecto: conservadores a propósito. Se ajustan en `.env`.
const DEFAULT_MAX_ORDER_NOTIONAL: &str = "5000";
const DEFAULT_MAX_POSITION_NOTIONAL: &str = "20000";
const DEFAULT_MAX_PRICE_DEVIATION_PCT: &str = "5";
const DEFAULT_MAX_QUOTE_AGE_SECS: &str = "15";

pub struct Settings {
    pub mode: TradingMode,
    pub watchlist: Vec<Symbol>,
    pub data_feed: DataFeed,
    pub log_dir: PathBuf,
    /// Archivo SQLite con el diario de órdenes y la watchlist.
    pub database_path: PathBuf,
    pub risk_limits: RiskLimits,
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
            database_path: optional("TICKFORGE_DATA_DIR")
                .map_or_else(|| PathBuf::from("data"), PathBuf::from)
                .join("tickforge.db"),
            log_dir: optional("TICKFORGE_LOG_DIR")
                .map_or_else(|| PathBuf::from("logs"), PathBuf::from),
            risk_limits: risk_limits()?,
            alpaca_key_id: required("ALPACA_API_KEY_ID")?,
            alpaca_secret_key: required("ALPACA_API_SECRET_KEY")?,
        })
    }
}

/// Límites de la política de riesgo, validados por el dominio.
fn risk_limits() -> anyhow::Result<RiskLimits> {
    let max_quote_age_secs: i64 =
        setting_or("TICKFORGE_MAX_QUOTE_AGE_SECS", DEFAULT_MAX_QUOTE_AGE_SECS)
            .parse()
            .context("TICKFORGE_MAX_QUOTE_AGE_SECS debe ser un número entero de segundos")?;

    RiskLimits::new(
        Money::usd(decimal_setting(
            "TICKFORGE_MAX_ORDER_NOTIONAL",
            DEFAULT_MAX_ORDER_NOTIONAL,
        )?),
        Money::usd(decimal_setting(
            "TICKFORGE_MAX_POSITION_NOTIONAL",
            DEFAULT_MAX_POSITION_NOTIONAL,
        )?),
        decimal_setting(
            "TICKFORGE_MAX_PRICE_DEVIATION_PCT",
            DEFAULT_MAX_PRICE_DEVIATION_PCT,
        )?,
        TimeDelta::try_seconds(max_quote_age_secs)
            .context("TICKFORGE_MAX_QUOTE_AGE_SECS está fuera de rango")?,
    )
    .context("los límites de riesgo de .env no son válidos")
}

fn setting_or(name: &str, default: &str) -> String {
    optional(name).unwrap_or_else(|| default.to_owned())
}

fn decimal_setting(name: &str, default: &str) -> anyhow::Result<Decimal> {
    let raw = setting_or(name, default);
    Decimal::from_str(&raw).with_context(|| format!("{name}='{raw}' no es un número válido"))
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
