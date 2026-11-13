//! Configuración desde variables de entorno (y el archivo `.env`, si existe).

use std::path::{Path, PathBuf};
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

/// Valores por defecto de los límites de riesgo de un modo, y el prefijo de las variables
/// de entorno que los ajustan (`TICKFORGE_MAX_...` o `TICKFORGE_LIVE_MAX_...`).
struct LimitDefaults {
    prefix: &'static str,
    max_order_notional: &'static str,
    max_position_notional: &'static str,
    max_price_deviation_pct: &'static str,
    max_quote_age_secs: &'static str,
    /// Al alcanzarla, el kill switch se activa solo.
    max_daily_loss: &'static str,
    max_orders_per_minute: &'static str,
    max_orders_per_day: &'static str,
}

/// Paper: conservadores, pero con margen para practicar.
const PAPER_LIMITS: LimitDefaults = LimitDefaults {
    prefix: "TICKFORGE_",
    max_order_notional: "5000",
    max_position_notional: "20000",
    max_price_deviation_pct: "5",
    max_quote_age_secs: "15",
    max_daily_loss: "1000",
    max_orders_per_minute: "30",
    max_orders_per_day: "500",
};

/// Real: mucho más estrictos. Son independientes de los de paper: subir un límite de paper
/// nunca afecta al dinero real.
const LIVE_LIMITS: LimitDefaults = LimitDefaults {
    prefix: "TICKFORGE_LIVE_",
    max_order_notional: "250",
    max_position_notional: "1000",
    max_price_deviation_pct: "3",
    max_quote_age_secs: "10",
    max_daily_loss: "50",
    max_orders_per_minute: "5",
    max_orders_per_day: "30",
};

/// Nombre del archivo de la base de datos dentro de `TICKFORGE_DATA_DIR`.
const DATABASE_FILE: &str = "tickforge.db";
/// Subcarpeta de la base del modo real: diario, watchlist y kill switch propios.
const LIVE_DATA_SUBDIR: &str = "live";
/// Histórico de mercado (velas) para ML. Uno solo para ambos modos: el mercado es el mismo.
const MARKET_DATABASE_FILE: &str = "market.db";

pub struct Settings {
    pub mode: TradingMode,
    pub watchlist: Vec<Symbol>,
    pub data_feed: DataFeed,
    pub log_dir: PathBuf,
    /// Archivo SQLite con el diario de órdenes, la watchlist y el kill switch. Cada modo
    /// tiene el suyo.
    pub database_path: PathBuf,
    /// Archivo SQLite con el histórico de velas (`market.db`), compartido por ambos modos.
    pub market_database_path: PathBuf,
    pub risk_limits: RiskLimits,
}

impl Settings {
    /// `live_flag` indica si se pasó `--live` en la línea de comandos.
    pub fn load(live_flag: bool) -> anyhow::Result<Self> {
        // Busca `.env` en el directorio actual y sus padres; si no existe, se usan las
        // variables de entorno del sistema
        dotenvy::dotenv().ok();

        let mode = resolve_mode(optional("TICKFORGE_MODE").as_deref(), live_flag)?;

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

        let data_dir =
            optional("TICKFORGE_DATA_DIR").map_or_else(|| PathBuf::from("data"), PathBuf::from);
        let risk_limits = match mode {
            TradingMode::Paper => risk_limits(&PAPER_LIMITS)?,
            // En real, además, las órdenes a mercado solo con el mercado abierto (no es
            // configurable: fuera de horario la ejecución puede alejarse mucho de la cotización)
            TradingMode::Live => risk_limits(&LIVE_LIMITS)?.restrict_market_orders_to_open_market(),
        };

        Ok(Self {
            mode,
            watchlist,
            data_feed,
            database_path: database_path(&data_dir, mode),
            market_database_path: data_dir.join(MARKET_DATABASE_FILE),
            log_dir: optional("TICKFORGE_LOG_DIR")
                .map_or_else(|| PathBuf::from("logs"), PathBuf::from),
            risk_limits,
        })
    }
}

/// Las dos primeras llaves del modo real: `TICKFORGE_MODE=live` **y** `--live`. Si solo
/// está una de las dos, no se arranca en ningún modo: la discrepancia indica un error.
fn resolve_mode(env_mode: Option<&str>, live_flag: bool) -> anyhow::Result<TradingMode> {
    match (env_mode, live_flag) {
        (None | Some("paper"), false) => Ok(TradingMode::Paper),
        (Some("live"), true) => Ok(TradingMode::Live),
        (Some("live"), false) => bail!(
            "TICKFORGE_MODE=live, pero falta el parámetro --live. Para operar con dinero \
             real ejecuta `tickforge --live`; para paper, usa TICKFORGE_MODE=paper"
        ),
        (None | Some("paper"), true) => bail!(
            "se pasó --live, pero TICKFORGE_MODE no es 'live'. El modo real exige ambas \
             cosas: TICKFORGE_MODE=live en .env y el parámetro --live"
        ),
        (Some(other), _) => {
            bail!("TICKFORGE_MODE='{other}' no es válido; usa 'paper' o 'live'")
        }
    }
}

/// Paper conserva la ruta histórica (`data/tickforge.db`); el modo real usa su propia
/// subcarpeta para que nada se cruce entre cuentas.
fn database_path(data_dir: &Path, mode: TradingMode) -> PathBuf {
    match mode {
        TradingMode::Paper => data_dir.join(DATABASE_FILE),
        TradingMode::Live => data_dir.join(LIVE_DATA_SUBDIR).join(DATABASE_FILE),
    }
}

/// Límites de la política de riesgo del modo, validados por el dominio.
fn risk_limits(defaults: &LimitDefaults) -> anyhow::Result<RiskLimits> {
    let name = |suffix: &str| format!("{}{suffix}", defaults.prefix);

    let quote_age_name = name("MAX_QUOTE_AGE_SECS");
    let max_quote_age_secs: i64 = setting_or(&quote_age_name, defaults.max_quote_age_secs)
        .parse()
        .with_context(|| format!("{quote_age_name} debe ser un número entero de segundos"))?;
    let max_daily_loss = Money::usd(decimal_setting(
        &name("MAX_DAILY_LOSS"),
        defaults.max_daily_loss,
    )?);
    let max_orders_per_minute = count_setting(
        &name("MAX_ORDERS_PER_MINUTE"),
        defaults.max_orders_per_minute,
    )?;
    let max_orders_per_day =
        count_setting(&name("MAX_ORDERS_PER_DAY"), defaults.max_orders_per_day)?;

    RiskLimits::new(
        Money::usd(decimal_setting(
            &name("MAX_ORDER_NOTIONAL"),
            defaults.max_order_notional,
        )?),
        Money::usd(decimal_setting(
            &name("MAX_POSITION_NOTIONAL"),
            defaults.max_position_notional,
        )?),
        decimal_setting(
            &name("MAX_PRICE_DEVIATION_PCT"),
            defaults.max_price_deviation_pct,
        )?,
        TimeDelta::try_seconds(max_quote_age_secs)
            .with_context(|| format!("{quote_age_name} está fuera de rango"))?,
    )
    .and_then(|limits| limits.with_max_daily_loss(max_daily_loss))
    .and_then(|limits| limits.with_order_rate_limits(max_orders_per_minute, max_orders_per_day))
    .with_context(|| {
        format!(
            "los límites de riesgo ({}MAX_*) no son válidos",
            defaults.prefix
        )
    })
}

fn setting_or(name: &str, default: &str) -> String {
    optional(name).unwrap_or_else(|| default.to_owned())
}

fn count_setting(name: &str, default: &str) -> anyhow::Result<u32> {
    let raw = setting_or(name, default);
    raw.parse()
        .with_context(|| format!("{name}='{raw}' debe ser un número entero positivo"))
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

pub fn required(name: &str) -> anyhow::Result<String> {
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
    fn live_mode_needs_both_the_environment_and_the_flag() {
        assert_eq!(resolve_mode(None, false).unwrap(), TradingMode::Paper);
        assert_eq!(
            resolve_mode(Some("paper"), false).unwrap(),
            TradingMode::Paper
        );
        assert_eq!(resolve_mode(Some("live"), true).unwrap(), TradingMode::Live);

        // Una sola llave no basta, y tampoco cae silenciosamente a paper
        assert!(resolve_mode(Some("live"), false).is_err());
        assert!(resolve_mode(None, true).is_err());
        assert!(resolve_mode(Some("paper"), true).is_err());
        assert!(resolve_mode(Some("LIVE"), true).is_err());
        assert!(resolve_mode(Some("real"), false).is_err());
    }

    #[test]
    fn each_mode_has_its_own_database() {
        let data = Path::new("data");
        assert_eq!(
            database_path(data, TradingMode::Paper),
            Path::new("data").join("tickforge.db")
        );
        assert_eq!(
            database_path(data, TradingMode::Live),
            Path::new("data").join("live").join("tickforge.db")
        );
    }

    #[test]
    fn live_defaults_are_stricter_than_paper_defaults() {
        // Sin variables TICKFORGE_LIVE_* definidas en el entorno de pruebas
        let live = risk_limits(&LIVE_LIMITS).unwrap();
        assert_eq!(live.max_order_notional(), Money::usd(Decimal::from(250)));
        assert_eq!(
            live.max_position_notional(),
            Money::usd(Decimal::from(1000))
        );

        let paper_order = Decimal::from_str(PAPER_LIMITS.max_order_notional).unwrap();
        let paper_position = Decimal::from_str(PAPER_LIMITS.max_position_notional).unwrap();
        assert!(live.max_order_notional().amount() < paper_order);
        assert!(live.max_position_notional().amount() < paper_position);

        // Pérdida diaria y topes de órdenes: siempre configurados, más estrictos en real
        let paper = risk_limits(&PAPER_LIMITS).unwrap();
        assert_eq!(live.max_daily_loss(), Some(Money::usd(Decimal::from(50))));
        assert_eq!(live.max_orders_per_minute(), Some(5));
        assert_eq!(live.max_orders_per_day(), Some(30));
        assert!(live.max_daily_loss().unwrap().amount() < paper.max_daily_loss().unwrap().amount());
        assert!(live.max_orders_per_minute() < paper.max_orders_per_minute());
        assert!(live.max_orders_per_day() < paper.max_orders_per_day());
    }

    #[test]
    fn invalid_or_empty_watchlists_are_rejected() {
        assert!(parse_watchlist("AAPL,$$$").is_err());
        assert!(parse_watchlist(" , ").is_err());
    }
}
