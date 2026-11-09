//! Capa anticorrupción: traduce los modelos de Alpaca a tipos del dominio, validando todo.
//!
//! Cualquier dato que el dominio no acepte se reporta como `MappingError`, que los
//! adaptadores convierten en `InvalidResponse`: nunca se fabrica un valor "aproximado".
//! En los flujos en tiempo real, un dato inválido se descarta (no se puede devolver un error
//! dentro del flujo sin cortarlo).

mod market;
mod primitives;
mod streaming;
mod trading;

pub(crate) use market::{bar, quote, timeframe_param};
pub(crate) use primitives::MappingError;
pub(crate) use streaming::{DataFrameItem, data_frame, trade_update};
pub(crate) use trading::{account, clock, new_order_body, order, position};
