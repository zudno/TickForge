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
