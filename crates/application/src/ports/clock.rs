use chrono::{DateTime, Utc};

/// Fuente de la hora actual.
///
/// Inyectarla (en vez de leer el reloj del sistema) hace deterministas las pruebas y
/// permite reproducir sesiones grabadas en el futuro (backtesting).
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}
