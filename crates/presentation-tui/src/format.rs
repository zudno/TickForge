//! Formato de números, dinero y horas para mostrar en pantalla.

use chrono::{DateTime, TimeDelta, Utc};
use chrono_tz::America::New_York;
use rust_decimal::{Decimal, RoundingStrategy};
use tickforge_domain::shared::{Money, Price};

/// Marcador para datos que aún no llegan.
pub const MISSING: &str = "—";

/// Precio con 2 decimales (4 por debajo de 1 USD, donde cotizan en 0.0001).
pub fn price(value: Option<Price>) -> String {
    value.map_or_else(|| MISSING.to_owned(), |price| decimal_price(price.value()))
}

pub fn decimal_price(value: Decimal) -> String {
    with_thousands(&plain_price(value))
}

/// Precio sin separador de miles, apto para editarse en un campo de texto (`1234.50`).
pub fn plain_price(value: Decimal) -> String {
    let decimals = if value.abs() < Decimal::ONE { 4 } else { 2 };
    fixed(value, decimals)
}

/// Diferencial de precios: al menos 2 decimales y hasta 4, sin ceros sobrantes
/// (`0.02`, `0.015`, `0.0001`).
pub fn spread(value: Option<Decimal>) -> String {
    let Some(value) = value else {
        return MISSING.to_owned();
    };
    let rounded = round(value, 4).normalize();
    fixed(rounded, rounded.scale().max(2))
}

/// Redondeo "mitad hacia arriba", el que espera una persona al leer la pantalla.
///
/// Importante: el formato `{:.2}` de `Decimal` **trunca** en lugar de redondear, así que
/// siempre se redondea antes de formatear.
fn round(value: Decimal, decimals: u32) -> Decimal {
    value.round_dp_with_strategy(decimals, RoundingStrategy::MidpointAwayFromZero)
}

/// Número redondeado y con exactamente `decimals` decimales.
fn fixed(value: Decimal, decimals: u32) -> String {
    let precision = usize::try_from(decimals).unwrap_or(2);
    format!("{:.precision$}", round(value, decimals))
}

/// Monto con separador de miles y 2 decimales, sin moneda: `100,000.00`.
/// Usa el redondeo bancario de `Money::rounded`, el mismo de los reportes.
pub fn money(value: Money) -> String {
    with_thousands(&fixed(value.rounded().amount(), 2))
}

/// Monto con signo explícito: `+1,250.75` / `-80.10`.
pub fn signed_money(value: Money) -> String {
    let formatted = money(value);
    if value.amount() > Decimal::ZERO {
        format!("+{formatted}")
    } else {
        formatted
    }
}

/// Porcentaje con signo y 2 decimales: `+1.23%`.
pub fn percent(value: Option<Decimal>) -> String {
    match value.map(|value| round(value, 2)) {
        Some(value) if value > Decimal::ZERO => format!("+{}%", fixed(value, 2)),
        // Evita mostrar "-0.00%" cuando el cambio redondeado es cero
        Some(value) if value.is_zero() => "0.00%".to_owned(),
        Some(value) => format!("{}%", fixed(value, 2)),
        None => MISSING.to_owned(),
    }
}

/// Número de cuenta con solo los últimos 4 caracteres visibles: `••••7890`.
pub fn masked_account(number: &str) -> String {
    let characters: Vec<char> = number.chars().collect();
    let visible: String = characters[characters.len().saturating_sub(4)..]
        .iter()
        .collect();
    format!("••••{visible}")
}

/// Hora de Nueva York (la de la bolsa), por ejemplo `14:30:05 ET`.
pub fn new_york_time(instant: DateTime<Utc>) -> String {
    instant
        .with_timezone(&New_York)
        .format("%H:%M:%S ET")
        .to_string()
}

/// Duración compacta: `2h 05m`, `12m`, `3d 4h`.
pub fn duration(value: TimeDelta) -> String {
    let minutes = value.num_minutes().max(0);
    let (days, hours, minutes) = (minutes / 1_440, (minutes % 1_440) / 60, minutes % 60);
    match (days, hours) {
        (0, 0) => format!("{minutes}m"),
        (0, _) => format!("{hours}h {minutes:02}m"),
        _ => format!("{days}d {hours}h"),
    }
}

/// Inserta comas de miles en la parte entera de un número ya formateado.
fn with_thousands(number: &str) -> String {
    let (sign, unsigned) = number
        .strip_prefix('-')
        .map_or(("", number), |rest| ("-", rest));
    let (integer, fraction) = unsigned
        .split_once('.')
        .map_or((unsigned, None), |(i, f)| (i, Some(f)));

    let digits: Vec<char> = integer.chars().collect();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.iter().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(*digit);
    }

    match fraction {
        Some(fraction) => format!("{sign}{grouped}.{fraction}"),
        None => format!("{sign}{grouped}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn formats_money_with_thousands() {
        assert_eq!(money(Money::usd(dec!(1234567.891))), "1,234,567.89");
        assert_eq!(money(Money::usd(dec!(-950))), "-950.00");
        assert_eq!(signed_money(Money::usd(dec!(1250.75))), "+1,250.75");
    }

    #[test]
    fn formats_prices_by_magnitude() {
        assert_eq!(decimal_price(dec!(189.5)), "189.50");
        assert_eq!(decimal_price(dec!(0.51234)), "0.5123");
        assert_eq!(price(None), MISSING);
    }

    #[test]
    fn rounds_instead_of_truncating() {
        // Regresión: el formato `{:.2}` de Decimal trunca (1.1283 se mostraba como 1.12)
        assert_eq!(percent(Some(dec!(1.1283))), "+1.13%");
        assert_eq!(decimal_price(dec!(189.105)), "189.11");
        assert_eq!(percent(Some(dec!(-0.004))), "0.00%");
    }

    #[test]
    fn formats_spreads_compactly() {
        assert_eq!(spread(Some(dec!(0.0200))), "0.02");
        assert_eq!(spread(Some(dec!(0.015))), "0.015");
        assert_eq!(spread(Some(dec!(0.00012))), "0.0001");
        assert_eq!(spread(None), MISSING);
    }

    #[test]
    fn formats_percent_and_duration() {
        assert_eq!(percent(Some(dec!(1.234))), "+1.23%");
        assert_eq!(percent(Some(dec!(-0.5))), "-0.50%");
        assert_eq!(duration(TimeDelta::minutes(125)), "2h 05m");
        assert_eq!(duration(TimeDelta::minutes(9)), "9m");
        assert_eq!(duration(TimeDelta::hours(52)), "2d 4h");
    }

    #[test]
    fn shows_exchange_time_in_new_york() {
        // 14:30 UTC en marzo (antes del horario de verano) = 09:30 ET
        let instant = DateTime::parse_from_rfc3339("2024-03-01T14:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(new_york_time(instant), "09:30:00 ET");
    }
}
