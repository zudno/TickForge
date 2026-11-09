use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Client, RequestBuilder, Response, StatusCode};
use secrecy::ExposeSecret;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::config::AlpacaCredentials;
use crate::error::AlpacaSetupError;
use crate::wire::ErrorBodyWire;

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
}

/// Cliente HTTP autenticado contra una de las APIs de Alpaca.
#[derive(Debug, Clone)]
pub(crate) struct AlpacaHttp {
    client: Client,
    base_url: String,
}

impl AlpacaHttp {
    pub(crate) fn new(
        base_url: &str,
        credentials: &AlpacaCredentials,
        timeout: Duration,
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
        })
    }

    pub(crate) async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<T, HttpError> {
        let response = self
            .execute(self.client.get(self.url(path)).query(query))
            .await?;
        decode(response).await
    }

    pub(crate) async fn post<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, HttpError> {
        let response = self
            .execute(self.client.post(self.url(path)).json(body))
            .await?;
        decode(response).await
    }

    pub(crate) async fn delete(&self, path: &str) -> Result<(), HttpError> {
        self.execute(self.client.delete(self.url(path))).await?;
        Ok(())
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    async fn execute(&self, request: RequestBuilder) -> Result<Response, HttpError> {
        let response = request
            .send()
            .await
            .map_err(|error| HttpError::Transport(error.to_string()))?;

        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }

        let body = response.text().await.unwrap_or_default();
        let message = serde_json::from_str::<ErrorBodyWire>(&body)
            .ok()
            .and_then(|parsed| parsed.message)
            .unwrap_or_else(|| truncate(&body, 200));
        Err(HttpError::Status {
            status: status.as_u16(),
            message,
        })
    }
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
