use std::fmt;
use std::time::Duration;

use secrecy::{ExposeSecret, SecretString};

use crate::error::AlpacaSetupError;
use crate::live_authorization::LiveTradingAuthorization;

/// Host de la Trading API de la cuenta real. Cualquier URL que apunte aquí exige la
/// autorización del modo real.
const LIVE_TRADING_HOST: &str = "api.alpaca.markets";
const PAPER_TRADING_HOST: &str = "paper-api.alpaca.markets";

/// Entorno de Alpaca. `Paper` se construye libremente; `Live` solo con una
/// `LiveTradingAuthorization`, que exige que una persona escriba la frase de confirmación.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlpacaEnvironment {
    Paper,
    Live,
}

impl AlpacaEnvironment {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Paper => "paper",
            Self::Live => "real (live)",
        }
    }

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
    /// Campo privado: impide construir la configuración con un literal fuera del crate y
    /// guarda la prueba de que el modo real se confirmó.
    live_authorization: Option<LiveTradingAuthorization>,
}

impl AlpacaConfig {
    pub const DATA_BASE_URL: &'static str = "https://data.alpaca.markets";
    pub const DATA_STREAM_BASE_URL: &'static str = "wss://stream.data.alpaca.markets";

    /// Configuración de paper trading con el feed gratuito IEX.
    pub fn paper(credentials: AlpacaCredentials) -> Self {
        Self::for_environment(AlpacaEnvironment::Paper, credentials, None)
    }

    /// Configuración de la cuenta **real**. Solo el composition root la construye, después
    /// de que la persona escribió la frase de confirmación.
    pub fn live(credentials: AlpacaCredentials, authorization: LiveTradingAuthorization) -> Self {
        Self::for_environment(AlpacaEnvironment::Live, credentials, Some(authorization))
    }

    fn for_environment(
        environment: AlpacaEnvironment,
        credentials: AlpacaCredentials,
        live_authorization: Option<LiveTradingAuthorization>,
    ) -> Self {
        Self {
            environment,
            credentials,
            data_feed: DataFeed::Iex,
            trading_base_url: environment.trading_base_url().to_owned(),
            data_base_url: Self::DATA_BASE_URL.to_owned(),
            trading_stream_url: environment.trading_stream_url().to_owned(),
            data_stream_base_url: Self::DATA_STREAM_BASE_URL.to_owned(),
            request_timeout: Duration::from_secs(10),
            max_retries: 3,
            requests_per_minute: 190,
            live_authorization,
        }
    }

    /// Verifica que la configuración no pueda llegar a la cuenta real por accidente. Los
    /// clientes la llaman al construirse, antes de cualquier petición:
    ///
    /// - Una URL de la cuenta real exige la autorización del modo real.
    /// - El entorno declarado debe coincidir con las URLs de trading.
    /// - Las llaves no pueden ser evidentemente del otro entorno (Alpaca emite las de paper
    ///   con prefijo `PK` y las reales con `AK`; es solo una pista, por eso se usa para
    ///   rechazar, nunca para aprobar).
    pub(crate) fn validate(&self) -> Result<(), AlpacaSetupError> {
        let urls = [&self.trading_base_url, &self.trading_stream_url];
        let targets_live = urls.iter().any(|url| host(url) == LIVE_TRADING_HOST);
        let targets_paper = urls.iter().any(|url| host(url) == PAPER_TRADING_HOST);
        let environment = self.environment.as_str();

        if (targets_live || self.environment == AlpacaEnvironment::Live)
            && self.live_authorization.is_none()
        {
            return Err(AlpacaSetupError::LiveNotAuthorized);
        }
        let mismatch = match self.environment {
            AlpacaEnvironment::Paper => targets_live,
            AlpacaEnvironment::Live => targets_paper,
        };
        if mismatch {
            return Err(AlpacaSetupError::EnvironmentMismatch { environment });
        }

        let key_id = self.credentials.key_id.expose_secret();
        let found = match self.environment {
            AlpacaEnvironment::Paper if key_id.starts_with("AK") => Some("la cuenta real"),
            AlpacaEnvironment::Live if key_id.starts_with("PK") => Some("paper"),
            _ => None,
        };
        match found {
            Some(found) => Err(AlpacaSetupError::KeysDoNotMatchEnvironment { environment, found }),
            None => Ok(()),
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

/// Host de una URL (`https://host:puerto/ruta` → `host`), en minúsculas.
fn host(url: &str) -> String {
    let without_scheme = url.split_once("://").map_or(url, |(_, rest)| rest);
    without_scheme
        .split(['/', ':', '?', '#'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credentials(key_id: &str) -> AlpacaCredentials {
        AlpacaCredentials::new(key_id.to_owned(), "secret".to_owned())
    }

    fn authorization() -> LiveTradingAuthorization {
        LiveTradingAuthorization::confirm(LiveTradingAuthorization::CONFIRMATION_PHRASE).unwrap()
    }

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

    #[test]
    fn paper_and_fake_servers_pass_validation() {
        assert_eq!(AlpacaConfig::paper(credentials("PK123")).validate(), Ok(()));
        let fake = AlpacaConfig::paper(credentials("key"))
            .with_base_urls("http://127.0.0.1:4000", "http://127.0.0.1:4001")
            .with_stream_urls("ws://127.0.0.1:4002/stream", "ws://127.0.0.1:4003");
        assert_eq!(fake.validate(), Ok(()));
    }

    #[test]
    fn paper_config_redirected_to_the_real_account_is_rejected() {
        let redirected = AlpacaConfig::paper(credentials("PK123"))
            .with_base_urls("https://api.alpaca.markets", AlpacaConfig::DATA_BASE_URL);
        assert_eq!(
            redirected.validate(),
            Err(AlpacaSetupError::LiveNotAuthorized)
        );

        let mut stream = AlpacaConfig::paper(credentials("PK123"));
        stream.trading_stream_url = "wss://API.alpaca.markets/stream".to_owned();
        assert_eq!(stream.validate(), Err(AlpacaSetupError::LiveNotAuthorized));
    }

    #[test]
    fn switching_the_environment_field_is_not_enough_to_go_live() {
        let mut config = AlpacaConfig::paper(credentials("AK123"));
        config.environment = AlpacaEnvironment::Live;
        assert_eq!(config.validate(), Err(AlpacaSetupError::LiveNotAuthorized));
    }

    #[test]
    fn live_config_requires_matching_urls_and_keys() {
        let live = AlpacaConfig::live(credentials("AK123"), authorization());
        assert_eq!(live.environment, AlpacaEnvironment::Live);
        assert_eq!(live.trading_base_url, "https://api.alpaca.markets");
        assert_eq!(live.validate(), Ok(()));

        let to_paper = AlpacaConfig::live(credentials("AK123"), authorization()).with_base_urls(
            "https://paper-api.alpaca.markets",
            AlpacaConfig::DATA_BASE_URL,
        );
        assert!(matches!(
            to_paper.validate(),
            Err(AlpacaSetupError::EnvironmentMismatch { .. })
        ));
    }

    #[test]
    fn keys_from_the_other_environment_are_rejected() {
        assert!(matches!(
            AlpacaConfig::live(credentials("PKPAPER"), authorization()).validate(),
            Err(AlpacaSetupError::KeysDoNotMatchEnvironment { found: "paper", .. })
        ));
        assert!(matches!(
            AlpacaConfig::paper(credentials("AKLIVE")).validate(),
            Err(AlpacaSetupError::KeysDoNotMatchEnvironment { .. })
        ));
    }

    #[test]
    fn host_is_extracted_from_any_url() {
        assert_eq!(host("https://api.alpaca.markets/v2"), "api.alpaca.markets");
        assert_eq!(
            host("wss://Paper-API.alpaca.markets/stream"),
            "paper-api.alpaca.markets"
        );
        assert_eq!(host("http://127.0.0.1:4000"), "127.0.0.1");
        assert_eq!(host("api.alpaca.markets"), "api.alpaca.markets");
    }
}
