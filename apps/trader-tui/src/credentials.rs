//! Llaves de API de cada entorno.
//!
//! - **Paper**: variables `ALPACA_API_KEY_ID` y `ALPACA_API_SECRET_KEY` (archivo `.env`).
//! - **Real**: solo en el almacén de credenciales del sistema (en Windows, el Administrador
//!   de credenciales). Nunca se leen de `.env` ni de variables de entorno, para que un
//!   archivo copiado por error no pueda dar acceso a dinero real.

use anyhow::{Context, bail};
use tickforge_adapter_alpaca::AlpacaCredentials;

use crate::settings;

/// Servicio bajo el que se guardan las entradas en el almacén de credenciales.
const KEYRING_SERVICE: &str = "tickforge";
const LIVE_KEY_ID_ENTRY: &str = "alpaca-live-api-key-id";
const LIVE_SECRET_KEY_ENTRY: &str = "alpaca-live-api-secret-key";

/// Llaves de paper desde el entorno.
pub fn paper_from_env() -> anyhow::Result<AlpacaCredentials> {
    Ok(AlpacaCredentials::new(
        settings::required("ALPACA_API_KEY_ID")?,
        settings::required("ALPACA_API_SECRET_KEY")?,
    ))
}

/// Llaves de la cuenta real desde el almacén de credenciales.
pub fn load_live() -> anyhow::Result<AlpacaCredentials> {
    let key_id = read_entry(LIVE_KEY_ID_ENTRY)?;
    let secret_key = read_entry(LIVE_SECRET_KEY_ENTRY)?;
    match (key_id, secret_key) {
        (Some(key_id), Some(secret_key)) => {
            validate_live_key_id(&key_id)?;
            Ok(AlpacaCredentials::new(key_id, secret_key))
        }
        _ => bail!(
            "no hay llaves de la cuenta real guardadas. Guárdalas con:\n\n    \
             tickforge credentials set-live"
        ),
    }
}

/// Pide las llaves reales en la consola (sin mostrarlas) y las guarda.
pub fn set_live_interactive() -> anyhow::Result<()> {
    println!("Llaves de la cuenta REAL de Alpaca (no se muestran al escribir).");
    println!("Se guardan en el almacén de credenciales del sistema, nunca en archivos.\n");

    let key_id = rpassword::prompt_password("API Key ID: ")
        .context("no se pudo leer la llave desde la consola")?;
    let key_id = key_id.trim().to_owned();
    validate_live_key_id(&key_id)?;

    let secret_key = rpassword::prompt_password("API Secret Key: ")
        .context("no se pudo leer el secreto desde la consola")?;
    let secret_key = secret_key.trim().to_owned();
    validate_secret(&secret_key)?;

    entry(LIVE_KEY_ID_ENTRY)?
        .set_password(&key_id)
        .context("no se pudo guardar la llave en el almacén de credenciales")?;
    entry(LIVE_SECRET_KEY_ENTRY)?
        .set_password(&secret_key)
        .context("no se pudo guardar el secreto en el almacén de credenciales")?;

    println!(
        "\nLlaves guardadas ({} caracteres de llave, {} de secreto).",
        key_id.chars().count(),
        secret_key.chars().count()
    );
    println!("Para usarlas: TICKFORGE_MODE=live en .env y `tickforge --live`.");
    Ok(())
}

/// Borra las llaves reales. No falla si no existían.
pub fn delete_live() -> anyhow::Result<()> {
    let mut deleted = 0;
    for name in [LIVE_KEY_ID_ENTRY, LIVE_SECRET_KEY_ENTRY] {
        match entry(name)?.delete_credential() {
            Ok(()) => deleted += 1,
            Err(keyring::Error::NoEntry) => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("no se pudo borrar la entrada '{name}'"));
            }
        }
    }
    if deleted == 0 {
        println!("No había llaves de la cuenta real guardadas.");
    } else {
        println!("Llaves de la cuenta real borradas del almacén de credenciales.");
    }
    Ok(())
}

fn entry(name: &str) -> anyhow::Result<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, name).with_context(|| {
        format!("no se pudo abrir la entrada '{name}' del almacén de credenciales")
    })
}

fn read_entry(name: &str) -> anyhow::Result<Option<String>> {
    match entry(name)?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(error).with_context(|| {
            format!("no se pudo leer la entrada '{name}' del almacén de credenciales")
        }),
    }
}

/// Rechaza llaves vacías, con espacios o evidentemente de paper (Alpaca emite las de paper
/// con prefijo `PK`). El prefijo es solo una pista: sirve para rechazar, no para aprobar.
fn validate_live_key_id(key_id: &str) -> anyhow::Result<()> {
    if key_id.is_empty() {
        bail!("la llave de la cuenta real está vacía");
    }
    if key_id.chars().any(char::is_whitespace) {
        bail!("la llave de la cuenta real contiene espacios");
    }
    if key_id.starts_with("PK") {
        bail!(
            "esa llave empieza con 'PK', que es el prefijo de las llaves de paper. Usa las \
             llaves de la cuenta real (normalmente empiezan con 'AK')"
        );
    }
    Ok(())
}

fn validate_secret(secret_key: &str) -> anyhow::Result<()> {
    if secret_key.is_empty() {
        bail!("el secreto de la cuenta real está vacío");
    }
    if secret_key.chars().any(char::is_whitespace) {
        bail!("el secreto de la cuenta real contiene espacios");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paper_or_malformed_keys_are_never_stored_as_live() {
        assert!(validate_live_key_id("AKABCDEF123").is_ok());
        assert!(validate_live_key_id("").is_err());
        assert!(validate_live_key_id("AK ABC").is_err());
        assert!(validate_live_key_id("PKABCDEF123").is_err());
    }

    #[test]
    fn secrets_must_be_non_empty_and_without_spaces() {
        assert!(validate_secret("abc123").is_ok());
        assert!(validate_secret("").is_err());
        assert!(validate_secret("abc 123").is_err());
    }
}
