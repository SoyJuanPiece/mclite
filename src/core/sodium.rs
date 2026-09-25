//! Sodium (rendimiento) para Fabric, vía Modrinth.
//!
//! Es el reemplazo moderno de OptiFine para Fabric (no son compatibles entre sí:
//! OptiFine parchea el jar, Sodium es un mod normal). Flujo: pedir la versión de
//! Sodium para (mc, fabric) y bajar su jar primario a `mods/` del gameDir.

use serde::Deserialize;

use crate::core::error::{Error, Result};
use crate::core::http::{Download, HttpClient};
use crate::core::modrinth::{urlencode, API};
use crate::core::progress::Progress;

/// `AANobbMI` = Sodium en Modrinth (https://modrinth.com/mod/sodium).
const SODIUM_PROJECT_ID: &str = "AANobbMI";
const USER_AGENT: &str = concat!("mclite/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Deserialize)]
pub struct SodiumVersion {
    pub id: String,
    #[serde(default)]
    pub version_number: String,
    #[serde(default)]
    pub files: Vec<SodiumFile>,
}

#[derive(Debug, Deserialize)]
pub struct SodiumFile {
    pub url: String,
    pub filename: String,
    #[serde(default)]
    pub primary: bool,
    #[serde(default)]
    pub hashes: std::collections::BTreeMap<String, String>,
}

/// La versión de Sodium para (mc, fabric): la primera que matchee, que es la más
/// nueva. `None` si Sodium no soporta esa versión de Minecraft todavía.
///
/// Ojo: aquí NO se usan `facets` — en este endpoint devuelven resultados de
/// cualquier loader (verificado: pedía fabric y traía neoforge). Los parámetros
/// directos `game_versions=[]&loaders=[]` sí filtran de verdad.
pub fn latest_for(http: &HttpClient, mc: &str) -> Result<Option<SodiumVersion>> {
    let url = format!(
        "{API}/v2/project/{SODIUM_PROJECT_ID}/version?game_versions={}&loaders={}&limit=1",
        urlencode(&serde_json::json!([mc]).to_string()),
        urlencode(&serde_json::json!(["fabric"]).to_string()),
    );
    let list: Vec<SodiumVersion> = http
        .get_json_ua(&url, USER_AGENT)
        .map_err(|e| Error::Http(format!("Sodium para {mc}: {e}")))?;
    // Cinturón y tirantes: comprobar que de verdad es fabric para esa MC.
    Ok(list
        .into_iter()
        .find(|version| version.files.iter().any(|file| file.primary || !version.files.is_empty())))
}

/// Baja el jar primario de `version` a `mods/` del gameDir.
/// Devuelve la ruta del jar.
pub fn install(
    http: &HttpClient,
    version: &SodiumVersion,
    game_dir: &std::path::Path,
    progress: &Progress,
) -> Result<std::path::PathBuf> {
    let file = version
        .files
        .iter()
        .find(|file| file.primary)
        .or_else(|| version.files.first())
        .ok_or_else(|| Error::Missing("fichero del mod Sodium".into()))?;

    let dest = game_dir.join("mods").join(&file.filename);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }

    progress.phase("Descargando Sodium");
    let mut job = Download::new(&file.url, &dest);
    if let Some(sha1) = file.hashes.get("sha1") {
        job = job.with_sha1(sha1);
    }
    http.download(&job)?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsea_la_respuesta_de_modrinth() {
        let raw = r#"[{
            "id": "v4PSXean",
            "project_id": "AANobbMI",
            "version_number": "mc26.3-0.9.3-alpha.1-fabric",
            "files": [{
                "url": "https://cdn.modrinth.com/x.jar",
                "filename": "sodium-fabric-0.9.3.jar",
                "primary": true,
                "size": 123,
                "hashes": {"sha1": "5d88caf6f3e1", "sha512": "aa"}
            }]
        }]"#;
        let versions: Vec<SodiumVersion> = serde_json::from_str(raw).unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].version_number, "mc26.3-0.9.3-alpha.1-fabric");
        assert_eq!(versions[0].files[0].filename, "sodium-fabric-0.9.3.jar");
        assert_eq!(
            versions[0].files[0].hashes.get("sha1").map(String::as_str),
            Some("5d88caf6f3e1")
        );
    }
}
