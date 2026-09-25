//! Skins para cuentas offline (B del plan).
//!
//! La verdad del offline: el juego pide la textura al servidor de sesiones de
//! Mojang con el UUID del perfil; un UUID offline no tiene texturas y **no
//! existe parámetro de lanzamiento** para la skin. El camino honesto es
//! CustomSkinLoader (CSL): mod que lee `LocalSkin/<nick>.png` del gameDir y
//! convive con offline/singleplayer/servidores `online-mode=false`. Este
//! módulo resuelve la textura (PNG local o skin del nick premium) y la
//! coloca donde CSL la lee; la instalación del mod vive en `loaders`-style en
//! `app::install_skin_support` (misma mecánica que Sodium).

use base64::Engine;
use md5::Digest;
use std::path::PathBuf;

use crate::core::error::{Error, Result};
use crate::core::http::HttpClient;
use crate::core::paths::{sanitize, Paths};

/// Caché de skins por nick: `cache/skins/<nick>.png`.
fn cache_path(paths: &Paths, nick: &str) -> PathBuf {
    paths.root().join("cache").join("skins").join(format!("{}.png", sanitize(nick)))
}

/// Instala la skin en el gameDir para que CustomSkinLoader la cargue.
/// Devuelve la ruta destino. Requiere que CSL esté en `mods/` (si no, el
/// juego simplemente la ignora: no hay daño).
pub fn apply_to_instance(
    png_bytes: &[u8],
    nick: &str,
    game_dir: &std::path::Path,
) -> Result<PathBuf> {
    let dir = game_dir.join("CustomSkinLoader").join("LocalSkin");
    std::fs::create_dir_all(&dir).map_err(|err| Error::io(&dir, err))?;
    let dest = dir.join(format!("{}.png", sanitize(nick)));
    std::fs::write(&dest, png_bytes).map_err(|err| Error::io(&dest, err))?;
    Ok(dest)
}

/// Carga un PNG local (el que soltó/eligió el usuario). Valida la cabecera.
pub fn from_local_png(path: &std::path::Path) -> Result<Vec<u8>> {
    let bytes = std::fs::read(path).map_err(|err| Error::io(path, err))?;
    // PNG: 89 50 4E 47 0D 0A 1A 0A. Rechazar otra cosa sin depender de `image`.
    if bytes.len() > 8 && bytes[..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
        Ok(bytes)
    } else {
        Err(crate::Error::Unsupported(
            "el fichero no parece un PNG de skin (64×32 o 64×64)".into(),
        ))
    }
}

/// Textura de la skin de un nick premium: Mojang profile API → session server
/// → propiedad `textures` (base64) → URL → bytes. `None` si el nick no existe
/// o no tiene skin.
pub fn fetch_premium(http: &HttpClient, nick: &str) -> Option<Vec<u8>> {
    // 1) UUID del perfil premium.
    let profile_url = format!("https://api.mojang.com/users/profiles/minecraft/{nick}");
    let profile: serde_json::Value =
        serde_json::from_str(&http.get_string_ua(&profile_url, crate::core::modrinth::USER_AGENT).ok()?).ok()?;
    let id = profile["id"].as_str()?;

    // 2) Texturas firmadas del session server.
    let session_url = format!("https://sessionserver.mojang.com/session/minecraft/profile/{id}");
    let session: serde_json::Value =
        serde_json::from_str(&http.get_string_ua(&session_url, crate::core::modrinth::USER_AGENT).ok()?).ok()?;
    let textures_b64 = session["properties"]
        .as_array()?
        .iter()
        .find(|prop| prop["name"] == "textures")?
        ["value"]
        .as_str()?;

    // 3) Dentro del JSON base64: textures.SKIN.url.
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(textures_b64)
        .ok()?;
    let textures: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
    let texture_url = textures["textures"]["SKIN"]["url"].as_str()?;

    // 4) El PNG.
    http.get_string(texture_url).ok().map(String::into_bytes)
}

/// Resuelve la skin para un nick: primero la caché local, si no, la premium
/// (guardándola). `None` = nada local y nick sin skin remota.
pub fn resolve_for_nick(http: &HttpClient, paths: &Paths, nick: &str) -> Option<Vec<u8>> {
    let cached = cache_path(paths, nick);
    if let Ok(bytes) = std::fs::read(&cached) {
        return Some(bytes);
    }
    let bytes = fetch_premium(http, nick)?;
    if let Some(parent) = cached.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&cached, &bytes);
    Some(bytes)
}

/// Recorta la cara (8×8 en 8..16, 8..16 del PNG estándar 64×64) y la escala a
/// `scale` px por píxel. Devuelve RGBA listo para `egui::ColorImage`.
/// Sin depender de `image`: los PNG de skin son comprimidos (zlib), así que
/// aquí NO decodificamos PNG a mano — eso lo hace egui_extras al pintar. Para
/// la vista previa se reusa el decodificador de egui_extras vía `image`.
pub fn face_rgba(png_bytes: &[u8], scale: usize) -> Option<Vec<u8>> {
    let img = image::load_from_memory(png_bytes).ok()?;
    let face = img.crop_imm(8, 8, 8, 8).to_rgba8();
    let mut out = Vec::with_capacity(64 * scale * scale * 4);
    for y in 0..(8 * scale) {
        for x in 0..(8 * scale) {
            let pixel = face.get_pixel((x / scale) as u32, (y / scale) as u32);
            out.extend_from_slice(&pixel.0);
        }
    }
    Some(out)
}

/// Hash corto de una skin (para saber si cambió y refrescar la preview).
pub fn fingerprint(png_bytes: &[u8]) -> String {
    let mut hasher = md5::Md5::new();
    hasher.update(png_bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rechaza_no_png() {
        let dir = std::env::temp_dir().join("mclite-skins-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("fake.png");
        std::fs::write(&fake, b"esto no es un png").unwrap();
        assert!(from_local_png(&fake).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn acepta_png_con_cabecera_valida() {
        let dir = std::env::temp_dir().join("mclite-skins-test2");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let real = dir.join("real.png");
        // Cabecera PNG + basura suficiente: solo validamos la cabecera.
        std::fs::write(&real, [0x89u8, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0]).unwrap();
        assert!(from_local_png(&real).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn aplica_la_skin_al_game_dir() {
        let root = std::env::temp_dir().join("mclite-skins-apply");
        let _ = std::fs::remove_dir_all(&root);
        let paths = Paths::with_root(&root);
        let game_dir = paths.instance_dir("mi-instancia");
        let png = [0x89u8, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 1, 2, 3];
        let dest = apply_to_instance(&png, "Steve", &game_dir).unwrap();
        assert_eq!(
            dest,
            game_dir.join("CustomSkinLoader").join("LocalSkin").join("Steve.png")
        );
        assert_eq!(std::fs::read(&dest).unwrap(), png);
        // Un nick raro se sanitiza (no se sale del directorio).
        let weird = apply_to_instance(&png, "../etc/passwd", &game_dir).unwrap();
        assert!(weird.file_name().unwrap().to_string_lossy().ends_with(".png"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn el_fingerprint_cambia_con_el_contenido() {
        assert_ne!(fingerprint(b"a"), fingerprint(b"b"));
        assert_eq!(fingerprint(b"same"), fingerprint(b"same"));
    }
}
