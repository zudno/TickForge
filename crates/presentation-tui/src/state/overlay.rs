use tickforge_application::use_cases::trading::OrderPreview;
use tickforge_domain::trading::Order;

use crate::state::OrderTicket;

/// Ventana modal activa. Mientras hay una, captura el teclado.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Overlay {
    #[default]
    None,
    Help,
    /// Formulario de orden nueva.
    Ticket(OrderTicket),
    /// Confirmación de la orden ya evaluada por la política de riesgo.
    Preview(PreviewOverlay),
    /// Confirmación de cancelación de una orden abierta.
    CancelConfirm(CancelConfirmation),
    /// Confirmación para activar o desactivar el kill switch.
    KillSwitch(KillSwitchDialog),
    /// Captura de un símbolo para agregarlo a la watchlist.
    AddSymbol(SymbolInput),
}

/// Campo de texto para un símbolo nuevo (se normaliza a mayúsculas al escribir).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SymbolInput {
    pub text: String,
    pub error: Option<String>,
    /// Se está verificando el símbolo con el proveedor de datos.
    pub awaiting: bool,
}

impl SymbolInput {
    /// Longitud máxima de un símbolo (igual que `Symbol::MAX_LEN`).
    const MAX_LEN: usize = 10;

    pub fn input(&mut self, character: char) {
        let accepted = character.is_ascii_alphanumeric() || character == '.' || character == '-';
        if accepted && self.text.len() < Self::MAX_LEN {
            self.text.push(character.to_ascii_uppercase());
            self.error = None;
        }
    }

    pub fn backspace(&mut self) {
        self.text.pop();
        self.error = None;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewOverlay {
    pub preview: OrderPreview,
    /// Ticket de origen, para volver a editar con `Esc`.
    pub ticket: OrderTicket,
    /// Se envió la orden y se espera la respuesta del broker.
    pub submitting: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancelConfirmation {
    pub order: Order,
    pub submitting: bool,
    pub error: Option<String>,
}

/// Confirmación del kill switch. `engage` indica si se va a activar (o a desactivar).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KillSwitchDialog {
    pub engage: bool,
    /// Órdenes abiertas conocidas al abrir la ventana (las que se cancelarían).
    pub open_orders: usize,
    pub submitting: bool,
    pub error: Option<String>,
}
