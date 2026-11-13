/// Órdenes enviadas recientemente (intentos de envío registrados en el diario), en ventanas
/// móviles. La política las compara con el tope de órdenes para frenar ráfagas: un error en
/// bucle o un modelo desbocado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OrderActivity {
    /// Órdenes enviadas en los últimos 60 segundos.
    pub last_minute: u32,
    /// Órdenes enviadas en las últimas 24 horas.
    pub last_day: u32,
}
