//! Modrinth: búsqueda de modpacks, versiones e instalación de `.mrpack`.
//!
//! Un `.mrpack` es un zip con dos cosas: `modrinth.index.json` (la lista de mods
//! con sus URLs y hashes, más el loader y la versión de MC que necesita) y
//! `overrides/` (config, options.txt, mods locales… que se vuelca sobre el
//! gameDir). El flujo de instalación:
//!
//! 1. Instalar el juego base (loader + MC del índice) con el flujo normal.
//! 2. Descargar cada fichero del índice a su ruta dentro del gameDir.
//! 3. Volcar `overrides/` sobre el gameDir.
//!
//! API v2 verificada 2026-09: `search` (facets `project_type:modpack`),
//! `project/{slug}/version` y los ficheros del índice con `downloads[]`
//! (primera URL) + `hashes.sha1`.

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::core::error::{Error, Result};
use crate::core::http::{Download, HttpClient, download_all};
use crate::core::paths::Paths;
use crate::core::progress::Progress;
use crate::loaders::LoaderKind;

pub const API: &str = "https://api.modrinth.com";
/// La CLI de modrinth pide identificar el cliente: formato `repo/version`.
const USER_AGENT: &str = concat!("mclite/", env!("CARGO_PKG_VERSION"));

// ── Búsqueda ─────────────────────────────────────────────────────────────────

/// Un resultado de búsqueda (proyecto tipo modpack).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackHit {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub downloads: u64,
    /// La última versión está cubierta por estas versiones de MC.
    #[serde(default)]
    pub versions: Vec<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub icon_url: Option<String>,
    /// Solo en la búsqueda (el detalle no lo trae).
    #[serde(default)]
    pub author: Option<String>,
}

/// Detalle completo de un pack (GET /v2/project/{slug}).
///
/// Ojo, el detalle NO es idéntico al hit de búsqueda: la id va como `id` (no
/// `project_id`), las versiones de MC como `game_versions`, y NO trae autor
/// (ese solo viene en la búsqueda; por eso `author()` admite fallback).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackDetail {
    #[serde(alias = "id", default)]
    pub project_id: String,
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// Descripción larga en Markdown.
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub followers: u64,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub icon_url: Option<String>,
    // Solo game_versions: el JSON trae TAMBIÉN `versions` (ids de versiones del
    // pack), y un alias a otro nombre distinto haría que serde los tratara como
    // duplicados del mismo campo.
    #[serde(rename = "game_versions", default)]
    pub versions: Vec<String>,
    #[serde(default)]
    pub published: Option<String>,
    #[serde(default)]
    pub license: Option<serde_json::Value>,
    /// En algunos endpoints viene como `user.username`.
    #[serde(default)]
    pub user: Option<PackAuthor>,
    #[serde(default)]
    pub author: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackAuthor {
    #[serde(default)]
    pub username: String,
}

impl PackDetail {
    pub fn author(&self) -> &str {
        self.user
            .as_ref()
            .map(|user| user.username.as_str())
            .or(self.author.as_deref())
            .unwrap_or("desconocido")
    }

    /// Versión de MC más reciente del pack. La lista va en orden ASCENDENTE
    /// (verificado contra la API), así que la más nueva es la ÚLTIMA.
    pub fn newest_mc(&self) -> Option<&String> {
        self.versions.last()
    }
}

/// Detalle completo de un pack.
pub fn detail(http: &HttpClient, slug: &str) -> Result<PackDetail> {
    let url = format!("{API}/v2/project/{slug}");
    http.get_json_ua(&url, USER_AGENT)
        .map_err(|e| Error::Http(format!("detalle de {slug}: {e}")))
}

/// Busca modpacks por texto (vacío = populares).
pub fn search(http: &HttpClient, query: &str, limit: usize) -> Result<Vec<PackHit>> {
    #[derive(Deserialize)]
    struct SearchResponse {
        hits: Vec<PackHit>,
    }
    // Facets: [[...]] es OR interno, [[..],[..]] es AND entre listas.
    // Los DOS van urlencoded: el JSON trae corchetes y comillas, que en una URI
    // sin codificar hacen que ureq rechace la petición (invalid uri character).
    let facets = urlencode(&serde_json::json!([[ "project_type:modpack" ]]).to_string());
    let url = format!(
        "{API}/v2/search?limit={limit}&query={query}&facets={facets}",
        query = urlencode(query),
    );
    let response: SearchResponse = http.get_json_ua(&url, USER_AGENT)?;
    Ok(response.hits)
}

/// Versiones (files) de un modpack, más nuevas primero.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackVersion {
    pub id: String,
    pub name: String,
    pub version_number: String,
    #[serde(default)]
    pub game_versions: Vec<String>,
    #[serde(default)]
    pub loaders: Vec<String>,
    #[serde(default)]
    pub files: Vec<VersionFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionFile {
    pub url: String,
    pub filename: String,
    #[serde(default)]
    pub primary: bool,
    #[serde(default)]
    pub size: u64,
}

pub fn versions(http: &HttpClient, slug: &str) -> Result<Vec<PackVersion>> {
    let url = format!("{API}/v2/project/{slug}/version");
    let list: Vec<PackVersion> = http.get_json_ua(&url, USER_AGENT)?;
    Ok(list)
}

// ── Índice del .mrpack ───────────────────────────────────────────────────────

/// `modrinth.index.json`.
#[derive(Debug, Deserialize)]
pub struct MrpackIndex {
    #[serde(default)]
    pub format_version: u32,
    pub game: String,
    #[serde(default)]
    pub version_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub summary: Option<String>,
    pub files: Vec<IndexFile>,
    #[serde(default)]
    pub dependencies: MrpackDependencies,
}

#[derive(Debug, Default, Deserialize)]
pub struct MrpackDependencies {
    #[serde(default)]
    pub minecraft: Option<String>,
    #[serde(rename = "fabric-loader", default)]
    pub fabric_loader: Option<String>,
    #[serde(rename = "quilt-loader", default)]
    pub quilt_loader: Option<String>,
    #[serde(rename = "forge", default)]
    pub forge: Option<String>,
    #[serde(rename = "neoforge", default)]
    pub neoforge: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct IndexFile {
    pub path: String,
    #[serde(default)]
    pub hashes: Option<IndexHashes>,
    #[serde(default)]
    pub env: Option<IndexEnv>,
    pub downloads: Vec<String>,
    #[serde(rename = "fileSize", default)]
    pub file_size: u64,
}

#[derive(Debug, Deserialize)]
pub struct IndexHashes {
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub sha512: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct IndexEnv {
    #[serde(default)]
    pub client: Option<String>,
    #[serde(default)]
    pub server: Option<String>,
}

impl MrpackIndex {
    /// Loader que pide el pack. Los packs de OptiFine no existen; el resto es
    /// fabric/quilt/forge/neoforge o vanilla.
    pub fn loader_kind(&self) -> LoaderKind {
        let deps = &self.dependencies;
        if deps.fabric_loader.is_some() {
            LoaderKind::Fabric
        } else if deps.quilt_loader.is_some() {
            LoaderKind::Quilt
        } else if deps.forge.is_some() {
            LoaderKind::Forge
        } else if deps.neoforge.is_some() {
            LoaderKind::NeoForge
        } else {
            LoaderKind::Vanilla
        }
    }

    /// Versión del cargador que pide el pack (para `PlayRequest.loader_version`).
    pub fn loader_version(&self) -> Option<String> {
        let deps = &self.dependencies;
        deps.fabric_loader
            .clone()
            .or_else(|| deps.quilt_loader.clone())
            .or_else(|| deps.forge.clone())
            .or_else(|| deps.neoforge.clone())
    }

    /// Versión de Minecraft que pide el pack.
    pub fn mc_version(&self) -> Option<String> {
        self.dependencies.minecraft.clone()
    }

    /// Descargas de mods que aplican al cliente (env omitido = aplicar).
    pub fn client_files(&self) -> Vec<&IndexFile> {
        self.files
            .iter()
            .filter(|file| {
                match &file.env {
                    None => true,
                    Some(env) => env.client.as_deref() != Some("unsupported"),
                }
            })
            .collect()
    }
}

/// Lee el `modrinth.index.json` de un `.mrpack` ya descargado.
pub fn read_index(mrpack: &Path) -> Result<MrpackIndex> {
    let file = std::fs::File::open(mrpack).map_err(|e| Error::io(mrpack, e))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| Error::Zip { path: mrpack.into(), reason: e.to_string() })?;
    let mut entry = archive
        .by_name("modrinth.index.json")
        .map_err(|e| Error::Zip { path: mrpack.into(), reason: format!("modrinth.index.json: {e}") })?;
    let mut raw = String::new();
    use std::io::Read as _;
    entry.read_to_string(&mut raw).map_err(|e| Error::io(mrpack, e))?;
    serde_json::from_str(&raw).map_err(Error::from)
}

/// Descarga los mods del índice al gameDir y vuelca `overrides/`.
/// Devuelve cuántos ficheros se bajaron.
pub fn install(
    http: &HttpClient,
    _paths: &Paths,
    mrpack: &Path,
    game_dir: &Path,
    threads: usize,
    progress: &Progress,
) -> Result<usize> {
    let index = read_index(mrpack)?;
    if index.game != "minecraft" {
        return Err(Error::Unsupported(format!(
            "el pack es para «{}», no para Minecraft",
            index.game
        )));
    }

    // 1) Mods del índice → dentro del gameDir (mods/, shaderpacks/, …).
    let jobs: Vec<Download> = index
        .client_files()
        .iter()
        .filter_map(|file| {
            let rel = safe_rel_path(&file.path)?;
            let url = file.downloads.first()?.clone();
            let mut job = Download::new(url, game_dir.join(rel)).with_size(file.file_size);
            if let Some(sha1) = file.hashes.as_ref().and_then(|h| h.sha1.clone()) {
                job = job.with_sha1(sha1);
            }
            Some(job)
        })
        .collect();
    if jobs.is_empty() {
        return Err(Error::Missing("ficheros descargables en modrinth.index.json".into()));
    }
    progress.phase("Descargando mods del pack");
    let pending = download_all(http, jobs, threads, progress)?;

    // 2) Overrides → se vuelcan sobre el gameDir (config, options, mods locales…).
    progress.phase("Volcando overrides del pack");
    let count = extract_overrides(mrpack, game_dir)?;
    progress.message(format!("overrides aplicados: {count} ficheros"));
    Ok(pending)
}

/// Extrae `overrides/` (y `client-overrides/`, presente en packs recientes)
/// del mrpack sobre `game_dir`. Devuelve cuántos ficheros se escribieron.
pub fn extract_overrides(mrpack: &Path, game_dir: &Path) -> Result<usize> {
    let file = std::fs::File::open(mrpack).map_err(|e| Error::io(mrpack, e))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| Error::Zip { path: mrpack.into(), reason: e.to_string() })?;

    let mut written = 0usize;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|e| {
            Error::Zip { path: mrpack.into(), reason: format!("entrada {index}: {e}") }
        })?;
        let Some(rel) = entry.enclosed_name() else {
            continue; // ruta rara: ignorar, nunca salir del destino
        };
        let Some(sub) = rel.iter().next().map(|first| first.to_string_lossy().into_owned())
        else {
            continue;
        };
        if sub != "overrides" && sub != "client-overrides" {
            continue;
        }
        let rest = rel.iter().skip(1).collect::<std::path::PathBuf>();
        if rest.as_os_str().is_empty() {
            continue; // la carpeta raíz misma
        }
        let dest = game_dir.join(&rest);
        if entry.is_dir() {
            std::fs::create_dir_all(&dest).map_err(|e| Error::io(&dest, e))?;
            continue;
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        let mut out = std::fs::File::create(&dest).map_err(|e| Error::io(&dest, e))?;
        std::io::copy(&mut entry, &mut out).map_err(|e| Error::io(&dest, e))?;
        written += 1;
    }
    Ok(written)
}

/// Ruta relativa segura dentro del gameDir (bloquea `../` y absolutas).
fn safe_rel_path(rel: &str) -> Option<std::path::PathBuf> {
    let path = Path::new(rel);
    if path.is_absolute() {
        return None;
    }
    let mut out = std::path::PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::Normal(piece) => out.push(piece),
            _ => return None,
        }
    }
    if out.as_os_str().is_empty() {
        None
    } else {
        Some(out)
    }
}

pub(crate) fn urlencode(raw: &str) -> String {
    let mut out = String::new();
    for byte in raw.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_query() {
        assert_eq!(urlencode("fabulously optimized"), "fabulously%20optimized");
        assert_eq!(urlencode("a+b"), "a%2Bb");
    }

    #[test]
    fn rutas_seguras_del_indice() {
        assert!(safe_rel_path("mods/a.jar").is_some());
        assert_eq!(safe_rel_path("../escape"), None);
        assert_eq!(safe_rel_path("/abs"), None);
    }

    #[test]
    fn parsea_el_indice_real() {
        let raw = r#"{
            "formatVersion": 1,
            "game": "minecraft",
            "versionId": "15.0.0-alpha.3",
            "name": "Fabulously Optimized",
            "files": [
                { "path": "mods/BetterGrassify.jar",
                  "hashes": { "sha1": "626f74d3265d140f80ebb5ca1703f43ba07ff1bd", "sha512": "aa" },
                  "env": { "client": "required", "server": "required" },
                  "downloads": ["https://cdn.modrinth.com/x.jar"],
                  "fileSize": 89214 },
                { "path": "mods/solo-servidor.jar",
                  "env": { "client": "unsupported", "server": "required" },
                  "downloads": ["https://cdn.modrinth.com/y.jar"],
                  "fileSize": 1 }
            ],
            "dependencies": { "fabric-loader": "0.19.5", "minecraft": "26.3" }
        }"#;
        let index: MrpackIndex = serde_json::from_str(raw).unwrap();
        assert_eq!(index.loader_kind(), LoaderKind::Fabric);
        assert_eq!(index.loader_version().as_deref(), Some("0.19.5"));
        assert_eq!(index.mc_version().as_deref(), Some("26.3"));
        // El solo-servidor no aplica al cliente.
        assert_eq!(index.client_files().len(), 1);
        assert_eq!(index.client_files()[0].file_size, 89214);
    }

    #[test]
    fn loader_por_dependencias() {
        let dep = |json: &str| -> MrpackIndex {
            serde_json::from_str(&format!(
                r#"{{ "game": "minecraft", "files": [], "dependencies": {json} }}"#
            ))
            .unwrap()
        };
        assert_eq!(dep(r#"{"forge":"47.2.0"}"#).loader_kind(), LoaderKind::Forge);
        assert_eq!(dep(r#"{"neoforge":"21.4.50"}"#).loader_kind(), LoaderKind::NeoForge);
        assert_eq!(dep(r#"{"quilt-loader":"0.27.1"}"#).loader_kind(), LoaderKind::Quilt);
        assert_eq!(dep("{}").loader_kind(), LoaderKind::Vanilla);
    }
}
