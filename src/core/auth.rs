//! Cuenta offline.
//!
//! El UUID tiene que ser el mismo que calcula Java (`UUID.nameUUIDFromBytes`), porque
//! es el que el juego usa para el jugador y el que los servidores con `online-mode=false`
//! comparan. Si no coincide, cada launcher daría un jugador distinto.

use md5::{Digest, Md5};

use crate::core::error::{Error, Result};

/// Token ficticio para el modo offline (`--accessToken`).
pub const OFFLINE_ACCESS_TOKEN: &str = "0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfflineAccount {
    pub username: String,
    /// UUID en formato 8-4-4-4-12, minúsculas.
    pub uuid: String,
    pub access_token: String,
}

impl OfflineAccount {
    pub fn new(username: &str) -> Result<Self> {
        let username = username.trim();
        validate_username(username)?;
        Ok(Self {
            username: username.to_string(),
            uuid: offline_uuid(username),
            access_token: OFFLINE_ACCESS_TOKEN.to_string(),
        })
    }

    /// `--userType`: `msa` es lo que manda el launcher oficial hoy; `legacy` se usa
    /// solo para versiones antiguas que no conocían el concepto.
    pub fn user_type(&self) -> &'static str {
        "msa"
    }
}

/// Reglas de Mojang para un nick.
pub fn validate_username(username: &str) -> Result<()> {
    if username.len() < 3 || username.len() > 16 {
        return Err(Error::InvalidUsername(
            "debe tener entre 3 y 16 caracteres".into(),
        ));
    }
    if !username
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(Error::InvalidUsername(
            "solo se permiten letras, números y guion bajo".into(),
        ));
    }
    Ok(())
}

/// UUID v3 de Java: MD5 de `OfflinePlayer:<nick>` con los bits de versión y variante.
pub fn offline_uuid(username: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(format!("OfflinePlayer:{username}").as_bytes());
    let mut bytes: [u8; 16] = hasher.finalize().into();
    bytes[6] = (bytes[6] & 0x0f) | 0x30; // versión 3
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // variante RFC 4122
    format_uuid(&bytes)
}

fn format_uuid(bytes: &[u8; 16]) -> String {
    let hex = crate::core::hash::hex(bytes);
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_offline_coincide_con_java() {
        // Estos valores salen de `UUID.nameUUIDFromBytes(("OfflinePlayer:" + nick))`
        // en Java. Son la referencia para no romper la identidad del jugador.
        assert_eq!(
            offline_uuid("Notch"),
            "b50ad385-829d-3141-a216-7e7d7539ba7f"
        );
        assert_eq!(
            offline_uuid("jeb_"),
            "a762f560-4fce-3236-812a-b80efff0b62b"
        );
        assert_eq!(
            offline_uuid("Steve"),
            "5627dd98-e6be-3c21-b8a8-e92344183641"
        );
    }

    #[test]
    fn los_bits_de_version_y_variante_estan_puestos() {
        let uuid = offline_uuid("Steve");
        let hex = uuid.replace('-', "");
        assert_eq!(&hex[12..13], "3", "versión 3");
        assert!(matches!(&hex[16..17], "8" | "9" | "a" | "b"), "variante RFC 4122");
    }

    #[test]
    fn valida_nicks() {
        assert!(validate_username("Steve").is_ok());
        assert!(validate_username("steve_123").is_ok());
        assert!(validate_username("ab").is_err(), "menos de 3");
        assert!(validate_username(&"a".repeat(17)).is_err(), "más de 16");
        assert!(validate_username("ste ve").is_err(), "espacio");
        assert!(validate_username("steve-").is_err(), "guion medio");
        assert!(validate_username("áéí").is_err(), "no ascii");
    }

    #[test]
    fn la_cuenta_recorta_espacios() {
        let account = OfflineAccount::new("  Steve  ").unwrap();
        assert_eq!(account.username, "Steve");
        assert_eq!(account.access_token, OFFLINE_ACCESS_TOKEN);
    }
}
