use thiserror::Error;

/// Error al construir los clientes de Alpaca (antes de hacer cualquier petición).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AlpacaSetupError {
    #[error("las llaves de API contienen caracteres no válidos para un encabezado HTTP")]
    InvalidCredentials,

    #[error("no se pudo crear el cliente HTTP: {0}")]
    HttpClient(String),

    #[error("no se escribió la frase de confirmación exacta: el modo real no se habilita")]
    LiveNotConfirmed,

    #[error(
        "la configuración apunta a la cuenta real de Alpaca sin la autorización explícita \
         del modo real"
    )]
    LiveNotAuthorized,

    #[error(
        "el entorno ({environment}) no coincide con las URLs configuradas: se rechaza por \
         seguridad"
    )]
    EnvironmentMismatch { environment: &'static str },

    #[error("las llaves de API parecen de {found} pero el entorno es {environment}")]
    KeysDoNotMatchEnvironment {
        environment: &'static str,
        found: &'static str,
    },
}
