use thiserror::Error;

/// Error al construir los clientes de Alpaca (antes de hacer cualquier petición).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AlpacaSetupError {
    #[error("las llaves de API contienen caracteres no válidos para un encabezado HTTP")]
    InvalidCredentials,

    #[error("no se pudo crear el cliente HTTP: {0}")]
    HttpClient(String),
}
