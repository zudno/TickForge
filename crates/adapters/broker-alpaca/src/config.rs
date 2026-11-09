use std::fmt;
use std::time::Duration;

use secrecy::SecretString;

/// Entorno de Alpaca. Solo `Paper` tiene constructor de conveniencia: operar en `Live`
/// debe ser una decisión explícita del composition root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlpacaEnvironment {
    Paper,
    Live,
}

impl AlpacaEnvironment {
    pub fn trading_base_url(self) -> &'static str {
        match self {
            Self::Paper => "https://paper-api.alpaca.markets",
            Self::Live => "https://api.alpaca.markets",
        }
    }
}

/// Feed de datos de mercado. `Iex` es gratuito (una fracción del volumen);
/// `Sip` cubre todo el mercado y requiere suscripción de pago.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataFeed {
    Iex,
    Sip,
}

impl DataFeed {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Iex => "iex",
            Self::Sip => "sip",
        }
    }
}

/// Llaves de API. `SecretString` evita que aparezcan en `Debug` o en logs por accidente.
pub struct AlpacaCredentials {
    pub(crate) key_id: SecretString,
    pub(crate) secret_key: SecretString,
}

impl AlpacaCredentials {
    pub fn new(key_id: String, secret_key: String) -> Self {
        Self {
            key_id: SecretString::from(key_id),
            secret_key: SecretString::from(secret_key),
        }
    }
}

impl fmt::Debug for AlpacaCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AlpacaCredentials { .. }")
    }
}

#[derive(Debug)]
pub struct AlpacaConfig {
    pub environment: AlpacaEnvironment,
    pub credentials: AlpacaCredentials,
    pub data_feed: DataFeed,
    pub trading_base_url: String,
    pub data_base_url: String,
    pub request_timeout: Duration,
}

impl AlpacaConfig {
    pub const DATA_BASE_URL: &'static str = "https://data.alpaca.markets";

    /// Configuración de paper trading con el feed gratuito IEX.
    pub fn paper(credentials: AlpacaCredentials) -> Self {
        Self {
            environment: AlpacaEnvironment::Paper,
            credentials,
            data_feed: DataFeed::Iex,
            trading_base_url: AlpacaEnvironment::Paper.trading_base_url().to_owned(),
            data_base_url: Self::DATA_BASE_URL.to_owned(),
            request_timeout: Duration::from_secs(10),
        }
    }

    /// Redirige ambas APIs a otras URLs (servidor falso en pruebas).
    pub fn with_base_urls(mut self, trading: &str, data: &str) -> Self {
        self.trading_base_url = trading.trim_end_matches('/').to_owned();
        self.data_base_url = data.trim_end_matches('/').to_owned();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_prints_secrets() {
        let credentials = AlpacaCredentials::new("key-123".to_owned(), "secret-456".to_owned());
        let printed = format!("{:?}", AlpacaConfig::paper(credentials));

        assert!(!printed.contains("key-123"));
        assert!(!printed.contains("secret-456"));
    }

    #[test]
    fn paper_is_the_default_environment() {
        let config = AlpacaConfig::paper(AlpacaCredentials::new(String::new(), String::new()));
        assert_eq!(config.environment, AlpacaEnvironment::Paper);
        assert_eq!(config.trading_base_url, "https://paper-api.alpaca.markets");
    }
}
