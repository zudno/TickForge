//! Textos en español para los tipos del dominio que se muestran al usuario.

use tickforge_domain::trading::{
    Order, OrderRequest, OrderSide, OrderStatus, OrderType, TimeInForce,
};

use crate::format;

pub fn side(side: OrderSide) -> &'static str {
    match side {
        OrderSide::Buy => "COMPRA",
        OrderSide::Sell => "VENTA",
    }
}

pub fn order_type(order_type: OrderType) -> String {
    match order_type {
        OrderType::Market => "Mercado".to_owned(),
        OrderType::Limit { limit_price } => {
            format!("Límite {}", format::decimal_price(limit_price.value()))
        }
    }
}

pub fn time_in_force(time_in_force: TimeInForce) -> &'static str {
    match time_in_force {
        TimeInForce::Day => "Día",
        TimeInForce::Gtc => "Hasta cancelar",
    }
}

pub fn status(status: OrderStatus) -> &'static str {
    match status {
        OrderStatus::PendingNew => "Pendiente",
        OrderStatus::New => "Abierta",
        OrderStatus::PartiallyFilled => "Parcial",
        OrderStatus::Filled => "Ejecutada",
        OrderStatus::PendingCancel => "Cancelando",
        OrderStatus::Canceled => "Cancelada",
        OrderStatus::Rejected => "Rechazada",
        OrderStatus::Expired => "Expirada",
    }
}

/// Resumen de una solicitud: `COMPRA 10 AAPL · Límite 350.00`.
pub fn request_summary(request: &OrderRequest) -> String {
    format!(
        "{} {} {} · {}",
        side(request.side()),
        request.quantity(),
        request.symbol(),
        order_type(request.order_type())
    )
}

pub fn order_summary(order: &Order) -> String {
    request_summary(order.request())
}
