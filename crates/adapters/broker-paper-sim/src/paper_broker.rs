use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, TimeDelta, Utc};
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use futures::stream::BoxStream;
use tickforge_application::ports::{
    BrokerError, BrokerGateway, Clock, MarketDataError, MarketDataFeed, MarketEvent, OrderUpdate,
    OrderUpdateKind,
};
use tickforge_domain::DomainError;
use tickforge_domain::market::{Bar, MarketClock, Quote, Timeframe};
use tickforge_domain::portfolio::{Account, AccountBalances, Position};
use tickforge_domain::shared::{Currency, Money, Price, Quantity, Symbol};
use tickforge_domain::trading::{
    ClientOrderId, Fill, FillId, Order, OrderId, OrderRequest, OrderSide, OrderType,
};

use crate::lock::lock;

/// Broker simulado en memoria: cuenta de efectivo (sin margen), solo posiciones largas.
///
/// Reglas de ejecución:
/// - Las órdenes a mercado se llenan de inmediato al precio ejecutable (`ask`/`bid`).
/// - Las órdenes límite se llenan cuando la cotización las vuelve ejecutables, al precio de
///   mercado (que es igual o mejor que el límite).
/// - Cada llenado es total (no simula ejecuciones parciales).
///
/// También implementa `MarketDataFeed` con las cotizaciones y velas que se le cargan.
pub struct PaperBroker {
    clock: Arc<dyn Clock>,
    state: Mutex<PaperState>,
}

struct MarketSubscriber {
    symbols: Vec<Symbol>,
    sender: UnboundedSender<MarketEvent>,
}

struct PaperState {
    cash: Money,
    last_equity: Money,
    trading_blocked: bool,
    market_open: bool,
    positions: BTreeMap<Symbol, Position>,
    orders: BTreeMap<OrderId, Order>,
    order_ids_by_client_id: HashMap<ClientOrderId, OrderId>,
    quotes: HashMap<Symbol, Quote>,
    bars: HashMap<(Symbol, Timeframe), Vec<Bar>>,
    next_order_sequence: u64,
    next_fill_sequence: u64,
    pending_submit_failure: Option<BrokerError>,
    order_subscribers: Vec<UnboundedSender<OrderUpdate>>,
    market_subscribers: Vec<MarketSubscriber>,
}

impl PaperBroker {
    pub fn new(clock: Arc<dyn Clock>, starting_cash: Money) -> Self {
        Self {
            clock,
            state: Mutex::new(PaperState {
                cash: starting_cash,
                last_equity: starting_cash,
                trading_blocked: false,
                market_open: true,
                positions: BTreeMap::new(),
                orders: BTreeMap::new(),
                order_ids_by_client_id: HashMap::new(),
                quotes: HashMap::new(),
                bars: HashMap::new(),
                next_order_sequence: 0,
                next_fill_sequence: 0,
                pending_submit_failure: None,
                order_subscribers: Vec::new(),
                market_subscribers: Vec::new(),
            }),
        }
    }

    // --- Controles para pruebas y demos ---

    /// Publica una cotización nueva y ejecuta las órdenes límite que se vuelvan ejecutables.
    pub fn set_quote(&self, quote: Quote) -> Result<(), BrokerError> {
        let now = self.clock.now();
        let mut state = lock(&self.state);
        let symbol = quote.symbol().clone();

        state.quotes.insert(symbol.clone(), quote.clone());
        state.publish_market_event(&MarketEvent::Quote(quote));

        let open_order_ids: Vec<OrderId> = state
            .orders
            .values()
            .filter(|order| order.symbol() == &symbol && order.status().accepts_fills())
            .map(|order| order.id().clone())
            .collect();
        for order_id in open_order_ids {
            state.try_fill(&order_id, now)?;
        }
        Ok(())
    }

    /// Carga el histórico de velas que devolverá `bars`.
    pub fn set_bars(&self, symbol: Symbol, timeframe: Timeframe, bars: Vec<Bar>) {
        lock(&self.state).bars.insert((symbol, timeframe), bars);
    }

    pub fn set_trading_blocked(&self, blocked: bool) {
        lock(&self.state).trading_blocked = blocked;
    }

    pub fn set_market_open(&self, open: bool) {
        lock(&self.state).market_open = open;
    }

    /// Equity del cierre anterior: permite simular una ganancia o pérdida del día.
    pub fn set_last_equity(&self, last_equity: Money) {
        lock(&self.state).last_equity = last_equity;
    }

    /// Hace que el próximo `submit_order` falle con `error` (por ejemplo, para simular una caída).
    pub fn fail_next_submit(&self, error: BrokerError) {
        lock(&self.state).pending_submit_failure = Some(error);
    }

    /// Número total de órdenes que el broker ha recibido.
    pub fn order_count(&self) -> usize {
        lock(&self.state).orders.len()
    }

    pub fn cash(&self) -> Money {
        lock(&self.state).cash
    }
}

impl PaperState {
    fn next_order_id(&mut self) -> Result<OrderId, BrokerError> {
        self.next_order_sequence = self.next_order_sequence.saturating_add(1);
        OrderId::new(&format!("paper-{:06}", self.next_order_sequence)).map_err(internal)
    }

    fn next_fill_id(&mut self) -> Result<FillId, BrokerError> {
        self.next_fill_sequence = self.next_fill_sequence.saturating_add(1);
        FillId::new(&format!("paper-fill-{:06}", self.next_fill_sequence)).map_err(internal)
    }

    fn publish_order_update(&mut self, update: OrderUpdate) {
        // Se descartan los suscriptores que ya cerraron su receptor
        self.order_subscribers
            .retain(|sender| sender.unbounded_send(update.clone()).is_ok());
    }

    fn publish_market_event(&mut self, event: &MarketEvent) {
        self.market_subscribers.retain(|subscriber| {
            if !subscriber.symbols.contains(event.symbol()) {
                return !subscriber.sender.is_closed();
            }
            subscriber.sender.unbounded_send(event.clone()).is_ok()
        });
    }

    /// Precio al que se ejecutaría la orden con la cotización actual, si es ejecutable.
    fn executable_fill_price(&self, order: &Order) -> Option<Price> {
        let side = order.side();
        let market_price = self.quotes.get(order.symbol())?.executable_price(side)?;

        match order.request().order_type() {
            OrderType::Market => Some(market_price),
            OrderType::Limit { limit_price } => {
                let marketable = match side {
                    OrderSide::Buy => market_price <= limit_price,
                    OrderSide::Sell => market_price >= limit_price,
                };
                marketable.then_some(market_price)
            }
        }
    }

    /// Ejecuta por completo la orden si la cotización actual lo permite.
    fn try_fill(&mut self, order_id: &OrderId, now: DateTime<Utc>) -> Result<(), BrokerError> {
        let Some(order) = self.orders.get(order_id) else {
            return Ok(());
        };
        if !order.status().accepts_fills() {
            return Ok(());
        }
        let Some(price) = self.executable_fill_price(order) else {
            return Ok(());
        };
        let quantity = order.remaining_quantity();
        let side = order.side();
        let symbol = order.symbol().clone();

        let fill = Fill::new(self.next_fill_id()?, quantity, price, now).map_err(internal)?;
        if let Some(order) = self.orders.get_mut(order_id) {
            order.apply_fill(fill.clone()).map_err(internal)?;
        }
        self.settle(side, &symbol, quantity, price)?;

        self.publish_order_update(OrderUpdate {
            order_id: order_id.clone(),
            kind: OrderUpdateKind::Filled(fill),
            occurred_at: now,
        });
        Ok(())
    }

    /// Mueve efectivo y posiciones tras una ejecución.
    fn settle(
        &mut self,
        side: OrderSide,
        symbol: &Symbol,
        quantity: Quantity,
        price: Price,
    ) -> Result<(), BrokerError> {
        let notional = Money::notional(price, quantity, Currency::Usd).map_err(internal)?;

        match side {
            OrderSide::Buy => {
                self.cash = self.cash.checked_sub(notional).map_err(internal)?;
                match self.positions.get_mut(symbol) {
                    Some(position) => position.apply_buy(quantity, price).map_err(internal)?,
                    None => {
                        let position =
                            Position::open(symbol.clone(), quantity, price).map_err(internal)?;
                        self.positions.insert(symbol.clone(), position);
                    }
                }
            }
            OrderSide::Sell => {
                let position =
                    self.positions
                        .get_mut(symbol)
                        .ok_or_else(|| BrokerError::Rejected {
                            reason: format!("no hay posición en {symbol}"),
                        })?;
                position.apply_sell(quantity, price).map_err(internal)?;
                if position.is_flat() {
                    self.positions.remove(symbol);
                }
                self.cash = self.cash.checked_add(notional).map_err(internal)?;
            }
        }
        Ok(())
    }

    /// Valor de la cuenta: efectivo + posiciones valuadas al bid (o al costo si no hay cotización).
    fn equity(&self) -> Result<Money, BrokerError> {
        self.positions
            .values()
            .try_fold(self.cash, |total, position| {
                let value = match self.quotes.get(position.symbol()).and_then(Quote::bid) {
                    Some(bid) => position.market_value(bid).map_err(internal)?,
                    None => position.cost_basis(),
                };
                total.checked_add(value).map_err(internal)
            })
    }

    /// Validaciones que haría un broker real al recibir la orden.
    fn validate_submission(&self, request: &OrderRequest) -> Result<(), BrokerError> {
        if self.trading_blocked {
            return Err(rejected("la cuenta tiene la operación bloqueada"));
        }

        match request.side() {
            OrderSide::Sell => {
                let held = self
                    .positions
                    .get(request.symbol())
                    .map_or(Quantity::ZERO, Position::quantity);
                if request.quantity() > held {
                    return Err(rejected("posición insuficiente (no se permiten cortos)"));
                }
            }
            OrderSide::Buy => {
                let market_price = self
                    .quotes
                    .get(request.symbol())
                    .and_then(|quote| quote.executable_price(OrderSide::Buy));
                let Some(price) = request.order_type().limit_price().or(market_price) else {
                    return Err(rejected("no hay precio para valuar la orden"));
                };
                let cost =
                    Money::notional(price, request.quantity(), Currency::Usd).map_err(internal)?;
                if cost.exceeds(self.cash).map_err(internal)? {
                    return Err(rejected("poder de compra insuficiente"));
                }
            }
        }
        Ok(())
    }
}

#[async_trait]
impl BrokerGateway for PaperBroker {
    async fn account(&self) -> Result<Account, BrokerError> {
        let state = lock(&self.state);
        let buying_power = if state.cash.is_negative() {
            Money::zero(state.cash.currency())
        } else {
            state.cash
        };
        Account::new(
            AccountBalances {
                equity: state.equity()?,
                cash: state.cash,
                buying_power,
                last_equity: state.last_equity,
            },
            state.trading_blocked,
        )
        .map_err(internal)
    }

    async fn positions(&self) -> Result<Vec<Position>, BrokerError> {
        Ok(lock(&self.state).positions.values().cloned().collect())
    }

    async fn position(&self, symbol: &Symbol) -> Result<Option<Position>, BrokerError> {
        Ok(lock(&self.state).positions.get(symbol).cloned())
    }

    async fn open_orders(&self) -> Result<Vec<Order>, BrokerError> {
        Ok(lock(&self.state)
            .orders
            .values()
            .filter(|order| !order.is_terminal())
            .cloned()
            .collect())
    }

    async fn order(&self, order_id: &OrderId) -> Result<Option<Order>, BrokerError> {
        Ok(lock(&self.state).orders.get(order_id).cloned())
    }

    async fn order_by_client_order_id(
        &self,
        client_order_id: &ClientOrderId,
    ) -> Result<Option<Order>, BrokerError> {
        let state = lock(&self.state);
        Ok(state
            .order_ids_by_client_id
            .get(client_order_id)
            .and_then(|order_id| state.orders.get(order_id))
            .cloned())
    }

    async fn submit_order(&self, request: &OrderRequest) -> Result<Order, BrokerError> {
        let now = self.clock.now();
        let mut state = lock(&self.state);

        if let Some(error) = state.pending_submit_failure.take() {
            return Err(error);
        }

        // Idempotencia: el mismo ClientOrderId devuelve la orden existente
        if let Some(existing_id) = state.order_ids_by_client_id.get(request.client_order_id()) {
            return state
                .orders
                .get(existing_id)
                .cloned()
                .ok_or_else(|| BrokerError::OrderNotFound(existing_id.clone()));
        }

        state.validate_submission(request)?;

        let order_id = state.next_order_id()?;
        let mut order = Order::new(order_id.clone(), request.clone(), now);
        order.mark_accepted(now).map_err(internal)?;
        state.orders.insert(order_id.clone(), order);
        state
            .order_ids_by_client_id
            .insert(request.client_order_id().clone(), order_id.clone());
        state.publish_order_update(OrderUpdate {
            order_id: order_id.clone(),
            kind: OrderUpdateKind::Accepted,
            occurred_at: now,
        });

        state.try_fill(&order_id, now)?;

        state
            .orders
            .get(&order_id)
            .cloned()
            .ok_or(BrokerError::OrderNotFound(order_id))
    }

    async fn cancel_order(&self, order_id: &OrderId) -> Result<(), BrokerError> {
        let now = self.clock.now();
        let mut state = lock(&self.state);

        let order = state
            .orders
            .get_mut(order_id)
            .ok_or_else(|| BrokerError::OrderNotFound(order_id.clone()))?;
        order
            .request_cancel(now)
            .map_err(|error| BrokerError::Rejected {
                reason: error.to_string(),
            })?;
        // El simulador confirma la cancelación de inmediato
        order.mark_canceled(now).map_err(internal)?;

        state.publish_order_update(OrderUpdate {
            order_id: order_id.clone(),
            kind: OrderUpdateKind::Canceled,
            occurred_at: now,
        });
        Ok(())
    }

    async fn order_updates(&self) -> Result<BoxStream<'static, OrderUpdate>, BrokerError> {
        let (sender, receiver) = unbounded();
        lock(&self.state).order_subscribers.push(sender);
        Ok(receiver.boxed())
    }

    async fn market_clock(&self) -> Result<MarketClock, BrokerError> {
        let now = self.clock.now();
        let open = lock(&self.state).market_open;
        // Horario sintético: el próximo evento ocurre en una hora (suma saturada, sin pánico)
        let after = |delta: TimeDelta| {
            now.checked_add_signed(delta)
                .unwrap_or(DateTime::<Utc>::MAX_UTC)
        };
        let in_one_hour = after(TimeDelta::hours(1));
        let in_one_day = after(TimeDelta::days(1));
        let (next_open, next_close) = if open {
            (in_one_day, in_one_hour)
        } else {
            (in_one_hour, in_one_day)
        };
        MarketClock::new(now, open, next_open, next_close).map_err(internal)
    }
}

#[async_trait]
impl MarketDataFeed for PaperBroker {
    async fn latest_quote(&self, symbol: &Symbol) -> Result<Quote, MarketDataError> {
        lock(&self.state)
            .quotes
            .get(symbol)
            .cloned()
            .ok_or_else(|| MarketDataError::NoData(symbol.clone()))
    }

    async fn bars(
        &self,
        symbol: &Symbol,
        timeframe: Timeframe,
        limit: usize,
    ) -> Result<Vec<Bar>, MarketDataError> {
        let state = lock(&self.state);
        let bars = state
            .bars
            .get(&(symbol.clone(), timeframe))
            .ok_or_else(|| MarketDataError::NoData(symbol.clone()))?;
        let start = bars.len().saturating_sub(limit);
        Ok(bars.iter().skip(start).cloned().collect())
    }

    async fn bars_between(
        &self,
        symbol: &Symbol,
        timeframe: Timeframe,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Bar>, MarketDataError> {
        let state = lock(&self.state);
        Ok(state
            .bars
            .get(&(symbol.clone(), timeframe))
            .map(|bars| {
                bars.iter()
                    .filter(|bar| bar.starts_at() >= start && bar.starts_at() < end)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn subscribe(
        &self,
        symbols: &[Symbol],
    ) -> Result<BoxStream<'static, MarketEvent>, MarketDataError> {
        let (sender, receiver) = unbounded();
        lock(&self.state).market_subscribers.push(MarketSubscriber {
            symbols: symbols.to_vec(),
            sender,
        });
        Ok(receiver.boxed())
    }
}

fn rejected(reason: &str) -> BrokerError {
    BrokerError::Rejected {
        reason: reason.to_owned(),
    }
}

/// Un error de dominio dentro del simulador indica un fallo del propio simulador.
fn internal(error: DomainError) -> BrokerError {
    BrokerError::InvalidResponse(format!("simulador: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ManualClock;
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;
    use tickforge_domain::trading::{OrderStatus, TimeInForce};

    fn symbol() -> Symbol {
        Symbol::new("AAPL").unwrap()
    }

    fn broker() -> PaperBroker {
        let clock = Arc::new(ManualClock::new(DateTime::from_timestamp(0, 0).unwrap()));
        PaperBroker::new(clock, Money::usd(dec!(10000)))
    }

    fn quote(bid: Decimal, ask: Decimal) -> Quote {
        Quote::new(
            symbol(),
            Some(Price::new(bid).unwrap()),
            Quantity::new(dec!(100)).unwrap(),
            Some(Price::new(ask).unwrap()),
            Quantity::new(dec!(100)).unwrap(),
            DateTime::from_timestamp(0, 0).unwrap(),
        )
    }

    fn request(
        client_id: &str,
        side: OrderSide,
        order_type: OrderType,
        quantity: Decimal,
    ) -> OrderRequest {
        OrderRequest::new(
            ClientOrderId::new(client_id).unwrap(),
            symbol(),
            side,
            order_type,
            Quantity::new(quantity).unwrap(),
            TimeInForce::Day,
        )
        .unwrap()
    }

    fn limit(price: Decimal) -> OrderType {
        OrderType::Limit {
            limit_price: Price::new(price).unwrap(),
        }
    }

    #[tokio::test]
    async fn market_buy_fills_at_ask_and_moves_cash() {
        let broker = broker();
        broker.set_quote(quote(dec!(99.99), dec!(100))).unwrap();

        let order = broker
            .submit_order(&request("c1", OrderSide::Buy, OrderType::Market, dec!(10)))
            .await
            .unwrap();

        assert_eq!(order.status(), OrderStatus::Filled);
        assert_eq!(order.average_fill_price(), Some(dec!(100)));
        assert_eq!(broker.cash(), Money::usd(dec!(9000)));
        let position = broker.position(&symbol()).await.unwrap().unwrap();
        assert_eq!(position.quantity(), Quantity::new(dec!(10)).unwrap());
    }

    #[tokio::test]
    async fn limit_buy_rests_until_quote_makes_it_marketable() {
        let broker = broker();
        broker.set_quote(quote(dec!(99.99), dec!(100))).unwrap();

        let order = broker
            .submit_order(&request("c1", OrderSide::Buy, limit(dec!(98)), dec!(5)))
            .await
            .unwrap();
        assert_eq!(order.status(), OrderStatus::New);

        // El ask baja por debajo del límite: se ejecuta al ask (mejora de precio)
        broker.set_quote(quote(dec!(97.50), dec!(97.60))).unwrap();
        let order = broker.order(order.id()).await.unwrap().unwrap();
        assert_eq!(order.status(), OrderStatus::Filled);
        assert_eq!(order.average_fill_price(), Some(dec!(97.60)));
    }

    #[tokio::test]
    async fn resubmitting_same_client_id_is_idempotent() {
        let broker = broker();
        broker.set_quote(quote(dec!(99.99), dec!(100))).unwrap();
        let request = request("c1", OrderSide::Buy, limit(dec!(90)), dec!(1));

        let first = broker.submit_order(&request).await.unwrap();
        let second = broker.submit_order(&request).await.unwrap();

        assert_eq!(first.id(), second.id());
        assert_eq!(broker.order_count(), 1);
    }

    #[tokio::test]
    async fn rejects_short_sales_and_unaffordable_buys() {
        let broker = broker();
        broker.set_quote(quote(dec!(99.99), dec!(100))).unwrap();

        let short = broker
            .submit_order(&request("c1", OrderSide::Sell, OrderType::Market, dec!(1)))
            .await;
        assert!(matches!(short, Err(BrokerError::Rejected { .. })));

        let too_big = broker
            .submit_order(&request(
                "c2",
                OrderSide::Buy,
                OrderType::Market,
                dec!(1000),
            ))
            .await;
        assert!(matches!(too_big, Err(BrokerError::Rejected { .. })));
    }

    #[tokio::test]
    async fn streams_order_updates_in_order() {
        let broker = broker();
        broker.set_quote(quote(dec!(99.99), dec!(100))).unwrap();
        let mut updates = broker.order_updates().await.unwrap();

        broker
            .submit_order(&request("c1", OrderSide::Buy, OrderType::Market, dec!(1)))
            .await
            .unwrap();

        assert_eq!(
            updates.next().await.map(|u| u.kind),
            Some(OrderUpdateKind::Accepted)
        );
        assert!(matches!(
            updates.next().await.map(|u| u.kind),
            Some(OrderUpdateKind::Filled(_))
        ));
    }

    #[tokio::test]
    async fn round_trip_restores_cash() {
        let broker = broker();
        broker.set_quote(quote(dec!(100), dec!(100))).unwrap();

        broker
            .submit_order(&request("c1", OrderSide::Buy, OrderType::Market, dec!(10)))
            .await
            .unwrap();
        broker
            .submit_order(&request("c2", OrderSide::Sell, OrderType::Market, dec!(10)))
            .await
            .unwrap();

        assert_eq!(broker.cash(), Money::usd(dec!(10000)));
        assert!(broker.positions().await.unwrap().is_empty());
    }
}
