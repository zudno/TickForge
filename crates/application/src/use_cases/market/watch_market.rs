use std::sync::Arc;

use chrono::{DateTime, TimeDelta, Utc};
use futures::future::join_all;
use futures::stream::BoxStream;
use tickforge_domain::market::{Bar, Quote, Timeframe};
use tickforge_domain::shared::{Price, Symbol};

use crate::error::ApplicationError;
use crate::ports::{Clock, MarketDataFeed, MarketEvent};

/// Estado inicial de un símbolo de la watchlist, antes de que lleguen datos en vivo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolSnapshot {
    pub symbol: Symbol,
    pub quote: Option<Quote>,
    /// Último precio negociado conocido (cierre de la vela diaria más reciente).
    pub last_price: Option<Price>,
    /// Cierre de la sesión anterior: base del cambio porcentual del día.
    pub previous_close: Option<Price>,
}

/// Caso de uso: seguir en tiempo real los símbolos de la watchlist.
///
/// Primero se pide una foto inicial (cotización y velas diarias) para no mostrar la pantalla
/// vacía, y después se abre el flujo en vivo.
pub struct WatchMarketUseCase {
    market_data: Arc<dyn MarketDataFeed>,
    clock: Arc<dyn Clock>,
}

impl WatchMarketUseCase {
    pub fn new(market_data: Arc<dyn MarketDataFeed>, clock: Arc<dyn Clock>) -> Self {
        Self { market_data, clock }
    }

    /// Foto inicial de cada símbolo, consultada en paralelo.
    ///
    /// Es tolerante a fallos por símbolo: si un dato no está disponible, ese campo queda
    /// vacío en lugar de impedir que se muestre el resto de la watchlist.
    pub async fn snapshot(&self, symbols: &[Symbol]) -> Vec<SymbolSnapshot> {
        let now = self.clock.now();
        join_all(symbols.iter().map(|symbol| self.snapshot_one(symbol, now))).await
    }

    /// Flujo en vivo de cotizaciones, operaciones y velas de todos los símbolos (una sola
    /// conexión, como exige el plan gratuito de Alpaca).
    pub async fn stream(
        &self,
        symbols: &[Symbol],
    ) -> Result<BoxStream<'static, MarketEvent>, ApplicationError> {
        Ok(self.market_data.subscribe(symbols).await?)
    }

    async fn snapshot_one(&self, symbol: &Symbol, now: DateTime<Utc>) -> SymbolSnapshot {
        let (quote, bars) = futures::join!(
            self.market_data.latest_quote(symbol),
            self.market_data.bars(symbol, Timeframe::Day1, 2),
        );
        let (last_price, previous_close) = bars
            .map(|bars| reference_prices(&bars, now))
            .unwrap_or((None, None));

        SymbolSnapshot {
            symbol: symbol.clone(),
            quote: quote.ok(),
            last_price,
            previous_close,
        }
    }
}

/// Último precio y cierre anterior a partir de las dos velas diarias más recientes.
///
/// Si la vela más reciente es de la sesión en curso (empezó hace menos de un día), el cierre
/// anterior es el de la vela previa. Si no (mercado aún sin abrir), el último precio y el
/// cierre anterior coinciden y el cambio del día es cero.
fn reference_prices(bars: &[Bar], now: DateTime<Utc>) -> (Option<Price>, Option<Price>) {
    let Some(latest) = bars.last() else {
        return (None, None);
    };
    let is_current_session = now.signed_duration_since(latest.starts_at()) < TimeDelta::days(1);

    let previous_close = if is_current_session {
        bars.len()
            .checked_sub(2)
            .and_then(|index| bars.get(index))
            .map(Bar::close)
    } else {
        Some(latest.close())
    };
    (Some(latest.close()), previous_close)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;
    use tickforge_domain::market::Ohlc;
    use tickforge_domain::shared::Quantity;

    fn daily_bar(day_start: i64, close: Decimal) -> Bar {
        let price = Price::new(close).unwrap();
        Bar::new(
            Symbol::new("AAPL").unwrap(),
            Timeframe::Day1,
            DateTime::from_timestamp(day_start, 0).unwrap(),
            Ohlc {
                open: price,
                high: price,
                low: price,
                close: price,
            },
            Quantity::new(dec!(1000)).unwrap(),
        )
        .unwrap()
    }

    const DAY: i64 = 86_400;

    #[test]
    fn during_the_session_previous_close_is_yesterday() {
        let bars = [daily_bar(0, dec!(100)), daily_bar(DAY, dec!(103))];
        let now = DateTime::from_timestamp(DAY + 3_600 * 15, 0).unwrap();

        let (last, previous) = reference_prices(&bars, now);
        assert_eq!(last.map(Price::value), Some(dec!(103)));
        assert_eq!(previous.map(Price::value), Some(dec!(100)));
    }

    #[test]
    fn before_the_open_there_is_no_change_yet() {
        let bars = [daily_bar(0, dec!(100)), daily_bar(DAY, dec!(103))];
        let now = DateTime::from_timestamp(3 * DAY, 0).unwrap();

        let (last, previous) = reference_prices(&bars, now);
        assert_eq!(last.map(Price::value), Some(dec!(103)));
        assert_eq!(previous.map(Price::value), Some(dec!(103)));
    }

    #[test]
    fn no_bars_means_no_reference_prices() {
        let now = DateTime::from_timestamp(0, 0).unwrap();
        assert_eq!(reference_prices(&[], now), (None, None));
    }
}
