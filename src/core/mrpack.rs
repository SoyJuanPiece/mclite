//! Instalación de modpacks `.mrpack` desde un fichero local (drag & drop).
//!
//! Un `.mrpack` es un zip con dos cosas: `modrinth.index.json` (la lista de mods
//! con sus URLs y hashes, más el loader y la versión de MC que necesita) y
//! `overrides/` (config, options.txt, mods locales… que se vuelca sobre el
//! gameDir). El flujo de instalación, igual para un pack arrastrado a la ventana:
//!
//! 1. Instalar el juego base (loader + MC del índice) con el flujo normal.
//! 2. Descargar cada fichero del índice a su ruta dentro del gameDir.
//! 3. Volcar `overrides/` sobre el gameDir.
//!
//! Formato del índice verificado 2026-09: ficheros con `downloads[]` (primera
//! URL) + `hashes.sha1` y `env.client` opcional.

use serde::Deserialize;
use std::path::Path;

use crate::core::error::{Error, Result};
use crate::core::http::{Download, HttpClient, download_all};
use crate::core::paths::Paths;
use crate::core::progress::Progress;
use crate::loaders::LoaderKind;

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

#[cfg(test)]
mod tests {
    use super::*;

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
