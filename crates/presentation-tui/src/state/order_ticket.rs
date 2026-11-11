use std::str::FromStr;

use rust_decimal::Decimal;
use tickforge_application::use_cases::trading::PlaceOrderCommand;
use tickforge_domain::shared::{Price, Quantity, Symbol};
use tickforge_domain::trading::{OrderSide, OrderType, TimeInForce};

/// Campo del ticket que tiene el foco.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TicketField {
    Side,
    OrderType,
    Quantity,
    LimitPrice,
    TimeInForce,
}

/// Tipo de orden elegido en el ticket (el precio límite se captura aparte como texto).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TicketOrderType {
    Market,
    Limit,
}

/// Formulario de orden nueva. Los números se capturan como texto y solo se convierten a
/// tipos del dominio al pedir la vista previa, con mensajes claros si algo no es válido.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderTicket {
    pub symbol: Symbol,
    pub side: OrderSide,
    pub order_type: TicketOrderType,
    pub quantity: String,
    pub limit_price: String,
    pub time_in_force: TimeInForce,
    pub focused: TicketField,
    pub error: Option<String>,
    /// Se pidió la vista previa y se espera la respuesta.
    pub awaiting_preview: bool,
    /// Cotización al abrir el ticket, para mostrarla y prellenar el precio límite.
    pub bid: Option<Price>,
    pub ask: Option<Price>,
}

/// Longitud máxima de los campos numéricos (evita textos absurdos).
const MAX_INPUT_LEN: usize = 12;

impl OrderTicket {
    pub fn new(symbol: Symbol, side: OrderSide, bid: Option<Price>, ask: Option<Price>) -> Self {
        Self {
            symbol,
            side,
            order_type: TicketOrderType::Market,
            quantity: String::new(),
            limit_price: String::new(),
            time_in_force: TimeInForce::Day,
            // Lo primero que el usuario escribe es la cantidad
            focused: TicketField::Quantity,
            error: None,
            awaiting_preview: false,
            bid,
            ask,
        }
    }

    /// Campos visibles en orden; el precio límite solo aparece en órdenes límite.
    pub fn fields(&self) -> Vec<TicketField> {
        let mut fields = vec![
            TicketField::Side,
            TicketField::OrderType,
            TicketField::Quantity,
        ];
        if self.order_type == TicketOrderType::Limit {
            fields.push(TicketField::LimitPrice);
        }
        fields.push(TicketField::TimeInForce);
        fields
    }

    pub fn next_field(&mut self) {
        self.move_focus(1);
    }

    pub fn previous_field(&mut self) {
        self.move_focus(-1);
    }

    fn move_focus(&mut self, step: isize) {
        let fields = self.fields();
        let current = fields
            .iter()
            .position(|field| *field == self.focused)
            .unwrap_or_default();
        let count = isize::try_from(fields.len()).unwrap_or(1);
        let current = isize::try_from(current).unwrap_or_default();
        let next = usize::try_from((current + step).rem_euclid(count)).unwrap_or_default();
        if let Some(field) = fields.get(next) {
            self.focused = *field;
        }
    }

    /// Alterna el valor del campo enfocado (lado, tipo o vigencia).
    pub fn toggle(&mut self) {
        self.error = None;
        match self.focused {
            TicketField::Side => {
                self.side = self.side.opposite();
            }
            TicketField::OrderType => {
                self.order_type = match self.order_type {
                    TicketOrderType::Market => TicketOrderType::Limit,
                    TicketOrderType::Limit => TicketOrderType::Market,
                };
                // Al pasar a límite se propone el precio de mercado actual del lado correcto
                if self.order_type == TicketOrderType::Limit && self.limit_price.is_empty() {
                    let suggested = match self.side {
                        OrderSide::Buy => self.ask,
                        OrderSide::Sell => self.bid,
                    };
                    if let Some(price) = suggested {
                        self.limit_price = crate::format::plain_price(price.value());
                    }
                }
            }
            TicketField::TimeInForce => {
                self.time_in_force = match self.time_in_force {
                    TimeInForce::Day => TimeInForce::Gtc,
                    TimeInForce::Gtc => TimeInForce::Day,
                };
            }
            TicketField::Quantity | TicketField::LimitPrice => {}
        }
    }

    /// Escribe un carácter en el campo numérico enfocado: solo dígitos y un punto decimal.
    pub fn input(&mut self, character: char) {
        let Some(text) = self.focused_text_mut() else {
            return;
        };
        let accepted = character.is_ascii_digit() || (character == '.' && !text.contains('.'));
        if accepted && text.len() < MAX_INPUT_LEN {
            text.push(character);
            self.error = None;
        }
    }

    pub fn backspace(&mut self) {
        if let Some(text) = self.focused_text_mut() {
            text.pop();
            self.error = None;
        }
    }

    fn focused_text_mut(&mut self) -> Option<&mut String> {
        match self.focused {
            TicketField::Quantity => Some(&mut self.quantity),
            TicketField::LimitPrice => Some(&mut self.limit_price),
            _ => None,
        }
    }

    /// Convierte el formulario en un comando validado, o explica qué corregir.
    pub fn to_command(&self) -> Result<PlaceOrderCommand, String> {
        if self.quantity.is_empty() {
            return Err("Escribe la cantidad de acciones".to_owned());
        }
        let quantity = Decimal::from_str(&self.quantity)
            .ok()
            .and_then(|value| Quantity::new(value).ok())
            .ok_or_else(|| format!("Cantidad no válida: '{}'", self.quantity))?;
        if quantity.is_zero() {
            return Err("La cantidad debe ser mayor que cero".to_owned());
        }

        let order_type = match self.order_type {
            TicketOrderType::Market => OrderType::Market,
            TicketOrderType::Limit => {
                if self.limit_price.is_empty() {
                    return Err("Escribe el precio límite".to_owned());
                }
                let limit_price = Decimal::from_str(&self.limit_price)
                    .ok()
                    .and_then(|value| Price::new(value).ok())
                    .ok_or_else(|| format!("Precio límite no válido: '{}'", self.limit_price))?;
                OrderType::Limit { limit_price }
            }
        };

        Ok(PlaceOrderCommand {
            symbol: self.symbol.clone(),
            side: self.side,
            order_type,
            quantity,
            time_in_force: self.time_in_force,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn ticket() -> OrderTicket {
        OrderTicket::new(
            Symbol::new("AAPL").unwrap(),
            OrderSide::Buy,
            Some(Price::new(dec!(350.77)).unwrap()),
            Some(Price::new(dec!(350.80)).unwrap()),
        )
    }

    fn type_text(ticket: &mut OrderTicket, text: &str) {
        text.chars().for_each(|c| ticket.input(c));
    }

    #[test]
    fn builds_a_market_order_command() {
        let mut ticket = ticket();
        type_text(&mut ticket, "10");

        let command = ticket.to_command().unwrap();
        assert_eq!(command.quantity, Quantity::new(dec!(10)).unwrap());
        assert_eq!(command.order_type, OrderType::Market);
        assert_eq!(command.side, OrderSide::Buy);
    }

    #[test]
    fn switching_to_limit_suggests_the_ask_for_buys() {
        let mut ticket = ticket();
        ticket.focused = TicketField::OrderType;
        ticket.toggle();

        assert_eq!(ticket.order_type, TicketOrderType::Limit);
        assert_eq!(ticket.limit_price, "350.80");
        assert!(ticket.fields().contains(&TicketField::LimitPrice));
    }

    #[test]
    fn numeric_fields_reject_letters_and_second_dots() {
        let mut ticket = ticket();
        type_text(&mut ticket, "1a.5.0");
        assert_eq!(ticket.quantity, "1.50");
    }

    #[test]
    fn explains_what_is_missing_or_invalid() {
        let mut ticket = ticket();
        assert_eq!(
            ticket.to_command().unwrap_err(),
            "Escribe la cantidad de acciones"
        );

        type_text(&mut ticket, "0");
        assert_eq!(
            ticket.to_command().unwrap_err(),
            "La cantidad debe ser mayor que cero"
        );

        ticket.quantity = "5".to_owned();
        ticket.order_type = TicketOrderType::Limit;
        assert_eq!(ticket.to_command().unwrap_err(), "Escribe el precio límite");
    }

    #[test]
    fn focus_cycles_through_visible_fields_only() {
        let mut ticket = ticket();
        // Mercado: Cantidad -> Vigencia -> Lado -> Tipo -> Cantidad
        ticket.next_field();
        assert_eq!(ticket.focused, TicketField::TimeInForce);
        ticket.next_field();
        assert_eq!(ticket.focused, TicketField::Side);
        ticket.previous_field();
        assert_eq!(ticket.focused, TicketField::TimeInForce);
    }
}
