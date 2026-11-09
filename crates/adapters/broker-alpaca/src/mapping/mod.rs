//! Capa anticorrupción: traduce los modelos de Alpaca a tipos del dominio, validando todo.
//!
//! Cualquier dato que el dominio no acepte se reporta como `MappingError`, que los
//! adaptadores convierten en `InvalidResponse`: nunca se fabrica un valor "aproximado".

mod market;
mod primitives;
mod trading;

pub(crate) use market::{bar, quote, timeframe_param};
pub(crate) use primitives::MappingError;
pub(crate) use trading::{account, clock, new_order_body, order, position};
