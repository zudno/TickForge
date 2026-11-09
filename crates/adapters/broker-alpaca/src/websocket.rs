//! Utilidades comunes de los WebSocket de Alpaca: conexión, envío de JSON y lectura del
//! saludo inicial con tiempo de espera.

use std::time::Duration;

use futures::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

pub(crate) type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Tiempo máximo para conectar y para cada respuesta del saludo (autenticación, suscripción).
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// Error del WebSocket; cada adaptador lo traduce a su error de puerto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StreamError {
    /// Fallo de red, TLS o del protocolo WebSocket.
    Transport(String),
    /// El servidor no respondió a tiempo durante el saludo.
    Timeout,
    /// El servidor cerró la conexión durante el saludo.
    Closed,
    /// Credenciales rechazadas.
    Unauthorized,
    /// Límite de conexiones simultáneas alcanzado (plan gratuito: una sola).
    ConnectionLimit(String),
    /// El plan contratado no incluye el recurso pedido (por ejemplo, el feed `sip`).
    NotEntitled(String),
    /// Mensaje inesperado o error reportado por el servidor.
    Protocol(String),
}

pub(crate) async fn connect(url: &str) -> Result<Socket, StreamError> {
    let (socket, _response) = tokio::time::timeout(HANDSHAKE_TIMEOUT, connect_async(url))
        .await
        .map_err(|_| StreamError::Timeout)?
        .map_err(|error| StreamError::Transport(error.to_string()))?;
    Ok(socket)
}

/// Envía un mensaje JSON como texto. Puede contener credenciales: nunca se registra.
pub(crate) async fn send_json<T: Serialize>(
    socket: &mut Socket,
    message: &T,
) -> Result<(), StreamError> {
    let text =
        serde_json::to_string(message).map_err(|error| StreamError::Protocol(error.to_string()))?;
    socket
        .send(Message::text(text))
        .await
        .map_err(|error| StreamError::Transport(error.to_string()))
}

/// Lee el siguiente mensaje de datos (texto o binario) durante el saludo, ignorando los
/// mensajes de control. Alpaca envía las respuestas del stream de trading como binario.
pub(crate) async fn next_payload(socket: &mut Socket) -> Result<Vec<u8>, StreamError> {
    loop {
        let message = tokio::time::timeout(HANDSHAKE_TIMEOUT, socket.next())
            .await
            .map_err(|_| StreamError::Timeout)?
            .ok_or(StreamError::Closed)?
            .map_err(|error| StreamError::Transport(error.to_string()))?;

        if let Some(payload) = payload(message) {
            return Ok(payload);
        }
    }
}

/// Contenido de un mensaje de datos; `None` para ping, pong, cierre y fragmentos.
pub(crate) fn payload(message: Message) -> Option<Vec<u8>> {
    match message {
        Message::Text(_) | Message::Binary(_) => Some(message.into_data().to_vec()),
        Message::Ping(_) | Message::Pong(_) | Message::Close(_) | Message::Frame(_) => None,
    }
}
