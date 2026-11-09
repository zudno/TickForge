use std::fmt;

/// Dirección de una orden.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OrderSide {
    Buy,
    Sell,
}

impl OrderSide {
    /// Código estable para logs y persistencia (no para mostrar al usuario).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Buy => "buy",
            Self::Sell => "sell",
        }
    }

    /// Lado contrario; útil para cerrar una posición.
    pub fn opposite(self) -> Self {
        match self {
            Self::Buy => Self::Sell,
            Self::Sell => Self::Buy,
        }
    }
}

impl fmt::Display for OrderSide {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opposite_is_an_involution() {
        for side in [OrderSide::Buy, OrderSide::Sell] {
            assert_ne!(side.opposite(), side);
            assert_eq!(side.opposite().opposite(), side);
        }
    }
}
