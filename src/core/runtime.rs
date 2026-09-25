//! Runtime de Java de Mojang (fase 3).
//!
//! El launcher oficial no usa el Java del sistema: baja runtimes `java-runtime-*`
//! publicados por Mojang. El formato es un catálogo global (`JAVA_RUNTIME_ALL`)
//! con una entrada por plataforma y componente; cada entrada apunta a un
//! manifiesto que lista los ficheros del runtime con su SHA-1 y tamaño.
//!
//! Aquí solo implementamos la mitad que hace falta: elegir el componente que
//! toca, descargar lo que falte a `runtime/<componente>/` y devolver la ruta del
//! binario `java`. Los ficheros ya descargados y con buen hash no se tocan, así
//! que la segunda vez es instantáneo.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::core::endpoints::{JAVA_RUNTIME_ALL, java_runtime_platform};
use crate::core::error::{Error, Result};
use crate::core::http::{Download, HttpClient, download_all};
use crate::core::java::java_binary_name;
use crate::core::paths::Paths;
use crate::core::progress::Progress;

/// Un runtime resuelto: binario y versión que lo identificó.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaRuntime {
    pub java: PathBuf,
    /// p. ej. `21.0.7` (tal cual lo publica Mojang).
    pub version: String,
    /// Componente del catálogo, p. ej. `java-runtime-delta`.
    pub component: String,
}

impl JavaRuntime {
    pub fn label(&self) -> String {
        format!("Java {} (runtime de Mojang, {})", self.version, self.component)
    }
}

// ── Catálogo ─────────────────────────────────────────────────────────────────

/// `all.json`: { plataforma: { componente: [entrada, …] } }.
/// Hay claves extra (`gamecore`, …) que no usamos: flatten las ignora por
/// ser mapas con la misma forma.
#[derive(Debug, Deserialize)]
pub struct RuntimeCatalog {
    #[serde(flatten)]
    pub platforms: BTreeMap<String, RuntimePlatform>,
}

#[derive(Debug, Deserialize)]
pub struct RuntimePlatform {
    #[serde(flatten)]
    pub components: BTreeMap<String, Vec<RuntimeEntry>>,
}

#[derive(Debug, Deserialize)]
pub struct RuntimeEntry {
    pub manifest: RuntimeLink,
    pub version: RuntimeVersion,
}

#[derive(Debug, Deserialize)]
pub struct RuntimeVersion {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct RuntimeLink {
    pub url: String,
    pub sha1: String,
    #[serde(default)]
    pub size: u64,
}

/// Primera entrada (la más reciente) de `plataforma/componente`.
pub fn first_entry<'a>(
    catalog: &'a RuntimeCatalog,
    platform: &str,
    component: &str,
) -> Result<&'a RuntimeEntry> {
    let components = catalog.platforms.get(platform).ok_or_else(|| {
        Error::Missing(format!("plataforma «{platform}» en el catálogo de runtimes de Java"))
    })?;
    let entries = components.components.get(component).ok_or_else(|| {
        Error::Missing(format!(
            "componente «{component}» para {platform} en el catálogo de runtimes de Java"
        ))
    })?;
    entries.first().ok_or_else(|| {
        Error::Missing(format!(
            "el componente «{component}» no tiene versiones para {platform}"
        ))
    })
}

// ── Manifiesto ───────────────────────────────────────────────────────────────

/// Manifiesto de un runtime: { files: { ruta: fichero } }.
#[derive(Debug, Deserialize)]
pub struct RuntimeManifest {
    pub files: BTreeMap<String, RuntimeFile>,
}

#[derive(Debug, Deserialize)]
pub struct RuntimeFile {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub executable: bool,
    #[serde(default)]
    pub downloads: RuntimeDownloads,
}

#[derive(Debug, Default, Deserialize)]
pub struct RuntimeDownloads {
    /// Los manifiestos traen también `lzma` (más pequeño, hay que descomprimir);
    /// nos quedamos con `raw`, que es directo y verificable por SHA-1.
    #[serde(default)]
    pub raw: Option<RuntimeRaw>,
}

#[derive(Debug, Deserialize)]
pub struct RuntimeRaw {
    pub url: String,
    pub sha1: String,
    #[serde(default)]
    pub size: u64,
}

/// Convierte la ruta del manifiesto en una ruta local segura.
/// `None` si intenta salir del directorio del runtime (`../`, absolutas…).
pub fn safe_rel_path(rel: &str) -> Option<PathBuf> {
    let path = Path::new(rel);
    if path.is_absolute() {
        return None;
    }
    let mut out = PathBuf::new();
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

/// Lista de descargas pendientes del runtime (sin mirar qué ya está en disco:
/// de eso se encarga `download_all`).
pub fn jobs_from(manifest: &RuntimeManifest, runtime_dir: &Path) -> Result<Vec<Download>> {
    let mut jobs = Vec::new();
    for (rel, file) in &manifest.files {
        if file.kind != "file" {
            continue;
        }
        let Some(raw) = &file.downloads.raw else {
            continue;
        };
        // Una ruta rara (../, absoluta) no debe abortar la instalación entera:
        // se ignora. Los manifiestos de Mojang no las traen.
        let Some(dest) = safe_rel_path(rel) else {
            continue;
        };
        jobs.push(
            Download::new(&raw.url, runtime_dir.join(dest))
                .with_sha1(&raw.sha1)
                .with_size(raw.size),
        );
    }
    if jobs.is_empty() {
        return Err(Error::Missing("ficheros en el manifiesto del runtime".to_string()));
    }
    Ok(jobs)
}

// ── Elección de componente ───────────────────────────────────────────────────

/// Componente del catálogo que mejor cubre `required_major`.
///
/// Contenido real del catálogo (verificado 2026-09): `jre-legacy` = Java 8,
/// `java-runtime-beta` = 17, `java-runtime-delta` = 21, `java-runtime-epsilon`
/// = 25 (lo usan las versiones que piden más de 21). alpha/gamma son 16/17 y
/// no se usan en versiones estables.
pub fn component_for(required_major: u32) -> &'static str {
    match required_major {
        ..=8 => "jre-legacy",
        9..=17 => "java-runtime-beta",
        18..=21 => "java-runtime-delta",
        _ => "java-runtime-epsilon",
    }
}

// ── Descarga ─────────────────────────────────────────────────────────────────

/// Asegura que el runtime `component` está en disco y devuelve su binario java.
/// Idempotente: si ya está descargado y con buen hash, no toca la red salvo
/// para el catálogo y el manifiesto (pocos KB).
pub fn ensure(
    http: &HttpClient,
    paths: &Paths,
    component: &str,
    threads: usize,
    progress: &Progress,
) -> Result<JavaRuntime> {
    let platform = java_runtime_platform().to_string();

    progress.phase("Buscando el runtime de Java de Mojang");
    let catalog: RuntimeCatalog = http.get_json(JAVA_RUNTIME_ALL)?;
    let entry = first_entry(&catalog, &platform, component)?;

    progress.phase(format!(
        "Descargando Java {} ({})",
        entry.version.name, component
    ));
    let manifest: RuntimeManifest = http.get_json(&entry.manifest.url)?;

    let runtime_dir = paths.runtime_component(component);
    let jobs = jobs_from(&manifest, &runtime_dir)?;
    let pending = download_all(http, jobs, threads, progress)?;
    if pending > 0 {
        progress.message(format!(
            "runtime de Java listo: {} ficheros nuevos ({} {})",
            pending,
            entry.version.name,
            component
        ));
    }

    // En Unix hay que marcar el bit de ejecución; el manifiesto dice cuáles.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for (rel, file) in &manifest.files {
            if !file.executable || file.kind != "file" {
                continue;
            }
            if let Some(dest) = safe_rel_path(rel) {
                let path = runtime_dir.join(dest);
                if let Ok(meta) = std::fs::metadata(&path) {
                    let mut permissions = meta.permissions();
                    permissions.set_mode(0o755);
                    let _ = std::fs::set_permissions(&path, permissions);
                }
            }
        }
    }

    let java = runtime_dir.join("bin").join(java_binary_name());
    if !java.is_file() {
        return Err(Error::Missing(format!(
            "el runtime {} se descargó pero no está {}",
            component,
            java.display()
        )));
    }

    Ok(JavaRuntime {
        java,
        version: entry.version.name.clone(),
        component: component.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Recorte del `all.json` real (2026-09): forma idéntica, datos mínimos.
    const CATALOG: &str = r#"{
        "gamecore": { "java-runtime-delta": [] },
        "linux": { "java-runtime-delta": [ { "availability": { "group": 7419, "progress": 100 },
            "manifest": { "sha1": "06a3884df3d9d4072b9b6df541495d81878ad2de", "size": 82843,
                          "url": "https://piston-meta.mojang.com/v1/packages/06a3/manifest.json" },
            "version": { "name": "21.0.7", "released": "2025-05-19T08:30:12+00:00" } } ] }
    }"#;

    /// Recorte de un manifiesto real de `java-runtime-delta`.
    const MANIFEST: &str = r#"{
        "files": {
            "bin": { "type": "directory" },
            "bin/java": { "type": "file", "executable": true, "downloads": { "raw": {
                "sha1": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "size": 123,
                "url": "https://piston-data.mojang.com/v1/objects/aaaa/bin/java" } } },
            "lib/jvm.cfg": { "type": "file", "executable": false, "downloads": { "raw": {
                "sha1": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", "size": 456,
                "url": "https://piston-data.mojang.com/v1/objects/bbbb/lib/jvm.cfg" } } },
            "lib/solo-lzma": { "type": "file", "executable": false, "downloads": { "lzma": {
                "sha1": "cccccccccccccccccccccccccccccccccccccccc", "size": 7,
                "url": "https://piston-data.mojang.com/v1/objects/cccc/lib/solo-lzma" } } },
            "../escape": { "type": "file", "downloads": { "raw": {
                "sha1": "dddddddddddddddddddddddddddddddddddddddd", "size": 1,
                "url": "https://piston-data.mojang.com/v1/objects/dddd/escape" } } }
        }
    }"#;

    #[test]
    fn lee_el_catalogo_y_elige_la_primera_entrada() {
        let catalog: RuntimeCatalog = serde_json::from_str(CATALOG).unwrap();
        let entry = first_entry(&catalog, "linux", "java-runtime-delta").unwrap();
        assert_eq!(entry.version.name, "21.0.7");
        assert_eq!(entry.manifest.sha1, "06a3884df3d9d4072b9b6df541495d81878ad2de");

        assert!(first_entry(&catalog, "linux", "jre-legacy").is_err());
        assert!(first_entry(&catalog, "solaris", "java-runtime-delta").is_err());
    }

    #[test]
    fn arma_jobs_solo_con_raw_y_fuera_directorios() {
        let manifest: RuntimeManifest = serde_json::from_str(MANIFEST).unwrap();
        let jobs = jobs_from(&manifest, Path::new("/rt")).unwrap();

        assert_eq!(jobs.len(), 2, "directorio, solo-lzma y escape no cuentan");
        assert_eq!(jobs[0].dest, PathBuf::from("/rt/bin/java"));
        assert_eq!(jobs[0].sha1.as_deref(), Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
        assert_eq!(jobs[0].size, Some(123));
    }

    #[test]
    fn el_manifiesto_no_sale_de_su_carpeta() {
        let manifest: RuntimeManifest = serde_json::from_str(MANIFEST).unwrap();
        let dir = Path::new("/rt");
        // jobs_from ya filtra el `../escape` devolviendo error; aquí probamos el helper.
        assert!(safe_rel_path("bin/java").is_some());
        assert_eq!(safe_rel_path("../escape"), None);
        assert_eq!(safe_rel_path("/absoluta"), None);
        assert_eq!(safe_rel_path(""), None);
        let _ = (&manifest, dir);
    }

    #[test]
    fn componente_segun_java_pedido() {
        assert_eq!(component_for(8), "jre-legacy");
        assert_eq!(component_for(7), "jre-legacy");
        assert_eq!(component_for(16), "java-runtime-beta");
        assert_eq!(component_for(17), "java-runtime-beta");
        assert_eq!(component_for(21), "java-runtime-delta");
        // Java 25 es epsilon (delta es 21: bajarlo para un Java 25 sería
        // el mismo UnsupportedClassVersionError que venimos a arreglar).
        assert_eq!(component_for(22), "java-runtime-epsilon");
        assert_eq!(component_for(25), "java-runtime-epsilon");
    }
}
