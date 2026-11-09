use std::fmt;

/// Estado del ciclo de vida de una orden.
///
/// | Desde             | Hacia                                                                 |
/// |-------------------|-----------------------------------------------------------------------|
/// | `PendingNew`      | `New`, `PartiallyFilled`, `Filled`, `PendingCancel`, `Canceled`, `Rejected`, `Expired` |
/// | `New`             | `PartiallyFilled`, `Filled`, `PendingCancel`, `Canceled`, `Expired`   |
/// | `PartiallyFilled` | `PartiallyFilled`, `Filled`, `PendingCancel`, `Canceled`, `Expired`   |
/// | `PendingCancel`   | `New`, `PartiallyFilled` (cancelación rechazada), `Filled`, `Canceled`, `Expired` |
/// | Terminales        | ninguno (`Filled`, `Canceled`, `Rejected`, `Expired`)                 |
///
/// Se permite "saltar" estados intermedios (por ejemplo `PendingNew -> Filled`) porque
/// los eventos del broker pueden llegar agrupados o perderse durante una reconexión.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OrderStatus {
    /// Enviada al broker, aún sin confirmar.
    PendingNew,
    /// Aceptada por el broker y vigente en el mercado.
    New,
    /// Ejecutada parcialmente y todavía vigente.
    PartiallyFilled,
    /// Ejecutada por completo.
    Filled,
    /// Se solicitó la cancelación y el broker aún no la confirma.
    PendingCancel,
    /// Cancelada (puede tener ejecuciones parciales previas).
    Canceled,
    /// Rechazada por el broker al recibirla.
    Rejected,
    /// Venció su vigencia (por ejemplo, una orden `Day` al cierre).
    Expired,
}

impl OrderStatus {
    /// Valida las transiciones legales de la máquina de estados.
    /// Cualquier otra transición indica un error de sincronización con el broker.
    pub fn can_transition_to(self, next: OrderStatus) -> bool {
        use OrderStatus::*;

        // `match` exhaustivo: si se agrega un estado nuevo, el compilador obliga a definir
        // sus transiciones aquí
        let allowed: &[OrderStatus] = match self {
            PendingNew => &[
                New,
                PartiallyFilled,
                Filled,
                PendingCancel,
                Canceled,
                Rejected,
                Expired,
            ],
            New => &[PartiallyFilled, Filled, PendingCancel, Canceled, Expired],
            PartiallyFilled => &[PartiallyFilled, Filled, PendingCancel, Canceled, Expired],
            // Si el broker rechaza la cancelación, la orden vuelve a estar vigente
            PendingCancel => &[New, PartiallyFilled, Filled, Canceled, Expired],
            Filled | Canceled | Rejected | Expired => &[],
        };
        allowed.contains(&next)
    }

    /// Indica si la orden ya no puede cambiar.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Filled | Self::Canceled | Self::Rejected | Self::Expired
        )
    }

    /// Indica si la orden puede recibir ejecuciones en este estado.
    pub fn accepts_fills(self) -> bool {
        matches!(
            self,
            Self::PendingNew | Self::New | Self::PartiallyFilled | Self::PendingCancel
        )
    }

    /// Indica si se puede solicitar la cancelación en este estado.
    pub fn is_cancelable(self) -> bool {
        matches!(self, Self::PendingNew | Self::New | Self::PartiallyFilled)
    }

    /// Código estable para logs y persistencia.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PendingNew => "pending_new",
            Self::New => "new",
            Self::PartiallyFilled => "partially_filled",
            Self::Filled => "filled",
            Self::PendingCancel => "pending_cancel",
            Self::Canceled => "canceled",
            Self::Rejected => "rejected",
            Self::Expired => "expired",
        }
    }

    /// Todos los estados, útil para pruebas exhaustivas.
    pub const ALL: [OrderStatus; 8] = [
        Self::PendingNew,
        Self::New,
        Self::PartiallyFilled,
        Self::Filled,
        Self::PendingCancel,
        Self::Canceled,
        Self::Rejected,
        Self::Expired,
    ];
}

impl fmt::Display for OrderStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::OrderStatus::*;
    use super::*;

    #[test]
    fn terminal_states_have_no_outgoing_transitions() {
        for from in OrderStatus::ALL.into_iter().filter(|s| s.is_terminal()) {
            for to in OrderStatus::ALL {
                assert!(
                    !from.can_transition_to(to),
                    "{from} -> {to} no debería ser válido"
                );
            }
        }
    }

    #[test]
    fn nothing_transitions_back_to_pending_new() {
        for from in OrderStatus::ALL {
            assert!(!from.can_transition_to(PendingNew));
        }
    }

    #[test]
    fn only_pending_new_can_be_rejected() {
        for from in OrderStatus::ALL {
            assert_eq!(from.can_transition_to(Rejected), from == PendingNew);
        }
    }

    #[test]
    fn happy_path_is_valid() {
        assert!(PendingNew.can_transition_to(New));
        assert!(New.can_transition_to(PartiallyFilled));
        assert!(PartiallyFilled.can_transition_to(PartiallyFilled));
        assert!(PartiallyFilled.can_transition_to(Filled));
    }

    #[test]
    fn cancel_path_is_valid() {
        assert!(New.can_transition_to(PendingCancel));
        assert!(PendingCancel.can_transition_to(Canceled));
        // La ejecución puede ganarle a la cancelación
        assert!(PendingCancel.can_transition_to(Filled));
        assert!(!PendingCancel.can_transition_to(PendingCancel));
    }

    #[test]
    fn helpers_are_consistent_with_terminality() {
        for status in OrderStatus::ALL {
            if status.is_terminal() {
                assert!(!status.accepts_fills());
                assert!(!status.is_cancelable());
            }
        }
    }
}
