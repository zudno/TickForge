use crate::error::AlpacaSetupError;

/// Prueba de que una persona confirmó, en esta ejecución, que quiere operar con dinero real.
///
/// Es la única forma de construir una configuración del entorno `Live` (ver
/// `AlpacaConfig::live`): no implementa `Clone` ni `Default` y solo se obtiene escribiendo la
/// frase exacta. Su uso está restringido al composition root; una prueba del workspace
/// (`no_live_access_in_tests`) falla si aparece en cualquier otro lugar.
#[derive(Debug)]
pub struct LiveTradingAuthorization {
    _confirmed: (),
}

impl LiveTradingAuthorization {
    /// Frase que hay que escribir, tal cual, para habilitar el modo real.
    pub const CONFIRMATION_PHRASE: &'static str = "OPERAR EN REAL";

    /// Acepta solo la frase exacta (se ignoran los espacios de los extremos y el salto de
    /// línea final). Mayúsculas incluidas: escribirla obliga a detenerse a leer.
    pub fn confirm(typed: &str) -> Result<Self, AlpacaSetupError> {
        if typed.trim() == Self::CONFIRMATION_PHRASE {
            Ok(Self { _confirmed: () })
        } else {
            Err(AlpacaSetupError::LiveNotConfirmed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_exact_phrase_confirms() {
        assert!(LiveTradingAuthorization::confirm("OPERAR EN REAL").is_ok());
        assert!(LiveTradingAuthorization::confirm("  OPERAR EN REAL\r\n").is_ok());

        for typed in ["", "operar en real", "OPERAR", "OPERAR  EN REAL", "si", "y"] {
            assert_eq!(
                LiveTradingAuthorization::confirm(typed).unwrap_err(),
                AlpacaSetupError::LiveNotConfirmed,
                "'{typed}' no debería confirmar"
            );
        }
    }
}
