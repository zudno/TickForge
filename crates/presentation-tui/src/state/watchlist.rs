use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use tickforge_application::ports::MarketEvent;
use tickforge_application::use_cases::market::SymbolSnapshot;
use tickforge_domain::shared::{Price, Symbol};

/// Dirección del último cambio de precio, para colorear la fila.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceDirection {
    Up,
    Down,
    Unchanged,
}

/// Una fila de la watchlist: lo último que se sabe de un símbolo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchlistRow {
    pub symbol: Symbol,
    pub bid: Option<Price>,
    pub ask: Option<Price>,
    pub last: Option<Price>,
    pub previous_close: Option<Price>,
    pub direction: PriceDirection,
    pub updated_at: Option<DateTime<Utc>>,
}

impl WatchlistRow {
    fn empty(symbol: Symbol) -> Self {
        Self {
            symbol,
            bid: None,
            ask: None,
            last: None,
            previous_close: None,
            direction: PriceDirection::Unchanged,
            updated_at: None,
        }
    }

    /// Cambio porcentual del día respecto al cierre anterior.
    pub fn change_pct(&self) -> Option<Decimal> {
        let last = self.last?.value();
        let previous = self.previous_close?.value();
        last.checked_sub(previous)?
            .checked_div(previous)?
            .checked_mul(Decimal::ONE_HUNDRED)
    }

    /// Diferencial `ask - bid`.
    pub fn spread(&self) -> Option<Decimal> {
        self.ask?.value().checked_sub(self.bid?.value())
    }

    /// Mejor estimación del precio actual: último negociado o, si no hay, el punto medio.
    pub fn reference_price(&self) -> Option<Price> {
        if self.last.is_some() {
            return self.last;
        }
        let mid = self
            .bid?
            .value()
            .checked_add(self.ask?.value())?
            .checked_div(Decimal::TWO)?;
        Price::new(mid.round_dp(Price::MAX_SCALE)).ok()
    }

    fn set_last(&mut self, price: Price, at: DateTime<Utc>) {
        self.direction = match self.last {
            Some(previous) if price > previous => PriceDirection::Up,
            Some(previous) if price < previous => PriceDirection::Down,
            _ => self.direction,
        };
        self.last = Some(price);
        self.updated_at = Some(at);
    }
}

/// Lista de símbolos seguidos y fila seleccionada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Watchlist {
    rows: Vec<WatchlistRow>,
    selected: usize,
}

impl Watchlist {
    pub fn new(symbols: Vec<Symbol>) -> Self {
        Self {
            rows: symbols.into_iter().map(WatchlistRow::empty).collect(),
            selected: 0,
        }
    }

    pub fn rows(&self) -> &[WatchlistRow] {
        &self.rows
    }

    pub fn symbols(&self) -> Vec<Symbol> {
        self.rows.iter().map(|row| row.symbol.clone()).collect()
    }

    pub fn row(&self, symbol: &Symbol) -> Option<&WatchlistRow> {
        self.rows.iter().find(|row| &row.symbol == symbol)
    }

    pub fn selected_index(&self) -> Option<usize> {
        (!self.rows.is_empty()).then_some(self.selected)
    }

    pub fn selected_row(&self) -> Option<&WatchlistRow> {
        self.rows.get(self.selected)
    }

    /// Selección circular hacia abajo.
    pub fn select_next(&mut self) {
        if !self.rows.is_empty() {
            self.selected = (self.selected + 1) % self.rows.len();
        }
    }

    /// Selección circular hacia arriba.
    pub fn select_previous(&mut self) {
        if !self.rows.is_empty() {
            self.selected = self.selected.checked_sub(1).unwrap_or(self.rows.len() - 1);
        }
    }

    /// Carga la foto inicial; los campos que vengan vacíos no borran datos ya recibidos.
    pub fn apply_snapshot(&mut self, snapshot: SymbolSnapshot) {
        let Some(row) = self.row_mut(&snapshot.symbol) else {
            return;
        };
        if let Some(quote) = snapshot.quote {
            row.bid = quote.bid().or(row.bid);
            row.ask = quote.ask().or(row.ask);
            row.updated_at = Some(quote.quoted_at());
        }
        row.last = row.last.or(snapshot.last_price);
        row.previous_close = snapshot.previous_close.or(row.previous_close);
    }

    /// Aplica un evento en vivo. Devuelve `false` si el símbolo no está en la lista.
    pub fn apply_event(&mut self, event: &MarketEvent) -> bool {
        let Some(row) = self.row_mut(event.symbol()) else {
            return false;
        };
        match event {
            MarketEvent::Quote(quote) => {
                row.bid = quote.bid();
                row.ask = quote.ask();
                row.updated_at = Some(quote.quoted_at());
            }
            MarketEvent::Trade(trade) => row.set_last(trade.price(), trade.executed_at()),
            // La vela de un minuto solo actualiza el último precio si no hay operaciones
            MarketEvent::Bar(bar) if row.last.is_none() => {
                row.set_last(bar.close(), bar.starts_at())
            }
            MarketEvent::Bar(_) => {}
        }
        true
    }

    fn row_mut(&mut self, symbol: &Symbol) -> Option<&mut WatchlistRow> {
        self.rows.iter_mut().find(|row| &row.symbol == symbol)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use tickforge_domain::market::TradeTick;
    use tickforge_domain::shared::Quantity;

    fn symbol(raw: &str) -> Symbol {
        Symbol::new(raw).unwrap()
    }

    fn price(value: Decimal) -> Price {
        Price::new(value).unwrap()
    }

    fn trade(raw: &str, value: Decimal) -> MarketEvent {
        MarketEvent::Trade(TradeTick::new(
            symbol(raw),
            price(value),
            Quantity::new(dec!(10)).unwrap(),
            DateTime::UNIX_EPOCH,
        ))
    }

    #[test]
    fn selection_wraps_around() {
        let mut watchlist = Watchlist::new(vec![symbol("AAPL"), symbol("MSFT")]);
        watchlist.select_previous();
        assert_eq!(watchlist.selected_index(), Some(1));
        watchlist.select_next();
        assert_eq!(watchlist.selected_index(), Some(0));
    }

    #[test]
    fn trades_update_last_price_direction_and_change() {
        let mut watchlist = Watchlist::new(vec![symbol("AAPL")]);
        watchlist.apply_snapshot(SymbolSnapshot {
            symbol: symbol("AAPL"),
            quote: None,
            last_price: Some(price(dec!(100))),
            previous_close: Some(price(dec!(98))),
        });

        watchlist.apply_event(&trade("AAPL", dec!(102.9)));
        let row = &watchlist.rows()[0];

        assert_eq!(row.direction, PriceDirection::Up);
        assert_eq!(row.change_pct().map(|pct| pct.round_dp(2)), Some(dec!(5)));
    }

    #[test]
    fn events_for_unknown_symbols_are_ignored() {
        let mut watchlist = Watchlist::new(vec![symbol("AAPL")]);
        assert!(!watchlist.apply_event(&trade("TSLA", dec!(1))));
    }

    #[test]
    fn reference_price_falls_back_to_mid() {
        let mut row = WatchlistRow::empty(symbol("AAPL"));
        row.bid = Some(price(dec!(10.00)));
        row.ask = Some(price(dec!(10.03)));
        assert_eq!(row.reference_price().map(Price::value), Some(dec!(10.015)));
    }
}
