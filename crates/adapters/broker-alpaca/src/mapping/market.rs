use rust_decimal::Decimal;
use serde_json::Number;
use tickforge_domain::market::{Bar, Ohlc, Quote, Timeframe};
use tickforge_domain::shared::{Price, Quantity, Symbol};

use crate::mapping::primitives::{MappingError, number, timestamp};
use crate::wire::{BarWire, LatestQuoteResponseWire};

pub(crate) fn quote(symbol: &Symbol, wire: LatestQuoteResponseWire) -> Result<Quote, MappingError> {
    let quote = wire.quote;
    Ok(Quote::new(
        symbol.clone(),
        optional_price("bp", &quote.bp)?,
        Quantity::new(number("bs", &quote.bs)?)?,
        optional_price("ap", &quote.ap)?,
        Quantity::new(number("as", &quote.ask_size)?)?,
        timestamp("t", &quote.t)?,
    ))
}

/// Alpaca usa `0` para indicar que un lado del libro no tiene ofertas.
fn optional_price(field: &str, value: &Number) -> Result<Option<Price>, MappingError> {
    let value = number(field, value)?;
    if value <= Decimal::ZERO {
        return Ok(None);
    }
    Ok(Some(Price::new(value)?))
}

pub(crate) fn bar(
    symbol: &Symbol,
    timeframe: Timeframe,
    wire: BarWire,
) -> Result<Bar, MappingError> {
    let price = |field: &str, value: &Number| -> Result<Price, MappingError> {
        Ok(Price::new(number(field, value)?)?)
    };

    Ok(Bar::new(
        symbol.clone(),
        timeframe,
        timestamp("t", &wire.t)?,
        Ohlc {
            open: price("o", &wire.o)?,
            high: price("h", &wire.h)?,
            low: price("l", &wire.l)?,
            close: price("c", &wire.c)?,
        },
        Quantity::new(number("v", &wire.v)?)?,
    )?)
}

/// Valor del parámetro `timeframe` de la API de velas.
pub(crate) fn timeframe_param(timeframe: Timeframe) -> &'static str {
    match timeframe {
        Timeframe::Minute1 => "1Min",
        Timeframe::Minute5 => "5Min",
        Timeframe::Minute15 => "15Min",
        Timeframe::Hour1 => "1Hour",
        Timeframe::Day1 => "1Day",
    }
}
