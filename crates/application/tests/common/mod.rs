//! Arnés compartido: casos de uso cableados contra las implementaciones en memoria.

// Código exclusivo de pruebas: clippy no reconoce estas funciones auxiliares como pruebas
// (no llevan `#[test]`), así que se permite explícitamente `unwrap` aquí.
// `dead_code`: no todos los archivos de prueba usan todas las utilidades.
#![allow(dead_code, clippy::unwrap_used)]

use std::sync::Arc;

use chrono::{DateTime, TimeDelta, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tickforge_adapter_paper_sim::{
    InMemoryOrderJournal, ManualClock, PaperBroker, RecordingEventPublisher,
    SequentialClientOrderIdGenerator,
};
use tickforge_application::use_cases::portfolio::GetPortfolioSnapshotUseCase;
use tickforge_application::use_cases::trading::{
    CancelOrderUseCase, PlaceOrderCommand, PlaceOrderUseCase, TradingPorts,
};
use tickforge_domain::market::Quote;
use tickforge_domain::risk::{RiskLimits, RiskPolicy};
use tickforge_domain::shared::{Money, Price, Quantity, Symbol};
use tickforge_domain::trading::{OrderSide, OrderType, TimeInForce};

pub const START: i64 = 1_700_000_000;

pub struct Harness {
    pub clock: Arc<ManualClock>,
    pub broker: Arc<PaperBroker>,
    pub journal: Arc<InMemoryOrderJournal>,
    pub events: Arc<RecordingEventPublisher>,
    pub place_order: PlaceOrderUseCase,
    pub cancel_order: CancelOrderUseCase,
    pub portfolio: GetPortfolioSnapshotUseCase,
}

impl Harness {
    /// Cuenta de 10,000 USD; límites: 5,000 por orden, 8,000 por posición, 5 % fat-finger,
    /// cotizaciones de máximo 5 segundos.
    pub fn new() -> Self {
        let clock = Arc::new(ManualClock::new(at(START)));
        let broker = Arc::new(PaperBroker::new(clock.clone(), Money::usd(dec!(10000))));
        let journal = Arc::new(InMemoryOrderJournal::new());
        let events = Arc::new(RecordingEventPublisher::new());

        let ports = TradingPorts {
            broker: broker.clone(),
            market_data: broker.clone(),
            journal: journal.clone(),
            events: events.clone(),
            clock: clock.clone(),
            client_order_ids: Arc::new(SequentialClientOrderIdGenerator::new("tf-test")),
        };
        let limits = RiskLimits::new(
            Money::usd(dec!(5000)),
            Money::usd(dec!(8000)),
            dec!(5),
            TimeDelta::seconds(5),
        )
        .unwrap();

        Self {
            place_order: PlaceOrderUseCase::new(ports.clone(), RiskPolicy::new(limits)),
            cancel_order: CancelOrderUseCase::new(ports),
            portfolio: GetPortfolioSnapshotUseCase::new(broker.clone()),
            clock,
            broker,
            journal,
            events,
        }
    }

    /// Publica una cotización de AAPL con marca de tiempo "ahora".
    pub fn quote(&self, bid: Decimal, ask: Decimal) {
        self.broker
            .set_quote(Quote::new(
                aapl(),
                Some(Price::new(bid).unwrap()),
                Quantity::new(dec!(100)).unwrap(),
                Some(Price::new(ask).unwrap()),
                Quantity::new(dec!(100)).unwrap(),
                self.clock_now(),
            ))
            .unwrap();
    }

    fn clock_now(&self) -> DateTime<Utc> {
        use tickforge_application::ports::Clock;
        self.clock.now()
    }
}

pub fn at(seconds: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(seconds, 0).unwrap()
}

pub fn aapl() -> Symbol {
    Symbol::new("AAPL").unwrap()
}

pub fn command(side: OrderSide, order_type: OrderType, quantity: Decimal) -> PlaceOrderCommand {
    PlaceOrderCommand {
        symbol: aapl(),
        side,
        order_type,
        quantity: Quantity::new(quantity).unwrap(),
        time_in_force: TimeInForce::Day,
    }
}

pub fn limit(price: Decimal) -> OrderType {
    OrderType::Limit {
        limit_price: Price::new(price).unwrap(),
    }
}
