//! Validación compartida de identificadores opacos (ids de broker, de cliente y de ejecución).

/// Devuelve el identificador recortado si no está vacío, no supera `max_len`
/// y todos sus caracteres cumplen `is_allowed`.
pub(crate) fn sanitize(raw: &str, max_len: usize, is_allowed: fn(char) -> bool) -> Option<String> {
    let trimmed = raw.trim();
    let valid = !trimmed.is_empty() && trimmed.len() <= max_len && trimmed.chars().all(is_allowed);
    valid.then(|| trimmed.to_owned())
}

/// Cualquier carácter ASCII visible (sin espacios ni caracteres de control).
pub(crate) fn is_visible_ascii(c: char) -> bool {
    c.is_ascii_graphic()
}
