//! Instalación de modpacks de CurseForge desde un fichero local (drag & drop).
//!
//! Un pack de CurseForge es un zip con `manifest.json` (versión de MC, cargador y
//! lista de `projectID`/`fileID`) y una carpeta de overrides. A diferencia del
//! `.mrpack` de Modrinth, NO trae las URLs de descarga: hay que resolver cada
//! fichero, y se hace en dos pasos SIN clave:
//!
//! * cfwidget.com da la ficha del proyecto: título y categoría (para saber si el
//!   fichero va a `mods/`, `shaderpacks/` o `resourcepacks/`).
//! * el endpoint de descarga de CurseForge (`/api/v1/mods/{p}/files/{f}/download`)
//!   redirige (307) al CDN final; el cliente HTTP sigue redirects y un HEAD previo
//!   deja resuelto nombre y tamaño sin bajar el fichero dos veces.
//!
//! 1. Instalar el juego base (loader + MC del manifest) con el flujo normal.
//! 2. Resolver cada (projectID, fileID) → URL del CDN, y bajarlo a su carpeta.
//! 3. Volcar la carpeta de overrides del manifest sobre el gameDir.
//!
//! Sin clave no hay hash SHA-1 que verificar (no se publican): la integridad es la
//! del CDN de CurseForge.

use serde::Deserialize;
use std::path::Path;

use crate::core::error::{Error, Result};
use crate::core::http::{Download, HttpClient, download_all};
use crate::core::paths::Paths;
use crate::core::progress::Progress;
use crate::loaders::LoaderKind;

const WIDGET_API: &str = "https://api.cfwidget.com";

// ── manifest.json ────────────────────────────────────────────────────────────

/// `manifest.json` de un pack de CurseForge (manifestVersion 1).
#[derive(Debug, Deserialize)]
pub struct CfManifest {
    #[serde(default)]
    pub manifest_type: String,
    #[serde(default)]
    pub name: String,
    pub minecraft: CfMinecraft,
    #[serde(default)]
    pub files: Vec<CfFileRef>,
    /// Carpeta del zip con la configuración (`overrides` en los packs normales).
    #[serde(default = "default_overrides")]
    pub overrides: String,
}

fn default_overrides() -> String {
    "overrides".into()
}

#[derive(Debug, Deserialize)]
pub struct CfMinecraft {
    #[serde(default)]
    pub version: String,
    #[serde(rename = "modLoaders", default)]
    pub mod_loaders: Vec<CfModLoader>,
}

#[derive(Debug, Deserialize)]
pub struct CfModLoader {
    /// `forge-47.2.0`, `fabric-0.14.21`, `quilt-0.20.0-beta.4`…
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub primary: bool,
}

/// Referencia a un fichero del pack: solo ids; la URL se resuelve con cfwidget.
#[derive(Debug, Clone, Deserialize)]
pub struct CfFileRef {
    #[serde(rename = "projectID")]
    pub project_id: u64,
    #[serde(rename = "fileID")]
    pub file_id: u64,
    #[serde(default)]
    pub required: bool,
}

impl CfManifest {
    /// Cargador del pack: `("forge", "47.2.0")`, `("fabric", "0.14.21")`…
    pub fn loader(&self) -> Option<(LoaderKind, String)> {
        let loader = self
            .minecraft
            .mod_loaders
            .iter()
            .find(|loader| loader.primary)
            .or_else(|| self.minecraft.mod_loaders.first())?;
        let (kind, version) = loader.id.split_once('-')?;
        Some((LoaderKind::parse(kind)?, version.to_string()))
    }
}

/// ¿Es este zip un pack de CurseForge? Un backup de McLite usa otro nombre de
/// manifiesto, así que no hay colisión: basta con encontrar `manifest.json` y
/// que sea del tipo correcto.
pub fn is_curseforge_zip(zip_path: &Path) -> bool {
    read_manifest(zip_path)
        .map(|manifest| {
            manifest.manifest_type.is_empty() || manifest.manifest_type == "minecraftModpack"
        })
        .unwrap_or(false)
}

/// Lee el `manifest.json` del zip del pack.
pub fn read_manifest(zip_path: &Path) -> Result<CfManifest> {
    let file = std::fs::File::open(zip_path).map_err(|e| Error::io(zip_path, e))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| Error::Zip { path: zip_path.into(), reason: e.to_string() })?;
    let mut entry = archive
        .by_name("manifest.json")
        .map_err(|e| Error::Zip { path: zip_path.into(), reason: format!("manifest.json: {e}") })?;
    let mut raw = String::new();
    use std::io::Read as _;
    entry.read_to_string(&mut raw).map_err(|e| Error::io(zip_path, e))?;
    serde_json::from_str(&raw).map_err(Error::from)
}

// ── cfwidget: projectID/fileID → URL ────────────────────────────────────────

/// Respuesta de cfwidget para un proyecto. Solo nos interesan el título y la
/// URL de la página (para saber la categoría): el resto del JSON (description,
/// files[], download…) se ignora al deserializar.
#[derive(Debug, Deserialize)]
struct WidgetProject {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    urls: WidgetUrls,
}

#[derive(Debug, Deserialize, Default)]
struct WidgetUrls {
    #[serde(default)]
    curseforge: Option<String>,
}

/// Un (projectID, fileID) resuelto: URL final del CDN + nombre de fichero.
#[derive(Debug, Clone)]
pub struct ResolvedFile {
    pub url: String,
    pub file_name: String,
    pub title: String,
    /// Carpeta destino dentro del gameDir (`mods`, `shaderpacks`, `resourcepacks`).
    pub dest_dir: &'static str,
}

/// Carpeta destino según la categoría que se ve en la URL del proyecto
/// (`…/minecraft/mc-mods/jei` → mods, `…/minecraft/shaders/…` → shaderpacks…).
/// El manifest no dice dónde va cada fichero; la categoría es lo más fiable.
fn dest_dir_por_url(url_proyecto: &str) -> &'static str {
    let lower = url_proyecto.to_lowercase();
    if lower.contains("/shaders/") {
        "shaderpacks"
    } else if lower.contains("/texture-packs/") {
        "resourcepacks"
    } else {
        "mods"
    }
}

/// Nombre de fichero a partir de la URL directa (último segmento).
fn file_name_de_url(url: &str) -> Option<String> {
    url.rsplit('/').next().filter(|s| !s.is_empty()).map(String::from)
}

/// Resuelve un (projectID, fileID) a URL final del CDN y nombre de fichero:
///
/// 1. cfwidget da la ficha del proyecto (título, categoría). Si está en cola
///    (`code: "queued"`, primera consulta a un proyecto poco popular), reintenta.
/// 2. HEAD al endpoint de descarga de CurseForge (sin clave) que redirige al CDN:
///    la URL final es la de descarga y su último segmento, el nombre real.
fn resolve_file(http: &HttpClient, project_id: u64, file_id: u64) -> Result<ResolvedFile> {
    // Ficha del proyecto: título y categoría.
    let url_api = format!("{WIDGET_API}/{project_id}");
    let mut en_cola = 0;
    let (title, dest_dir) = loop {
        let raw = http.get_string(&url_api)?;
        let project: WidgetProject = serde_json::from_str(&raw)
            .map_err(|e| Error::Unsupported(format!("cfwidget respondió algo raro: {e}")))?;
        if project.code.as_deref() == Some("queued") {
            en_cola += 1;
            if en_cola > 2 {
                return Err(Error::Unsupported(format!(
                    "cfwidget aún no tiene en caché el proyecto {project_id}: reintenta en unos segundos"
                )));
            }
            std::thread::sleep(std::time::Duration::from_millis(1500));
            continue;
        }
        break (
            project.title.unwrap_or_else(|| project_id.to_string()),
            dest_dir_por_url(project.urls.curseforge.as_deref().unwrap_or("")),
        );
    };

    // URL final del CDN (el endpoint oficial redirige 307 sin clave) y nombre real
    // del fichero (último segmento de esa URL).
    let url_descarga = format!("https://www.curseforge.com/api/v1/mods/{project_id}/files/{file_id}/download");
    let (url, _) = http.probe(&url_descarga)?;
    if url.starts_with(&url_descarga[..url_descarga.len().min(url.len())]) {
        return Err(Error::Unsupported(format!(
            "CurseForge no resolvió la descarga del proyecto {project_id} (fichero {file_id})"
        )));
    }
    Ok(ResolvedFile {
        dest_dir,
        title,
        file_name: file_name_de_url(&url).unwrap_or_else(|| format!("{file_id}.jar")),
        url,
    })
}

// ── Instalación ──────────────────────────────────────────────────────────────

/// Resuelve y baja todos los ficheros del manifest al gameDir y vuelca los
/// overrides. Devuelve cuántos ficheros se bajaron.
pub fn install(
    http: &HttpClient,
    _paths: &Paths,
    pack: &Path,
    game_dir: &Path,
    threads: usize,
    progress: &Progress,
) -> Result<usize> {
    let manifest = read_manifest(pack)?;
    if manifest.files.is_empty() {
        return Err(Error::Missing("ficheros en manifest.json".into()));
    }

    // 1) Resolver las URLs (una consulta a cfwidget por proyecto).
    progress.phase("Resolviendo los mods del pack (cfwidget)");
    let total = manifest.files.len();
    let mut jobs: Vec<Download> = Vec::new();
    for (i, reference) in manifest.files.iter().enumerate() {
        let resolved = resolve_file(http, reference.project_id, reference.file_id)?;
        progress.message(format!(
            "{}/{}: {} → {}",
            i + 1,
            total,
            resolved.title,
            resolved.file_name
        ));
        let dest = game_dir.join(resolved.dest_dir).join(&resolved.file_name);
        jobs.push(Download::new(resolved.url, dest));
    }
    if jobs.is_empty() {
        return Err(Error::Missing("nada descargable en manifest.json".into()));
    }

    // 2) Bajar todo en paralelo.
    progress.phase("Descargando mods del pack");
    let downloaded = download_all(http, jobs, threads, progress)?;

    // 3) Overrides del pack sobre el gameDir.
    progress.phase("Volcando overrides del pack");
    let count = extract_overrides(pack, &manifest.overrides, game_dir)?;
    progress.message(format!("overrides aplicados: {count} ficheros"));
    Ok(downloaded)
}

/// Extrae la carpeta de overrides del zip sobre `game_dir` (igual que en mrpack,
/// pero el nombre de la carpeta lo dice el manifest). Devuelve ficheros escritos.
pub fn extract_overrides(pack: &Path, overrides: &str, game_dir: &Path) -> Result<usize> {
    let file = std::fs::File::open(pack).map_err(|e| Error::io(pack, e))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| Error::Zip { path: pack.into(), reason: e.to_string() })?;

    let mut written = 0usize;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|e| {
            Error::Zip { path: pack.into(), reason: format!("entrada {index}: {e}") }
        })?;
        let Some(rel) = entry.enclosed_name() else {
            continue; // ruta rara: ignorar, nunca salir del destino
        };
        let Some(first) = rel.iter().next().map(|f| f.to_string_lossy().into_owned()) else {
            continue;
        };
        if first != overrides {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsea_un_manifest_real() {
        let raw = r#"{
            "manifestType": "minecraftModpack",
            "manifestVersion": 1,
            "name": "El pack de prueba",
            "version": "1.2",
            "minecraft": {
                "version": "1.20.1",
                "modLoaders": [{ "id": "forge-47.2.0", "primary": true }]
            },
            "files": [
                { "projectID": 238222, "fileID": 4612345, "required": true }
            ],
            "overrides": "overrides"
        }"#;
        let manifest: CfManifest = serde_json::from_str(raw).unwrap();
        assert_eq!(manifest.name, "El pack de prueba");
        assert_eq!(manifest.minecraft.version, "1.20.1");
        assert_eq!(manifest.files.len(), 1);
        assert_eq!(manifest.files[0].project_id, 238222);
        assert_eq!(manifest.overrides, "overrides");
        let (kind, version) = manifest.loader().unwrap();
        assert_eq!(kind, LoaderKind::Forge);
        assert_eq!(version, "47.2.0");
    }

    #[test]
    fn parsea_cargadores_varios() {
        let loader = |id: &str| -> (LoaderKind, String) {
            let raw = format!(
                r#"{{"minecraft":{{"version":"1.20.1","modLoaders":[{{"id":"{id}","primary":true}}]}}}}"#
            );
            let manifest: CfManifest = serde_json::from_str(&raw).unwrap();
            manifest.loader().unwrap()
        };
        assert_eq!(loader("fabric-0.14.21"), (LoaderKind::Fabric, "0.14.21".into()));
        assert_eq!(loader("quilt-0.20.0-beta.4"), (LoaderKind::Quilt, "0.20.0-beta.4".into()));
        assert_eq!(loader("neoforge-20.4.237"), (LoaderKind::NeoForge, "20.4.237".into()));
        // Un cargador desconocido no se resuelve.
        let raw = r#"{"minecraft":{"version":"1.20.1","modLoaders":[{"id":"raro-1.0","primary":true}]}}"#;
        let manifest: CfManifest = serde_json::from_str(raw).unwrap();
        assert!(manifest.loader().is_none());
    }

    #[test]
    fn la_categoria_de_la_url_dice_el_destino() {
        assert_eq!(dest_dir_por_url("https://www.curseforge.com/minecraft/mc-mods/jei"), "mods");
        assert_eq!(
            dest_dir_por_url("https://www.curseforge.com/minecraft/shaders/complementary"),
            "shaderpacks"
        );
        assert_eq!(
            dest_dir_por_url("https://www.curseforge.com/minecraft/texture-packs/algun-pack"),
            "resourcepacks"
        );
        // Sin URL conocida: mods, que es lo que son casi todos.
        assert_eq!(dest_dir_por_url(""), "mods");
    }

    #[test]
    fn el_nombre_de_fichero_sale_de_la_url() {
        assert_eq!(
            file_name_de_url("https://mediafilez.forgecdn.net/files/4612/345/jei-1.20.1-forge.jar"),
            Some("jei-1.20.1-forge.jar".into())
        );
        assert_eq!(file_name_de_url("https://mediafilez.forgecdn.net/"), None);
    }

    #[test]
    fn la_ficha_del_widget_da_categoria_y_titulo() {
        // Forma real de https://api.cfwidget.com/238222 (los files[] traen `name`,
        // sin URL de descarga: esa la da el endpoint oficial con el redirect).
        let raw = r#"{
            "id": 238222,
            "title": "Just Enough Items (JEI)",
            "urls": { "curseforge": "https://www.curseforge.com/minecraft/mc-mods/jei" }
        }"#;
        let project: WidgetProject = serde_json::from_str(raw).unwrap();
        assert_eq!(project.title.as_deref(), Some("Just Enough Items (JEI)"));
        assert_eq!(
            dest_dir_por_url(project.urls.curseforge.as_deref().unwrap_or("")),
            "mods"
        );
        // Y el nombre de fichero del redirect del CDN:
        assert_eq!(
            file_name_de_url("https://mediafilez.forgecdn.net/files/4612/345/jei-1.20.1-forge.jar"),
            Some("jei-1.20.1-forge.jar".into())
        );
    }
}
