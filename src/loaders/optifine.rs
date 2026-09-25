//! OptiFine.
//!
//! No es un cargador: el jar que publica OptiFine es **instalador y parche** a la
//! vez. El plan (§2.1) documenta el flujo completo; este módulo lo implementa sin
//! ejecutar nada con GUI:
//!
//! 1. Listar y bajar desde BMCLAPI (mirror que permite automatizar; fallback manual
//!    en la UI si un día deja de responder).
//! 2. Ejecutar el `Patcher` que lleva dentro: `java -cp installer.jar optifine.Patcher
//!    <client.jar> <installer.jar> <salida.jar>` (OptiFine clásico, con
//!    `optifine/Patcher.class`). El OptiFine moderno (1.17+) **no** trae `Patcher.class`:
//!    el jar del instalador se usa tal cual como librería.
//! 3. Generar el manifiesto: mainClass `net.minecraft.launchwrapper.Launch`,
//!    `--tweakClass optifine.OptiFineTweaker`, librería OptiFine + launchwrapper.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::core::endpoints;
use crate::core::error::{Error, Result};
use crate::core::http::HttpClient;
use crate::core::install::{self, InstallOptions};
use crate::core::paths::Paths;
use crate::core::libraries::{Library, MavenCoord};
use crate::core::process::hidden_command;
use crate::core::version_json::{ArgValue, Arguments, VersionJson};
use crate::loaders::{Loader, LoaderCtx, LoaderKind, LoaderVersion};

pub struct OptiFineLoader;

#[derive(Debug, Deserialize)]
struct BmclEntry {
    #[serde(rename = "type")]
    kind: String,
    patch: String,
}

impl OptiFineLoader {
    /// `https://bmclapi2.bangbang93.com/optifine/1.21.4`
    fn list_url(mc: &str) -> String {
        format!("{}/optifine/{mc}", endpoints::BMCLAPI)
    }

    /// `https://bmclapi2.bangbang93.com/optifine/1.21.4/HD_U/J3`
    fn download_url(mc: &str, kind: &str, patch: &str) -> String {
        format!("{}/optifine/{mc}/{kind}/{patch}", endpoints::BMCLAPI)
    }

    /// Versión maven del artefacto OptiFine: `<mc>_<ed>_<rel>`, p. ej.
    /// `26.2_HD_U_K2_pre1`. Con la coordenada `optifine:OptiFine:<versión>` la ruta
    /// maven resulta `optifine/OptiFine/<versión>/OptiFine-<versión>.jar`, que es
    /// exactamente la convención del instalador oficial (el prefijo `OptiFine-`
    /// del fichero lo añade el maven, NO va en la versión: si va en ambas, sale
    /// `OptiFine-OptiFine-…`).
    fn coord(mc: &str, kind: &str, patch: &str) -> String {
        format!("{mc}_{kind}_{patch}")
    }

    /// Ruta local del jar parcheado (la genera este módulo con el Patcher).
    fn patched_jar_path(paths: &crate::core::paths::Paths, version: &str) -> std::path::PathBuf {
        let coord = MavenCoord::parse(&format!("optifine:OptiFine:{version}"))
            .expect("coordenada OptiFine válida");
        paths.library_file(&coord.path())
    }

    /// Librería local que representa el jar parcheado: existe en `libraries/`
    /// pero no hay nada que descargar de ningún maven (`local_only`).
    fn optifine_library(version: &str) -> Library {
        Library {
            name: format!("optifine:OptiFine:{version}"),
            downloads: None,
            natives: None,
            rules: None,
            url: None,
            extract: None,
            local_only: true,
        }
    }

    fn launchwrapper_library(version: &str) -> Library {
        Library {
            name: format!("optifine:launchwrapper-of:{version}"),
            downloads: None,
            natives: None,
            rules: None,
            url: None,
            extract: None,
            local_only: true,
        }
    }

    /// Ejecuta el Patcher de OptiFine (solo OptiFine clásico).
    fn run_patcher(
        ctx: &LoaderCtx<'_>,
        installer_jar: &Path,
        client_jar: &Path,
        out_jar: &Path,
    ) -> Result<()> {
        // OptiFine clásico funciona con Java 8; el moderno, con 8+.
        let opts = install::InstallOptions::default();
        let (java, _rt) = install::resolve_java(ctx.http, ctx.paths, 8, None, &opts, ctx.progress)?;

        ctx.progress.phase("Parcheando el client jar con OptiFine");
        // El Patcher escribe con FileOutputStream de Java, que NO crea carpetas:
        // sin esto, falla con «El sistema no puede encontrar la ruta especificada».
        if let Some(parent) = out_jar.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        let output = hidden_command(&java)
            .arg("-cp")
            .arg(installer_jar)
            .arg("optifine.Patcher")
            .arg(client_jar)
            .arg(installer_jar)
            .arg(out_jar)
            .output()
            .map_err(|e| Error::Launch(format!("no pude ejecutar optifine.Patcher: {e}")))?;

        if !output.status.success() || !out_jar.is_file() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let tail: String = stderr
                .lines()
                .chain(stdout.lines())
                .filter(|l| !l.trim().is_empty())
                .rev()
                .take(8)
                .collect::<Vec<_>>()
                .join("\n");
            return Err(Error::Launch(format!(
                "el Patcher de OptiFine terminó con {}. {}",
                output.status,
                if tail.is_empty() { String::new() } else { format!("Últimas líneas:\n{tail}") }
            )));
        }
        Ok(())
    }

    /// ¿Contiene el jar `optifine/Patcher.class`? (OptiFine clásico)
    fn has_patcher(jar: &Path) -> bool {
        let Ok(file) = std::fs::File::open(jar) else {
            return false;
        };
        let mut archive = match zip::ZipArchive::new(file) {
            Ok(archive) => archive,
            Err(_) => return false,
        };
        archive
            .by_name("optifine/Patcher.class")
            .map(|_| true)
            .unwrap_or(false)
    }

    /// Versión de launchwrapper que trae dentro (`launchwrapper-of-<v>.jar`).
    /// Devuelve el jar extraído a `libraries/` y su versión.
    fn extract_launchwrapper(
        ctx: &LoaderCtx<'_>,
        installer_jar: &Path,
    ) -> Result<Option<(PathBuf, String)>> {
        let file = std::fs::File::open(installer_jar)
            .map_err(|e| Error::io(installer_jar, e))?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| Error::Zip { path: installer_jar.into(), reason: e.to_string() })?;

        // OptiFine moderno: `launchwrapper-of.txt` + `launchwrapper-of-<v>.jar`.
        let version = match archive.by_name("launchwrapper-of.txt") {
            Ok(mut entry) => {
                let mut text = String::new();
                use std::io::Read as _;
                entry.read_to_string(&mut text).map_err(|e| Error::io(installer_jar, e))?;
                text.trim().to_string()
            }
            Err(_) => String::new(),
        };
        if !version.is_empty() {
            let inner = format!("launchwrapper-of-{version}.jar");
            let dest = ctx
                .paths
                .library_file(&format!("optifine/launchwrapper-of/{version}/{inner}"));
            let mut entry = archive
                .by_name(&inner)
                .map_err(|e| Error::Zip { path: installer_jar.into(), reason: e.to_string() })?;
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
            }
            let mut out = std::fs::File::create(&dest).map_err(|e| Error::io(&dest, e))?;
            std::io::copy(&mut entry, &mut out).map_err(|e| Error::io(&dest, e))?;
            return Ok(Some((dest, version)));
        }

        // OptiFine clásico: `launchwrapper-2.0.jar` (sin sufijo de versión).
        if let Ok(mut entry) = archive.by_name("launchwrapper-2.0.jar") {
            let dest = ctx
                .paths
                .library_file("optifine/launchwrapper/2.0/launchwrapper-2.0.jar");
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
            }
            let mut out = std::fs::File::create(&dest).map_err(|e| Error::io(&dest, e))?;
            std::io::copy(&mut entry, &mut out).map_err(|e| Error::io(&dest, e))?;
            return Ok(Some((dest, "2.0".to_string())));
        }

        Ok(None)
    }
}

impl Loader for OptiFineLoader {
    fn kind(&self) -> LoaderKind {
        LoaderKind::OptiFine
    }

    fn list_versions(&self, ctx: &LoaderCtx<'_>, mc: &str) -> Result<Vec<LoaderVersion>> {
        // BMCLAPI a veces responde `[]` vacío cuando acaba de recibir una ráfaga
        // de peticiones (le pasa a la sonda de versiones soportadas). Un reintento
        // con respiro lo absorbe; si sigue vacío, es que de verdad no hay nada.
        let fetch = || -> Result<Vec<BmclEntry>> {
            let raw = ctx.http.get_string(&Self::list_url(mc))?;
            serde_json::from_str(&raw).map_err(Error::from)
        };
        let mut entries = fetch()?;
        if entries.is_empty() {
            std::thread::sleep(std::time::Duration::from_millis(800));
            entries = fetch()?;
        }
        if entries.is_empty() {
            let suggestion = newest_optifine_mc(ctx.http, ctx.paths)
                .map(|newest| format!(" La más reciente con OptiFine es {newest}."))
                .unwrap_or_default();
            return Err(Error::Unsupported(format!(
                "OptiFine no tiene versiones para Minecraft {mc}.{suggestion}"
            )));
        }
        Ok(entries
            .into_iter()
            .map(|entry| LoaderVersion {
                id: format!("{}_{}", entry.kind, entry.patch),
                stable: true,
                mc: mc.to_string(),
            })
            .collect())
    }

    fn supported_mc_versions(&self, ctx: &LoaderCtx<'_>) -> Option<Vec<String>> {
        optifine_supported_mcs(ctx.http, ctx.paths)
    }

    fn resolve(
        &self,
        ctx: &LoaderCtx<'_>,
        mc: &str,
        loader_version: Option<&str>,
        version_id: &str,
        _opts: &InstallOptions,
    ) -> Result<VersionJson> {
        let loader = loader_version.ok_or_else(|| Error::Missing("falta la versión de OptiFine (p. ej. HD_U_J3)".into()))?;
        // `HD_U_J3` → kind `HD_U`, patch `J3`. El patch es el último componente:
        // cortar por el primer guion bajo partiría `HD_U` en dos.
        let (kind, patch) = loader.rsplit_once('_').ok_or_else(|| {
            Error::Unsupported(format!(
                "«{loader}» no parece una versión de OptiFine (esperaba algo como HD_U_J3)"
            ))
        })?;
        let version = Self::coord(mc, kind, patch);

        // 1) ¿Ya está generado?
        let dest = ctx.paths.version_json(version_id);
        if let Ok(raw) = std::fs::read_to_string(&dest) {
            if let Ok(version) = VersionJson::from_str(&raw) {
                return Ok(version);
            }
        }

        // 2) Vanilla base.
        let parent = install::resolve_vanilla(ctx.http, ctx.paths, ctx.progress, mc, mc)?;

        // 3) Instalador de OptiFine.
        let work_dir = ctx.paths.version_dir(version_id);
        std::fs::create_dir_all(&work_dir).map_err(|e| Error::io(&work_dir, e))?;
        let installer_jar = work_dir.join("OptiFine-installer.jar");
        if !installer_jar.is_file() {
            ctx.progress.phase("Descargando OptiFine (BMCLAPI)");
            ctx.http
                .download(&crate::core::http::Download::new(
                    Self::download_url(mc, kind, patch),
                    &installer_jar,
                ))?;
        }

        // 4) Jar de salida + launchwrapper.
        let patched_jar = Self::patched_jar_path(ctx.paths, &version);
        if !patched_jar.is_file() {
            if Self::has_patcher(&installer_jar) {
                let client_jar = ctx.paths.version_jar(mc);
                if !client_jar.is_file() {
                    // El Patcher necesita el client vanilla: baja el mínimo.
                    install::download(
                        ctx.http,
                        ctx.paths,
                        &parent,
                        mc,
                        &install::InstallOptions {
                            skip_assets: true,
                            dry_run: false,
                            ..install::InstallOptions::default()
                        },
                        ctx.progress,
                    )?;
                }
                Self::run_patcher(ctx, &installer_jar, &client_jar, &patched_jar)?;
            } else {
                // OptiFine moderno (1.17+): el instalador ES la librería.
                ctx.progress.phase("Copiando OptiFine como librería");
                if let Some(lib_parent) = patched_jar.parent() {
                    std::fs::create_dir_all(lib_parent).map_err(|e| Error::io(lib_parent, e))?;
                }
                std::fs::copy(&installer_jar, &patched_jar)
                    .map_err(|e| Error::io(&patched_jar, e))?;
            }
        }

        let launchwrapper = Self::extract_launchwrapper(ctx, &installer_jar)?;

        // 5) Manifiesto: hijo (OptiFine) fusionado con el vanilla. CRÍTICO fusionar:
        // launchwrapper necesita sus dependencias (log4j, asm, jopt-simple) y el
        // juego las de LWJGL — todas vienen de las librerías del padre. Sin el
        // merge, el classpath solo tendría 3 jars y moriría en silencio.
        let mut libraries = vec![Self::optifine_library(&version)];
        if let Some((_, lw_version)) = &launchwrapper {
            libraries.push(Self::launchwrapper_library(lw_version));
        } else {
            libraries.push(Library {
                name: "net.minecraft:launchwrapper:1.12".into(),
                downloads: None,
                natives: None,
                rules: None,
                url: Some(endpoints::LIBRARIES_MAVEN.into()),
                extract: None,
                local_only: false,
            });
        }

        let arguments = Arguments {
            game: vec![ArgValue::Plain("--tweakClass".into()), ArgValue::Plain("optifine.OptiFineTweaker".into())],
            jvm: Vec::new(),
        };

        let child = VersionJson {
            id: version_id.to_string(),
            inherits_from: Some(mc.to_string()),
            main_class: "net.minecraft.launchwrapper.Launch".into(),
            libraries,
            arguments: Some(arguments),
            version_type: Some("release".into()),
            ..VersionJson::default()
        };
        let mut merged = VersionJson::merge(&parent, &child);
        merged.id = version_id.to_string();
        merged.inherits_from = None;
        Ok(merged)
    }
}/// ¿Para qué versiones de MC tiene builds OptiFine? BMCLAPI no publica un listado
/// global (los endpoints `all`/`list` devuelven vacío), pero `/optifine/{mc}`
/// responde `[]` (HTTP 200) cuando no hay nada.
///
/// Se sondean TODAS las releases del manifiesto en tandas (así no se asume nada:
/// 1.7.6 de verdad no tiene, y lo sabremos) y el resultado se cachea en
/// `cache/optifine-mcs.json` para no repetir la sonda (~180 peticiones, en tandas
/// tarda unos segundos; con caché, instantáneo).
pub fn optifine_supported_mcs(http: &HttpClient, paths: &Paths) -> Option<Vec<String>> {
    if let Some(cached) = read_supported_cache(paths) {
        return Some(cached);
    }

    let manifest = install::cached_manifest_of(http, paths)?;
    let releases: Vec<String> = manifest
        .versions
        .iter()
        .filter(|entry| entry.version_type == crate::core::manifest::VersionType::Release)
        .map(|entry| entry.id.clone())
        .collect();

    let supported: Vec<String> = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        // En tandas de 6 con un respiro entre tandas: BMCLAPI con 180 peticiones
        // seguidas a la vez a veces responde `[]` vacío a algunas.
        for (batch, mc) in releases.iter().enumerate() {
            if batch > 0 && batch % 6 == 0 {
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
            let http = &http;
            handles.push((mc.clone(), scope.spawn(move || {
                let has = http
                    .get_string(&OptiFineLoader::list_url(mc))
                    .ok()
                    .and_then(|raw| serde_json::from_str::<Vec<BmclEntry>>(&raw).ok())
                    .is_some_and(|entries| !entries.is_empty());
                has
            })));
        }
        handles
            .into_iter()
            .filter_map(|(mc, handle)| handle.join().ok().filter(|has| *has).map(|_| mc))
            .collect()
    });

    if supported.is_empty() {
        return None; // la red dice que no: mejor no filtrar
    }
    write_supported_cache(paths, &supported);
    Some(supported)
}

/// Caché de la sonda (`cache/optifine-mcs.json`): una lista de ids de MC.
fn supported_cache_file(paths: &Paths) -> std::path::PathBuf {
    paths.root().join("cache").join("optifine-mcs.json")
}

fn read_supported_cache(paths: &Paths) -> Option<Vec<String>> {
    let raw = std::fs::read_to_string(supported_cache_file(paths)).ok()?;
    let list: Vec<String> = serde_json::from_str(&raw).ok()?;
    (!list.is_empty()).then_some(list)
}

fn write_supported_cache(paths: &Paths, supported: &[String]) {
    let file = supported_cache_file(paths);
    if let Some(parent) = file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(raw) = serde_json::to_string(supported) {
        let _ = std::fs::write(&file, raw);
    }
}

/// La versión estable más reciente que tiene builds de OptiFine, para sugerirla
/// cuando el usuario elige una que no tiene.
fn newest_optifine_mc(http: &HttpClient, paths: &Paths) -> Option<String> {
    let manifest = install::cached_manifest_of(http, paths)?;
    let recent: Vec<String> = manifest
        .versions
        .iter()
        .filter(|entry| entry.version_type == crate::core::manifest::VersionType::Release)
        .take(24)
        .map(|entry| entry.id.clone())
        .collect();

    let supported = optifine_supported_mcs(http, paths)?;
    recent.into_iter().find(|mc| supported.contains(mc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsea_la_lista_de_bmclapi() {
        // Forma real de https://bmclapi2.bangbang93.com/optifine/1.21.4
        let raw = r#"[
            {"_id":"a","mcversion":"1.21.4","type":"HD_U","patch":"J3","date":"2024-08-08","filename":"OptiFine_1.21.4_HD_U_J3.jar","forge":"Forge 54.0.34"},
            {"_id":"b","mcversion":"1.21.4","type":"HD_U","patch":"I7","date":"2024-06-14","filename":"OptiFine_1.21.4_HD_U_I7.jar","forge":"Forge 54.0.15"}
        ]"#;
        let entries: Vec<BmclEntry> = serde_json::from_str(raw).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].kind, "HD_U");
        assert_eq!(entries[0].patch, "J3");
    }

    #[test]
    fn urls_de_bmclapi() {
        assert_eq!(OptiFineLoader::list_url("1.21.4"), "https://bmclapi2.bangbang93.com/optifine/1.21.4");
        assert_eq!(
            OptiFineLoader::download_url("1.21.4", "HD_U", "J3"),
            "https://bmclapi2.bangbang93.com/optifine/1.21.4/HD_U/J3"
        );
    }

    #[test]
    fn coordenadas_y_librerias() {
        // La convención maven de OptiFine: el prefijo del fichero lo añade la ruta,
        // no la versión (si no, salía OptiFine-OptiFine-…).
        assert_eq!(OptiFineLoader::coord("1.21.4", "HD_U", "J3"), "1.21.4_HD_U_J3");
        let coord = MavenCoord::parse("optifine:OptiFine:1.21.4_HD_U_J3").unwrap();
        assert_eq!(
            coord.path(),
            "optifine/OptiFine/1.21.4_HD_U_J3/OptiFine-1.21.4_HD_U_J3.jar"
        );

        let lib = OptiFineLoader::optifine_library("1.21.4_HD_U_J3");
        assert_eq!(lib.name, "optifine:OptiFine:1.21.4_HD_U_J3");
        assert!(lib.url.is_none());
        assert!(lib.local_only);

        let lw = OptiFineLoader::launchwrapper_library("2.0");
        assert_eq!(lw.name, "optifine:launchwrapper-of:2.0");
        assert!(lw.local_only);
    }

    #[test]
    fn parte_el_id_de_version() {
        // El tipo es `HD_U` (dos componentes) y el patch el resto.
        let (kind, patch) = "HD_U_J3".rsplit_once('_').unwrap();
        assert_eq!(kind, "HD_U");
        assert_eq!(patch, "J3");

        let (kind, patch) = "HD_U_H9_pre1".rsplit_once('_').unwrap();
        assert_eq!(kind, "HD_U_H9");
        assert_eq!(patch, "pre1");
    }
}
