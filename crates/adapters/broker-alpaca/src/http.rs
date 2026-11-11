use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};
use reqwest::{Client, RequestBuilder, Response, StatusCode};
use secrecy::ExposeSecret;
use serde::Serialize;
use serde::de::DeserializeOwned;
use tickforge_application::resilience::ExponentialBackoff;

use crate::config::AlpacaCredentials;
use crate::error::AlpacaSetupError;
use crate::rate_limiter::RateLimiter;
use crate::wire::ErrorBodyWire;

/// Espera máxima que se respeta de un encabezado `Retry-After`.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(60);

/// Error HTTP crudo; cada adaptador lo traduce a su propio tipo de error de puerto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HttpError {
    /// Fallo de red, DNS, TLS o tiempo de espera agotado.
    Transport(String),
    /// Respuesta con código de error y el mensaje que envió Alpaca.
    Status { status: u16, message: String },
    /// La respuesta no tenía el formato esperado.
    Decode(String),
}

impl HttpError {
    pub(crate) fn is_status(&self, expected: StatusCode) -> bool {
        matches!(self, Self::Status { status, .. } if *status == expected.as_u16())
    }

    /// Errores pasajeros que tiene sentido reintentar: red, 429 y 5xx.
    fn is_transient(&self) -> bool {
        match self {
            Self::Transport(_) => true,
            Self::Status { status, .. } => {
                *status == StatusCode::TOO_MANY_REQUESTS.as_u16() || *status >= 500
            }
            Self::Decode(_) => false,
        }
    }
}

/// Fallo de un intento, con la espera que pidió el servidor (si la pidió).
struct Failure {
    error: HttpError,
    retry_after: Option<Duration>,
}

/// Si una petición puede repetirse sin efectos no deseados.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Retry {
    /// Lecturas y envíos idempotentes (una orden lleva `client_order_id`: repetirla no la duplica).
    Allowed,
    /// Operaciones donde repetir puede confundir (por ejemplo, cancelar dos veces).
    Never,
}

/// Política de reintentos y límite de peticiones de un cliente.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RequestPolicy {
    pub max_retries: u32,
    pub requests_per_minute: u32,
}

/// Cliente HTTP autenticado contra una de las APIs de Alpaca, con límite de peticiones y
/// reintentos automáticos para errores pasajeros.
#[derive(Debug, Clone)]
pub(crate) struct AlpacaHttp {
    client: Client,
    base_url: String,
    limiter: Arc<RateLimiter>,
    max_retries: u32,
    backoff: ExponentialBackoff,
}

impl AlpacaHttp {
    pub(crate) fn new(
        base_url: &str,
        credentials: &AlpacaCredentials,
        timeout: Duration,
        policy: RequestPolicy,
    ) -> Result<Self, AlpacaSetupError> {
        // Las llaves viajan como encabezados por defecto marcados como sensibles, para que
        // reqwest nunca los incluya en su salida de depuración
        let mut headers = HeaderMap::new();
        headers.insert(
            "apca-api-key-id",
            sensitive_header(credentials.key_id.expose_secret())?,
        );
        headers.insert(
            "apca-api-secret-key",
            sensitive_header(credentials.secret_key.expose_secret())?,
        );

        let client = Client::builder()
            .default_headers(headers)
            .timeout(timeout)
            .user_agent(concat!("tickforge/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| AlpacaSetupError::HttpClient(error.to_string()))?;

        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_owned(),
            limiter: Arc::new(RateLimiter::per_minute(policy.requests_per_minute)),
            max_retries: policy.max_retries,
            backoff: ExponentialBackoff::requests(),
        })
    }

    pub(crate) async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<T, HttpError> {
        let url = self.url(path);
        let response = self
            .send(Retry::Allowed, || self.client.get(&url).query(query))
            .await?;
        decode(response).await
    }

    pub(crate) async fn post<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
        retry: Retry,
    ) -> Result<T, HttpError> {
        let url = self.url(path);
        let response = self
            .send(retry, || self.client.post(&url).json(body))
            .await?;
        decode(response).await
    }

    /// Nunca se reintenta: si el primer intento sí llegó, el segundo fallaría con un error
    /// engañoso ("la orden ya no es cancelable").
    pub(crate) async fn delete(&self, path: &str) -> Result<(), HttpError> {
        let url = self.url(path);
        self.send(Retry::Never, || self.client.delete(&url)).await?;
        Ok(())
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    /// Envía la petición respetando el límite y reintentando los errores pasajeros con
    /// espera exponencial (o la que indique `Retry-After`).
    async fn send(
        &self,
        retry: Retry,
        build: impl Fn() -> RequestBuilder,
    ) -> Result<Response, HttpError> {
        let mut attempt: u32 = 0;
        loop {
            self.limiter.acquire().await;
            let failure = match execute_once(build()).await {
                Ok(response) => return Ok(response),
                Err(failure) => failure,
            };

            let can_retry = retry == Retry::Allowed
                && failure.error.is_transient()
                && attempt < self.max_retries;
            if !can_retry {
                return Err(failure.error);
            }
            attempt = attempt.saturating_add(1);
            let delay = failure
                .retry_after
                .unwrap_or_else(|| self.backoff.delay(attempt));
            tokio::time::sleep(delay).await;
        }
    }
}

async fn execute_once(request: RequestBuilder) -> Result<Response, Failure> {
    let response = request.send().await.map_err(|error| Failure {
        error: HttpError::Transport(error.to_string()),
        retry_after: None,
    })?;

    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }

    let retry_after = retry_after(&response);
    let body = response.text().await.unwrap_or_default();
    let message = serde_json::from_str::<ErrorBodyWire>(&body)
        .ok()
        .and_then(|parsed| parsed.message)
        .unwrap_or_else(|| truncate(&body, 200));
    Err(Failure {
        error: HttpError::Status {
            status: status.as_u16(),
            message,
        },
        retry_after,
    })
}

/// Segundos del encabezado `Retry-After` (la forma con fecha HTTP no se usa en Alpaca).
fn retry_after(response: &Response) -> Option<Duration> {
    response
        .headers()
        .get(RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(|seconds| Duration::from_secs(seconds).min(MAX_RETRY_AFTER))
}

async fn decode<T: DeserializeOwned>(response: Response) -> Result<T, HttpError> {
    let body = response
        .bytes()
        .await
        .map_err(|error| HttpError::Transport(error.to_string()))?;
    serde_json::from_slice(&body).map_err(|error| HttpError::Decode(error.to_string()))
}

fn sensitive_header(value: &str) -> Result<HeaderValue, AlpacaSetupError> {
    let mut header =
        HeaderValue::from_str(value).map_err(|_| AlpacaSetupError::InvalidCredentials)?;
    header.set_sensitive(true);
    Ok(header)
}

fn truncate(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}
