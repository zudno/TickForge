use thiserror::Error;

/// Error al abrir o migrar la base de datos (antes de usarla).
#[derive(Debug, Error)]
pub enum StoreError {
    #[error("no se pudo crear la carpeta de datos '{path}': {reason}")]
    DataDirectory { path: String, reason: String },

    #[error("no se pudo abrir la base de datos: {0}")]
    Open(String),

    #[error("no se pudo actualizar el esquema de la base de datos: {0}")]
    Migration(String),
}
