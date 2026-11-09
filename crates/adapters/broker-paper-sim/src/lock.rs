use std::sync::{Mutex, MutexGuard, PoisonError};

/// Toma el candado aunque esté "envenenado" (otro hilo entró en pánico con él tomado).
///
/// En un simulador en memoria es preferible seguir con el último estado que propagar el
/// pánico, y así se evita usar `unwrap` sobre el resultado de `lock`.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
