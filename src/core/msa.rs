//! Cuenta Microsoft (MSA) con el flujo *device code*.
//!
//! Es el mismo flujo que usan lanzadores como Prism/ATLauncher: no abre
//! navegador embebido, pide al usuario abrir `microsoft.com/link` y escribir
//! un código corto. Detrás corre el OAuth estándar de Microsoft:
//!
//! 1. `devicecode`  → nos da el código y el intervalo de sondeo.
//! 2. Se sondea `token` (error 400 = todavía pendiente).
//! 3. `refresh_token` se guarda en disco (acceso offline futuro).
//! 4. Refresh → XBL (`user.auth.xboxlive.com`) → XSTS (`xsts.auth.xboxlive.com`).
//! 5. El token XSTS canjea el perfil de Minecraft (`api.minecraftservices.com`).
//!
//! Necesita un Client ID de Azure registrado como "público" con los permisos
//! `XboxLive.signin` + `offline_access` (ver docs/MICROSOFT-ACCOUNT.md).

use base64::Engine;
use serde::Deserialize;
use std::io::Read;

use crate::core::error::{Error, Result};
use crate::core::http::HttpClient;

const DEVICE_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const XBL_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MC_LOGIN_URL: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
const MC_PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";

/// Borra el banner de Xbox Live (XERR > 0: hay que aceptar términos en xbox.com).
fn xerr_hint(xerr: u32) -> &'static str {
    match xerr {
        2148916233 => "esta cuenta no tiene Xbox Live",
        2148916238 => "esta cuenta es de un menor: un adulto debe añadirla a una familia",
        2148916235 => "Xbox Live no está disponible en tu país",
        2148916236 | 2148916237 => "esta cuenta necesita verificación de email/edad en xbox.com",
        _ => "error de Xbox Live: inicia sesión en xbox.com y vuelve a intentar",
    }
}

#[derive(Deserialize)]
struct DeviceCodeResp {
    device_code: String,
    user_code: String,
    verification_uri: String,
    #[serde(default = "default_interval")]
    interval: u64,
    #[serde(default = "default_expires")]
    expires_in: u64,
}
fn default_interval() -> u64 {
    5
}
fn default_expires() -> u64 {
    900
}

#[derive(Deserialize)]
struct TokenResp {
    access_token: String,
    #[serde(default)]
    refresh_token: String,
}

#[derive(Deserialize)]
struct TokenErrorResp {
    error: String,
    #[serde(default)]
    error_description: String,
}

#[derive(Deserialize)]
struct XblXstsResp {
    #[serde(rename = "DisplayClaims")]
    display_claims: XblClaims,
    #[serde(rename = "Token")]
    token: String,
}
#[derive(Deserialize)]
struct XblClaims {
    #[serde(rename = "xui")]
    xui: Vec<XuiClaim>,
}
#[derive(Deserialize)]
struct XuiClaim {
    #[serde(rename = "uhs", default)]
    uhs: String,
}

#[derive(Deserialize)]
struct McLoginResp {
    access_token: String,
    #[serde(default)]
    expires_in: i64,
}

#[derive(Deserialize)]
struct ProfileResp {
    id: String,
    name: String,
}

/// El usuario ya inició sesión: nombre, UUID y acceso a Minecraft válidos.
#[derive(Debug, Clone)]
pub struct MsaAccount {
    pub username: String,
    /// UUID sin guiones → formateado 8-4-4-4-12 (minúsculas), como lo pide el juego.
    pub uuid: String,
    /// Token de acceso a Minecraft (lo pide `--accessToken`; caduca ~24 h,
    /// el launcher lo refresca al lanzar).
    pub mc_token: String,
    /// `refresh_token` de MSA: la llave para renovar todo sin re-login.
    pub refresh_token: String,
    /// Caducidad del token de Minecraft (segundos desde epoch).
    pub expires_at: i64,
}

/// Estado del sondeo (la UI muestra `UserCode` en pantalla mientras tanto).
#[derive(Debug, Clone)]
pub struct DeviceCode {
    /// El código corto que el usuario escribe en `verification_uri`.
    pub user_code: String,
    /// Casi siempre `https://www.microsoft.com/link`.
    pub verify_url: String,
    /// Segundos entre sondeos (Microsoft pide respetar el valor).
    pub interval: u64,
    /// Segundos hasta que el código caduca (típicamente 900 = 15 min).
    pub expires_in: u64,
    device_code: String,
}

impl DeviceCode {
    /// URL con el código precargado (https://www.microsoft.com/link?otc=XXXXX).
    pub fn url_with_code(&self) -> String {
        format!("{}?otc={}", self.verify_url.trim_end_matches('/'), self.user_code)
    }
}

/// Errores de sondeo que el llamador distingue para la UI.
#[derive(Debug)]
pub enum PollOutcome {
    /// Aún esperando: el usuario no ha confirmado el código.
    Pending,
    /// El usuario denegó o el código caducó: hay que reiniciar el flujo.
    Abandoned,
    /// ¡Listo!
    Done(Box<MsaAccount>),
    /// Fallo de red/protocolo (mensaje para mostrar).
    Failed(String),
}

/// Paso 1: pide a Microsoft el código que el usuario escribirá en el navegador.
pub fn begin_device_flow(http: &HttpClient, client_id: &str) -> Result<DeviceCode> {
    let body = [
        ("client_id", client_id),
        ("scope", "XboxLive.signin offline_access"),
    ];
    let response = http
        .agent
        .post(DEVICE_URL)
        .send_form(body)
        .map_err(|e| Error::Http(format!("devicecode: {e}")))?;
    let mut raw = String::new();
    response
        .into_body()
        .as_reader()
        .read_to_string(&mut raw)
        .map_err(|e| Error::Http(format!("devicecode body: {e}")))?;
    let parsed: DeviceCodeResp = serde_json::from_str(&raw)?;
    Ok(DeviceCode {
        user_code: parsed.user_code,
        verify_url: parsed.verification_uri,
        interval: parsed.interval.max(3),
        expires_in: parsed.expires_in,
        device_code: parsed.device_code,
    })
}

/// Paso 2: sondea el token. Respeta `interval` entre llamadas (bloquea;
/// llamar desde un hilo de fondo).
pub fn poll_once(
    http: &HttpClient,
    client_id: &str,
    device: &DeviceCode,
) -> Result<PollOutcome> {
    std::thread::sleep(std::time::Duration::from_secs(device.interval));
    let body = [
        ("client_id", client_id),
        ("device_code", device.device_code.as_str()),
        ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
    ];
    let response = http
        .agent
        .post(TOKEN_URL)
        .send_form(body)
        .map_err(|e| Error::Http(format!("token: {e}")))?;
    let status = response.status().as_u16();
    let mut raw = String::new();
    response
        .into_body()
        .as_reader()
        .read_to_string(&mut raw)
        .map_err(|e| Error::Http(format!("token body: {e}")))?;

    if status == 200 {
        let token: TokenResp = serde_json::from_str(&raw)?;
        return exchange_full_chain(http, &token.access_token, &token.refresh_token)
            .map(|account| PollOutcome::Done(Box::new(account)));
    }
    let err: TokenErrorResp = serde_json::from_str(&raw)
        .map_err(|e| Error::Http(format!("token error body: {e}")))?;
    match err.error.as_str() {
        "authorization_pending" => Ok(PollOutcome::Pending),
        "slow_down" => Ok(PollOutcome::Pending),
        "expired_token" | "authorization_declined" => Ok(PollOutcome::Abandoned),
        _ => Ok(PollOutcome::Failed(err.error_description)),
    }
}

/// Paso 3-5: MSA → XBL → XSTS → Minecraft. Devuelve la cuenta lista para jugar.
fn exchange_full_chain(
    http: &HttpClient,
    msa_token: &str,
    refresh_token: &str,
) -> Result<MsaAccount> {
    let xbl = xbl_token(http, msa_token)?;
    let (xsts_token, uhs) = xsts_token(http, &xbl)?;

    let mc = mc_login(http, &xsts_token, &uhs)?;
    let expires_at = now_epoch() + mc.expires_in.max(3600);

    let profile = http.get_json::<ProfileResp>(MC_PROFILE_URL).map_err(|_| {
        Error::Auth("el token no tiene Minecraft Java Edition (¿compraste el juego?)".into())
    })?;

    Ok(MsaAccount {
        uuid: dashed(&profile.id),
        username: profile.name,
        mc_token: mc.access_token,
        refresh_token: refresh_token.to_string(),
        expires_at,
    })
}

/// Renueva la sesión con el refresh_token guardado (para lanzar el juego).
pub fn refresh(http: &HttpClient, client_id: &str, refresh_token: &str) -> Result<MsaAccount> {
    let body = [
        ("client_id", client_id),
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("scope", "XboxLive.signin offline_access"),
    ];
    let response = http
        .agent
        .post(TOKEN_URL)
        .send_form(body)
        .map_err(|e| Error::Http(format!("refresh: {e}")))?;
    let status = response.status().as_u16();
    let mut raw = String::new();
    response
        .into_body()
        .as_reader()
        .read_to_string(&mut raw)
        .map_err(|e| Error::Http(format!("refresh body: {e}")))?;
    if status != 200 {
        return Err(Error::Auth(
            "la sesión de Microsoft caducó: vuelve a iniciar sesión".into(),
        ));
    }
    let token: TokenResp = serde_json::from_str(&raw)?;
    // Microsoft rota el refresh_token: si trae uno nuevo, manda el nuevo.
    let refresh = if token.refresh_token.is_empty() {
        refresh_token
    } else {
        token.refresh_token.as_str()
    };
    exchange_full_chain(http, &token.access_token, refresh)
}

fn xbl_token(http: &HttpClient, msa_token: &str) -> Result<String> {
    let body = serde_json::json!({
        "Properties": {
            "AuthMethod": "RPS",
            "SiteName": "user.auth.xboxlive.com",
            "RpsTicket": format!("d={msa_token}"),
        },
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT",
    });
    let response = http
        .agent
        .post(XBL_URL)
        .send_json(body)
        .map_err(|e| Error::Http(format!("XBL: {e}")))?;
    let parsed: XblXstsResp = read_json(response, "XBL")?;
    Ok(parsed.token)
}

fn xsts_token(http: &HttpClient, xbl_token: &str) -> Result<(String, String)> {
    let body = serde_json::json!({
        "Properties": {
            "SandboxId": "RETAIL",
            "UserTokens": [xbl_token],
        },
        "RelyingParty": "rp://api.minecraftservices.com/",
        "TokenType": "JWT",
    });
    let response = http
        .agent
        .post(XSTS_URL)
        .send_json(body)
        .map_err(|e| Error::Http(format!("XSTS: {e}")))?;
    let status = response.status().as_u16();
    let mut raw = String::new();
    response
        .into_body()
        .as_reader()
        .read_to_string(&mut raw)
        .map_err(|e| Error::Http(format!("XSTS body: {e}")))?;
    if status != 200 {
        // Los errores de XSTS traen XERR en el cuerpo: pista legible.
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(xerr) = value["XErr"].as_u64() {
                return Err(Error::Auth(xerr_hint(xerr as u32).to_string()));
            }
        }
        return Err(Error::Auth(format!("XSTS rechazó la sesión (HTTP {status})")));
    }
    let parsed: XblXstsResp = serde_json::from_str(&raw)?;
    let uhs = parsed
        .display_claims
        .xui
        .first()
        .map(|claim| claim.uhs.clone())
        .ok_or_else(|| Error::Auth("XSTS sin uhs".into()))?;
    Ok((parsed.token, uhs))
}

fn mc_login(http: &HttpClient, xsts_token: &str, uhs: &str) -> Result<McLoginResp> {
    let body = serde_json::json!({ "identityToken": format!("XBL3.0 x={uhs};{xsts_token}") });
    let response = http
        .agent
        .post(MC_LOGIN_URL)
        .send_json(body)
        .map_err(|e| Error::Http(format!("Minecraft login: {e}")))?;
    read_json(response, "Minecraft login")
}

fn read_json<T: for<'de> Deserialize<'de>>(
    response: ureq::http::Response<ureq::Body>,
    what: &str,
) -> Result<T> {
    let mut raw = String::new();
    response
        .into_body()
        .as_reader()
        .read_to_string(&mut raw)
        .map_err(|e| Error::Http(format!("{what} body: {e}")))?;
    serde_json::from_str(&raw).map_err(|e| Error::Http(format!("{what}: {e}")))
}

fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// UUID sin guiones → 8-4-4-4-12 (el formato que el juego espera en `--uuid`).
fn dashed(id: &str) -> String {
    let clean: String = id.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if clean.len() < 32 {
        return clean;
    }
    format!(
        "{}-{}-{}-{}-{}",
        &clean[..8],
        &clean[8..12],
        &clean[12..16],
        &clean[16..20],
        &clean[20..32]
    )
}

/// Sesión guardada en `msa-session.json` (refresh_token + caché del perfil).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredSession {
    pub refresh_token: String,
    pub username: String,
    pub uuid: String,
    /// Caducidad del token de Minecraft cacheado (segundos desde epoch).
    #[serde(default)]
    pub expires_at: i64,
    #[serde(default)]
    pub mc_token: String,
}

pub fn save_session(paths: &crate::core::paths::Paths, session: &StoredSession) -> Result<()> {
    let dir = paths.root();
    std::fs::create_dir_all(dir).map_err(|e| crate::core::error::Error::io(dir, e))?;
    let path = dir.join("msa-session.json");
    let json = serde_json::to_string_pretty(session)?;
    std::fs::write(&path, json)
        .map_err(|e| crate::core::error::Error::io(&path, e))
}

pub fn load_session(paths: &crate::core::paths::Paths) -> Option<StoredSession> {
    let path = paths.root().join("msa-session.json");
    let raw = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&raw).ok()
}

pub fn clear_session(paths: &crate::core::paths::Paths) {
    let _ = std::fs::remove_file(paths.root().join("msa-session.json"));
}

impl StoredSession {
    pub fn from_account(account: &MsaAccount) -> Self {
        Self {
            refresh_token: account.refresh_token.clone(),
            username: account.username.clone(),
            uuid: account.uuid.clone(),
            expires_at: account.expires_at,
            mc_token: account.mc_token.clone(),
        }
    }

    /// ¿El token cacheado sigue vigente por >5 min?
    pub fn fresh(&self) -> bool {
        self.expires_at > now_epoch() + 300
    }
}

/// Base64-URL sin padding (no se usa hoy, pero documenta el formato del JWT
/// por si se necesita inspeccionar el token en un futuro).
#[allow(dead_code)]
fn b64url(data: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_uuid_se_formatea_con_guiones() {
        assert_eq!(
            dashed("7125ba8b1c864508b92bb5c0494e5d97"),
            "7125ba8b-1c86-4508-b92b-b5c0494e5d97"
        );
        // Entradas inválidas: devuelve lo que quede tras filtrar hex y no pánico.
        assert_eq!(dashed("corto"), "c");
        assert_eq!(dashed(""), "");
    }

    #[test]
    fn la_url_de_verificacion_lleva_el_codigo() {
        let device = DeviceCode {
            user_code: "ABC123XYZ".into(),
            verify_url: "https://www.microsoft.com/link".into(),
            interval: 5,
            expires_in: 900,
            device_code: "xyz".into(),
        };
        assert_eq!(device.url_with_code(), "https://www.microsoft.com/link?otc=ABC123XYZ");
    }

    #[test]
    fn la_sesion_guardada_selee() {
        // roundtrip por serde, sin disco: valida los campos y el default.
        let json = r#"{"refresh_token":"r1","username":"Steve","uuid":"aa-bb"}"#;
        let session: StoredSession = serde_json::from_str(json).unwrap();
        assert_eq!(session.username, "Steve");
        assert!(!session.fresh());
    }
}
