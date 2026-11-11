use std::fmt;
use std::time::Duration;

use secrecy::{ExposeSecret, SecretString};

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

    /// WebSocket de actualizaciones de órdenes (`trade_updates`).
    pub fn trading_stream_url(self) -> &'static str {
        match self {
            Self::Paper => "wss://paper-api.alpaca.markets/stream",
            Self::Live => "wss://api.alpaca.markets/stream",
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

    /// Copia independiente para los clientes que necesitan autenticarse más tarde
    /// (por ejemplo, al abrir un WebSocket).
    pub(crate) fn duplicate(&self) -> Self {
        Self::new(
            self.key_id.expose_secret().to_owned(),
            self.secret_key.expose_secret().to_owned(),
        )
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
    pub trading_stream_url: String,
    /// Base del WebSocket de datos; el feed se agrega al conectar (`/v2/iex`).
    pub data_stream_base_url: String,
    pub request_timeout: Duration,
    /// Reintentos automáticos ante errores pasajeros (red, 429, 5xx) en operaciones seguras.
    pub max_retries: u32,
    /// Peticiones por minuto que se permite cada API (Alpaca admite unas 200; se deja margen).
    pub requests_per_minute: u32,
}

impl AlpacaConfig {
    pub const DATA_BASE_URL: &'static str = "https://data.alpaca.markets";
    pub const DATA_STREAM_BASE_URL: &'static str = "wss://stream.data.alpaca.markets";

    /// Configuración de paper trading con el feed gratuito IEX.
    pub fn paper(credentials: AlpacaCredentials) -> Self {
        Self {
            environment: AlpacaEnvironment::Paper,
            credentials,
            data_feed: DataFeed::Iex,
            trading_base_url: AlpacaEnvironment::Paper.trading_base_url().to_owned(),
            data_base_url: Self::DATA_BASE_URL.to_owned(),
            trading_stream_url: AlpacaEnvironment::Paper.trading_stream_url().to_owned(),
            data_stream_base_url: Self::DATA_STREAM_BASE_URL.to_owned(),
            request_timeout: Duration::from_secs(10),
            max_retries: 3,
            requests_per_minute: 190,
        }
    }

    /// Redirige ambas APIs REST a otras URLs (servidor falso en pruebas).
    pub fn with_base_urls(mut self, trading: &str, data: &str) -> Self {
        self.trading_base_url = trading.trim_end_matches('/').to_owned();
        self.data_base_url = data.trim_end_matches('/').to_owned();
        self
    }

    /// Redirige ambos WebSocket a otras URLs (servidor falso en pruebas).
    pub fn with_stream_urls(mut self, trading_stream: &str, data_stream_base: &str) -> Self {
        self.trading_stream_url = trading_stream.to_owned();
        self.data_stream_base_url = data_stream_base.trim_end_matches('/').to_owned();
        self
    }

    /// URL completa del WebSocket de datos para el feed configurado.
    pub fn data_stream_url(&self) -> String {
        format!(
            "{}/v2/{}",
            self.data_stream_base_url,
            self.data_feed.as_str()
        )
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
