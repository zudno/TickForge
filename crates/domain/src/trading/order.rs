use std::collections::HashSet;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;

use crate::error::DomainError;
use crate::shared::{Currency, Money, Quantity, Symbol};
use crate::trading::{
    ClientOrderId, Fill, FillId, OrderEvent, OrderEventKind, OrderId, OrderRequest, OrderSide,
    OrderStatus,
};

/// Agregado raíz del subdominio de trading: una orden ya aceptada por el broker.
///
/// Invariantes que garantiza:
/// - `filled_quantity <= request.quantity()` en todo momento.
/// - El estado solo cambia mediante transiciones válidas de `OrderStatus`.
/// - Una ejecución con el mismo `FillId` nunca se aplica dos veces.
/// - Si una operación falla, el agregado queda exactamente como estaba.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Order {
    id: OrderId,
    request: OrderRequest,
    status: OrderStatus,
    filled_quantity: Quantity,
    /// Suma de `precio * cantidad` de todas las ejecuciones; de aquí se deriva el precio promedio.
    filled_notional: Money,
    applied_fill_ids: HashSet<FillId>,
    reject_reason: Option<String>,
    submitted_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

/// Estado de una orden tal como lo reporta el broker, para reconstruir el agregado
/// (reconciliación al iniciar o consulta de órdenes abiertas).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderSnapshot {
    pub id: OrderId,
    pub request: OrderRequest,
    pub status: OrderStatus,
    pub filled_quantity: Quantity,
    pub average_fill_price: Option<Decimal>,
    pub reject_reason: Option<String>,
    pub submitted_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Order {
    /// Crea la orden en `PendingNew` cuando el broker devuelve su identificador.
    pub fn new(id: OrderId, request: OrderRequest, submitted_at: DateTime<Utc>) -> Self {
        Self {
            id,
            request,
            status: OrderStatus::PendingNew,
            filled_quantity: Quantity::ZERO,
            filled_notional: Money::zero(Currency::Usd),
            applied_fill_ids: HashSet::new(),
            reject_reason: None,
            submitted_at,
            updated_at: submitted_at,
        }
    }

    /// Reconstruye una orden a partir del estado reportado por el broker, validando
    /// que sea coherente antes de aceptarlo.
    pub fn restore(snapshot: OrderSnapshot) -> Result<Self, DomainError> {
        use OrderStatus::*;

        let OrderSnapshot {
            id,
            request,
            status,
            filled_quantity,
            average_fill_price,
            reject_reason,
            submitted_at,
            updated_at,
        } = snapshot;

        let quantity = request.quantity();
        if filled_quantity > quantity {
            return Err(DomainError::InconsistentOrderSnapshot(
                "la cantidad llenada excede la cantidad de la orden",
            ));
        }

        let has_fills = !filled_quantity.is_zero();
        let fully_filled = filled_quantity == quantity;
        let status_matches_fills = match status {
            PendingNew | New | Rejected => !has_fills,
            PartiallyFilled => has_fills && !fully_filled,
            Filled => fully_filled,
            PendingCancel | Canceled | Expired => !fully_filled,
        };
        if !status_matches_fills {
            return Err(DomainError::InconsistentOrderSnapshot(
                "el estado no corresponde a la cantidad llenada",
            ));
        }

        let filled_notional = match (has_fills, average_fill_price) {
            (false, _) => Money::zero(Currency::Usd),
            (true, Some(average)) if average > Decimal::ZERO => average
                .checked_mul(filled_quantity.value())
                .map(Money::usd)
                .ok_or(DomainError::ArithmeticOverflow("restore notional"))?,
            (true, _) => {
                return Err(DomainError::InconsistentOrderSnapshot(
                    "hay ejecuciones sin un precio promedio válido",
                ));
            }
        };

        Ok(Self {
            id,
            request,
            status,
            filled_quantity,
            filled_notional,
            applied_fill_ids: HashSet::new(),
            reject_reason,
            submitted_at,
            updated_at,
        })
    }

    // --- Consultas ---

    pub fn id(&self) -> &OrderId {
        &self.id
    }

    pub fn request(&self) -> &OrderRequest {
        &self.request
    }

    pub fn client_order_id(&self) -> &ClientOrderId {
        self.request.client_order_id()
    }

    pub fn symbol(&self) -> &Symbol {
        self.request.symbol()
    }

    pub fn side(&self) -> OrderSide {
        self.request.side()
    }

    pub fn status(&self) -> OrderStatus {
        self.status
    }

    pub fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }

    pub fn filled_quantity(&self) -> Quantity {
        self.filled_quantity
    }

    /// Cantidad aún no ejecutada. Nunca es negativa gracias a las invariantes del agregado.
    pub fn remaining_quantity(&self) -> Quantity {
        self.request
            .quantity()
            .checked_sub(self.filled_quantity)
            .unwrap_or(Quantity::ZERO)
    }

    pub fn filled_notional(&self) -> Money {
        self.filled_notional
    }

    /// Precio promedio ponderado de las ejecuciones, con precisión completa.
    pub fn average_fill_price(&self) -> Option<Decimal> {
        if self.filled_quantity.is_zero() {
            return None;
        }
        self.filled_notional
            .amount()
            .checked_div(self.filled_quantity.value())
    }

    pub fn reject_reason(&self) -> Option<&str> {
        self.reject_reason.as_deref()
    }

    pub fn submitted_at(&self) -> DateTime<Utc> {
        self.submitted_at
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    // --- Comandos ---

    /// El broker confirmó la orden y está vigente en el mercado.
    pub fn mark_accepted(&mut self, at: DateTime<Utc>) -> Result<OrderEvent, DomainError> {
        self.change_status(OrderStatus::New, at)?;
        Ok(self.event(OrderEventKind::Accepted, at))
    }

    /// Aplica una ejecución. Devuelve `Ok(None)` si la ejecución ya se había aplicado
    /// (evento duplicado), de modo que procesar dos veces el mismo evento es inofensivo.
    pub fn apply_fill(&mut self, fill: Fill) -> Result<Option<OrderEvent>, DomainError> {
        if self.applied_fill_ids.contains(fill.id()) {
            return Ok(None);
        }
        if !self.status.accepts_fills() {
            return Err(DomainError::FillNotAllowed(self.status));
        }

        let remaining = self.remaining_quantity();
        if fill.quantity() > remaining {
            return Err(DomainError::Overfill {
                fill: fill.quantity(),
                remaining,
            });
        }

        // Se calcula todo lo que puede fallar antes de modificar el agregado
        let new_filled = self.filled_quantity.checked_add(fill.quantity())?;
        let fill_notional = Money::notional(fill.price(), fill.quantity(), Currency::Usd)?;
        let new_notional = self.filled_notional.checked_add(fill_notional)?;

        let fully_filled = new_filled == self.request.quantity();
        let next_status = if fully_filled {
            OrderStatus::Filled
        } else if self.status == OrderStatus::PendingCancel {
            // Una ejecución parcial no cancela la solicitud de cancelación en curso
            OrderStatus::PendingCancel
        } else {
            OrderStatus::PartiallyFilled
        };
        if next_status != self.status {
            self.ensure_transition(next_status)?;
        }

        let at = fill.executed_at();
        self.filled_quantity = new_filled;
        self.filled_notional = new_notional;
        self.applied_fill_ids.insert(fill.id().clone());
        self.status = next_status;
        self.updated_at = at;

        let kind = if fully_filled {
            OrderEventKind::Filled { fill }
        } else {
            OrderEventKind::PartiallyFilled { fill }
        };
        Ok(Some(self.event(kind, at)))
    }

    /// Registra que se pidió cancelar la orden al broker.
    pub fn request_cancel(&mut self, at: DateTime<Utc>) -> Result<OrderEvent, DomainError> {
        self.change_status(OrderStatus::PendingCancel, at)?;
        Ok(self.event(OrderEventKind::CancelRequested, at))
    }

    /// El broker rechazó la cancelación: la orden vuelve a estar vigente.
    pub fn mark_cancel_rejected(&mut self, at: DateTime<Utc>) -> Result<OrderEvent, DomainError> {
        let next = if self.filled_quantity.is_zero() {
            OrderStatus::New
        } else {
            OrderStatus::PartiallyFilled
        };
        if self.status != OrderStatus::PendingCancel {
            return Err(DomainError::InvalidStatusTransition {
                from: self.status,
                to: next,
            });
        }
        self.change_status(next, at)?;
        Ok(self.event(OrderEventKind::CancelRejected, at))
    }

    pub fn mark_canceled(&mut self, at: DateTime<Utc>) -> Result<OrderEvent, DomainError> {
        self.change_status(OrderStatus::Canceled, at)?;
        Ok(self.event(OrderEventKind::Canceled, at))
    }

    pub fn mark_expired(&mut self, at: DateTime<Utc>) -> Result<OrderEvent, DomainError> {
        self.change_status(OrderStatus::Expired, at)?;
        Ok(self.event(OrderEventKind::Expired, at))
    }

    pub fn mark_rejected(
        &mut self,
        reason: impl Into<String>,
        at: DateTime<Utc>,
    ) -> Result<OrderEvent, DomainError> {
        self.change_status(OrderStatus::Rejected, at)?;
        let reason = reason.into();
        self.reject_reason = Some(reason.clone());
        Ok(self.event(OrderEventKind::Rejected { reason }, at))
    }

    // --- Auxiliares privados ---

    fn ensure_transition(&self, next: OrderStatus) -> Result<(), DomainError> {
        if self.status.can_transition_to(next) {
            Ok(())
        } else {
            Err(DomainError::InvalidStatusTransition {
                from: self.status,
                to: next,
            })
        }
    }

    fn change_status(&mut self, next: OrderStatus, at: DateTime<Utc>) -> Result<(), DomainError> {
        self.ensure_transition(next)?;
        self.status = next;
        self.updated_at = at;
        Ok(())
    }

    fn event(&self, kind: OrderEventKind, occurred_at: DateTime<Utc>) -> OrderEvent {
        OrderEvent {
            order_id: self.id.clone(),
            kind,
            occurred_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::{Price, Symbol};
    use crate::trading::{OrderType, TimeInForce};
    use proptest::prelude::*;
    use rust_decimal_macros::dec;

    fn at(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(seconds, 0).unwrap()
    }

    fn request(quantity: Decimal) -> OrderRequest {
        OrderRequest::new(
            ClientOrderId::new("tf-1").unwrap(),
            Symbol::new("AAPL").unwrap(),
            OrderSide::Buy,
            OrderType::Limit {
                limit_price: Price::new(dec!(190)).unwrap(),
            },
            Quantity::new(quantity).unwrap(),
            TimeInForce::Day,
        )
        .unwrap()
    }

    fn new_order(quantity: Decimal) -> Order {
        Order::new(OrderId::new("ord-1").unwrap(), request(quantity), at(0))
    }

    fn accepted_order(quantity: Decimal) -> Order {
        let mut order = new_order(quantity);
        order.mark_accepted(at(1)).unwrap();
        order
    }

    fn fill(id: &str, quantity: Decimal, price: Decimal) -> Fill {
        Fill::new(
            FillId::new(id).unwrap(),
            Quantity::new(quantity).unwrap(),
            Price::new(price).unwrap(),
            at(10),
        )
        .unwrap()
    }

    #[test]
    fn starts_pending_with_nothing_filled() {
        let order = new_order(dec!(10));
        assert_eq!(order.status(), OrderStatus::PendingNew);
        assert!(order.filled_quantity().is_zero());
        assert_eq!(order.remaining_quantity(), Quantity::new(dec!(10)).unwrap());
        assert_eq!(order.average_fill_price(), None);
    }

    #[test]
    fn partial_then_full_fill_computes_weighted_average() {
        let mut order = accepted_order(dec!(10));

        let event = order.apply_fill(fill("e1", dec!(4), dec!(189))).unwrap();
        assert!(matches!(
            event.map(|e| e.kind),
            Some(OrderEventKind::PartiallyFilled { .. })
        ));
        assert_eq!(order.status(), OrderStatus::PartiallyFilled);

        let event = order.apply_fill(fill("e2", dec!(6), dec!(190))).unwrap();
        assert!(matches!(
            event.map(|e| e.kind),
            Some(OrderEventKind::Filled { .. })
        ));
        assert_eq!(order.status(), OrderStatus::Filled);
        assert!(order.remaining_quantity().is_zero());
        // (4 * 189 + 6 * 190) / 10 = 189.6
        assert_eq!(order.average_fill_price(), Some(dec!(189.6)));
    }

    #[test]
    fn duplicate_fill_is_ignored() {
        let mut order = accepted_order(dec!(10));
        order.apply_fill(fill("e1", dec!(4), dec!(189))).unwrap();

        let duplicate = order.apply_fill(fill("e1", dec!(4), dec!(189))).unwrap();
        assert_eq!(duplicate, None);
        assert_eq!(order.filled_quantity(), Quantity::new(dec!(4)).unwrap());
    }

    #[test]
    fn overfill_is_rejected_without_changing_state() {
        let mut order = accepted_order(dec!(10));
        let before = order.clone();

        let result = order.apply_fill(fill("e1", dec!(11), dec!(189)));
        assert!(matches!(result, Err(DomainError::Overfill { .. })));
        assert_eq!(order, before);
    }

    #[test]
    fn terminal_order_rejects_fills() {
        let mut order = accepted_order(dec!(10));
        order.mark_canceled(at(2)).unwrap();

        let result = order.apply_fill(fill("e1", dec!(1), dec!(189)));
        assert_eq!(
            result,
            Err(DomainError::FillNotAllowed(OrderStatus::Canceled))
        );
    }

    #[test]
    fn partial_fill_during_pending_cancel_keeps_pending_cancel() {
        let mut order = accepted_order(dec!(10));
        order.request_cancel(at(2)).unwrap();

        order.apply_fill(fill("e1", dec!(3), dec!(189))).unwrap();
        assert_eq!(order.status(), OrderStatus::PendingCancel);

        order.mark_canceled(at(3)).unwrap();
        assert_eq!(order.status(), OrderStatus::Canceled);
        assert_eq!(order.filled_quantity(), Quantity::new(dec!(3)).unwrap());
    }

    #[test]
    fn fill_can_win_the_race_against_cancel() {
        let mut order = accepted_order(dec!(10));
        order.request_cancel(at(2)).unwrap();

        order.apply_fill(fill("e1", dec!(10), dec!(189))).unwrap();
        assert_eq!(order.status(), OrderStatus::Filled);
    }

    #[test]
    fn cancel_rejection_restores_previous_live_status() {
        let mut order = accepted_order(dec!(10));
        order.apply_fill(fill("e1", dec!(2), dec!(189))).unwrap();
        order.request_cancel(at(2)).unwrap();

        order.mark_cancel_rejected(at(3)).unwrap();
        assert_eq!(order.status(), OrderStatus::PartiallyFilled);
    }

    #[test]
    fn cancel_rejection_requires_pending_cancel() {
        let mut order = accepted_order(dec!(10));
        assert!(matches!(
            order.mark_cancel_rejected(at(2)),
            Err(DomainError::InvalidStatusTransition { .. })
        ));
    }

    #[test]
    fn rejection_stores_reason() {
        let mut order = new_order(dec!(10));
        let event = order
            .mark_rejected("insufficient buying power", at(1))
            .unwrap();

        assert_eq!(order.status(), OrderStatus::Rejected);
        assert_eq!(order.reject_reason(), Some("insufficient buying power"));
        assert!(matches!(event.kind, OrderEventKind::Rejected { .. }));
    }

    #[test]
    fn accepted_order_cannot_be_rejected() {
        let mut order = accepted_order(dec!(10));
        assert_eq!(
            order.mark_rejected("late", at(2)),
            Err(DomainError::InvalidStatusTransition {
                from: OrderStatus::New,
                to: OrderStatus::Rejected
            })
        );
    }

    fn snapshot(status: OrderStatus, filled: Decimal, average: Option<Decimal>) -> OrderSnapshot {
        OrderSnapshot {
            id: OrderId::new("ord-1").unwrap(),
            request: request(dec!(10)),
            status,
            filled_quantity: Quantity::new(filled).unwrap(),
            average_fill_price: average,
            reject_reason: None,
            submitted_at: at(0),
            updated_at: at(5),
        }
    }

    #[test]
    fn restores_consistent_snapshot() {
        let order = Order::restore(snapshot(
            OrderStatus::PartiallyFilled,
            dec!(4),
            Some(dec!(189.5)),
        ))
        .unwrap();

        assert_eq!(order.status(), OrderStatus::PartiallyFilled);
        assert_eq!(order.average_fill_price(), Some(dec!(189.5)));
        assert_eq!(order.filled_notional(), Money::usd(dec!(758)));
    }

    #[test]
    fn rejects_inconsistent_snapshots() {
        let cases = [
            snapshot(OrderStatus::New, dec!(4), Some(dec!(189))),
            snapshot(OrderStatus::Filled, dec!(4), Some(dec!(189))),
            snapshot(OrderStatus::PartiallyFilled, dec!(0), None),
            snapshot(OrderStatus::PartiallyFilled, dec!(4), None),
            snapshot(OrderStatus::Canceled, dec!(11), Some(dec!(189))),
        ];
        for case in cases {
            assert!(
                matches!(
                    Order::restore(case.clone()),
                    Err(DomainError::InconsistentOrderSnapshot(_))
                ),
                "debió rechazar {case:?}"
            );
        }
    }

    proptest! {
        // Con cualquier secuencia de ejecuciones, lo llenado nunca excede lo ordenado y el
        // estado final es coherente con la cantidad restante
        #[test]
        fn fills_never_exceed_order_quantity(
            order_quantity in 1i64..1_000,
            fill_quantities in prop::collection::vec(1i64..300, 0..20),
        ) {
            let mut order = accepted_order(Decimal::from(order_quantity));

            for (index, quantity) in fill_quantities.into_iter().enumerate() {
                let before = order.clone();
                let result = order.apply_fill(fill(&format!("e{index}"), Decimal::from(quantity), dec!(100)));

                match result {
                    Ok(_) => {}
                    Err(DomainError::Overfill { .. }) | Err(DomainError::FillNotAllowed(_)) => {
                        prop_assert_eq!(&order, &before);
                    }
                    Err(other) => prop_assert!(false, "error inesperado: {other}"),
                }

                let total = order.filled_quantity().checked_add(order.remaining_quantity()).unwrap();
                prop_assert_eq!(total.value(), Decimal::from(order_quantity));
                prop_assert_eq!(order.status() == OrderStatus::Filled, order.remaining_quantity().is_zero());
            }
        }
    }
}
