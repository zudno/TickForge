//! Modelos exactos de la API de Alpaca (JSON). No contienen lógica ni salen de este crate.
//!
//! La Trading API envía los números como texto (`"100.25"`), mientras que la Market Data API
//! los envía como números JSON; por eso unos campos son `String` y otros `serde_json::Number`.
//! Los campos que no se usan se ignoran al deserializar.

mod market_data;
mod trading;

pub(crate) use market_data::{BarWire, BarsResponseWire, LatestQuoteResponseWire};
pub(crate) use trading::{
    AccountWire, ClockWire, ErrorBodyWire, NewOrderBodyWire, OrderWire, PositionWire,
};
