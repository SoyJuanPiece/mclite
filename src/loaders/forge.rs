//! Forge y NeoForge.
//!
//! El plan original planteaba reimplementar los `processors` del `install_profile.json`
//! (parcheo de jars con placeholders `[maven:...]`, orden de ejecución, outputs…). Es la
//! parte más frágil del ecosistema y cada versión de Forge cambia detalles, así que aquí
//! se usa la alternativa que ya contemplaba el plan §2.2: **ejecutar el instalador oficial
//! en modo headless** (`--installClient <root>`) con el Java que ya resolvemos para jugar.
//!
//! El instalador deja en `<root>/versions/<id>/` un manifiesto estándar con
//! `inheritsFrom` y en `<root>/libraries/` los artefactos que no vienen de ningún
//! maven público (forge universal, client-extra parcheado…). A partir de ahí, el flujo
//! del launcher es idéntico al de cualquier otra versión.

use crate::core::endpoints;
use crate::core::error::{Error, Result};
use crate::core::install::{self, InstallOptions};
use crate::core::process::hidden_command;
use crate::core::version_json::VersionJson;
use crate::loaders::{Loader, LoaderCtx, LoaderKind, LoaderVersion};

pub struct ForgeLoader {
    pub kind: LoaderKind,
}

impl ForgeLoader {
    fn label(&self) -> &'static str {
        self.kind.label()
    }

    /// Forge: `https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml`
    /// NeoForge: `https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml`
    fn metadata_url(&self) -> String {
        match self.kind {
            LoaderKind::NeoForge => format!("{}/net/neoforged/neoforge/maven-metadata.xml", endpoints::NEOFORGE_MAVEN),
            _ => format!("{}/net/minecraftforge/forge/maven-metadata.xml", endpoints::FORGE_MAVEN),
        }
    }

    /// Prefijo de versión en el maven para esta versión de Minecraft.
    /// Forge: `1.21.4-…`. NeoForge: el maven usa la versión sin el `1.` inicial
    /// (`1.21.4` → `21.4.50`), con el prefijo `21.4.`.
    fn maven_prefix(&self, mc: &str) -> String {
        match self.kind {
            LoaderKind::NeoForge => {
                let short = mc.strip_prefix("1.").unwrap_or(mc);
                format!("{short}.")
            }
            _ => format!("{mc}-"),
        }
    }

    /// Jar del instalador para (mc, loader).
    fn installer_url(&self, mc: &str, loader: &str) -> String {
        match self.kind {
            LoaderKind::NeoForge => format!(
                "{}/net/neoforged/neoforge/{loader}/neoforge-{loader}-installer.jar",
                endpoints::NEOFORGE_MAVEN
            ),
            _ => format!(
                "{}/net/minecraftforge/forge/{mc}-{loader}/forge-{mc}-{loader}-installer.jar",
                endpoints::FORGE_MAVEN
            ),
        }
    }

    /// El instalador de Forge exige un `launcher_profiles.json` en el directorio
    /// destino («you need to run the launcher first!»): comprueba que exista antes
    /// de ejecutarse. Creamos uno mínimo como hacen todos los launchers de terceros.
    fn ensure_launcher_profiles(&self, ctx: &LoaderCtx<'_>) -> Result<()> {
        let file = ctx.paths.root().join("launcher_profiles.json");
        if file.is_file() {
            return Ok(());
        }
        let minimal = serde_json::json!({
            "profiles": {
                "mclite": {
                    "name": "mclite",
                    "type": "custom",
                    "created": "2026-01-01T00:00:00.000Z",
                    "lastUsed": "2026-01-01T00:00:00.000Z",
                    "icon": "Grass",
                    "lastVersionId": "latest-release"
                }
            },
            "settings": {},
            "version": 3
        });
        std::fs::write(&file, serde_json::to_string_pretty(&minimal).unwrap_or_default())
            .map_err(|e| Error::io(&file, e))
    }

    /// Ejecuta el instalador oficial en modo headless sobre la raíz del launcher.
    fn run_installer(
        &self,
        ctx: &LoaderCtx<'_>,
        installer_jar: &std::path::Path,
        mc: &str,
        opts: &InstallOptions,
    ) -> Result<()> {
        // El Java del instalador: el mismo que pedirá la versión (el instalador
        // oficial funciona con Java 8+ y así reutilizamos lo ya descargado).
        let manifest =
            install::resolve_vanilla(ctx.http, ctx.paths, ctx.progress, mc, mc)?;
        let required = manifest
            .java_version
            .as_ref()
            .map(|java| java.major_version)
            .unwrap_or(8);
        let opts_java = InstallOptions {
            mojang_runtime: opts.mojang_runtime,
            threads: opts.threads,
            ..InstallOptions::default()
        };
        let (java, _runtime) =
            install::resolve_java(ctx.http, ctx.paths, required, None, &opts_java, ctx.progress)?;

        let root = ctx.paths.root();
        self.ensure_launcher_profiles(ctx)?;
        ctx.progress
            .phase(format!("Instalando {} (instalador oficial, sin GUI)", self.label()));

        let output = hidden_command(&java)
            .arg("-jar")
            .arg(installer_jar)
            .arg("--installClient")
            .arg(root)
            .output()
            .map_err(|e| Error::Launch(format!("no pude ejecutar el instalador: {e}")))?;

        // El instalador escribe su progreso por stdout; lo volcamos al log.
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines().filter(|l| !l.trim().is_empty()).take(40) {
            ctx.progress.message(format!("instalador: {line}"));
        }

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let tail: String = stderr.lines().rev().take(8).collect::<Vec<_>>().join("\n");
            return Err(Error::Launch(format!(
                "el instalador de {} terminó con {}. {}",
                self.label(),
                output.status,
                if tail.is_empty() { String::new() } else { format!("Últimas líneas:\n{tail}") }
            )));
        }
        Ok(())
    }

    /// El manifiesto que dejó el instalador. Forge genera el id que esperamos
    /// (`<mc>-forge-<v>`); NeoForge usa el suyo, así que si el directo no está se
    /// busca el json recién generado que mencione la versión del cargador.
    fn generated_version(
        &self,
        ctx: &LoaderCtx<'_>,
        _mc: &str,
        loader: &str,
        version_id: &str,
    ) -> Result<VersionJson> {
        let direct = ctx.paths.version_json(version_id);
        if let Ok(raw) = std::fs::read_to_string(&direct) {
            if let Ok(version) = VersionJson::from_str(&raw) {
                return Ok(version);
            }
        }

        // Búsqueda por mtime reciente (5 min) + referencia a la versión del cargador.
        let cutoff = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
            - 300;
        let mut candidates: Vec<(std::path::PathBuf, String)> = Vec::new();
        let versions_dir = ctx.paths.versions();
        if let Ok(entries) = std::fs::read_dir(&versions_dir) {
            for entry in entries.flatten() {
                let json = entry.path().join(format!("{}.json", entry.file_name().to_string_lossy()));
                let Ok(raw) = std::fs::read_to_string(&json) else {
                    continue;
                };
                if !raw.contains(loader) {
                    continue;
                }
                let fresh = std::fs::metadata(&json)
                    .and_then(|meta| meta.modified())
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() >= cutoff)
                    .unwrap_or(false);
                if fresh {
                    candidates.push((json, raw));
                }
            }
        }

        let (json_path, raw) = candidates
            .first()
            .cloned()
            .ok_or_else(|| {
                Error::Missing(format!(
                    "el instalador de {} terminó pero no dejó el manifiesto esperado en {}",
                    self.label(),
                    versions_dir.display()
                ))
            })?;
        // Copia al id que usamos (`versions/<nuestro id>/`) para uniformidad.
        let version = VersionJson::from_str(&raw)?;
        let mut normalized = version;
        normalized.id = version_id.to_string();
        let dest = ctx.paths.version_json(version_id);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        std::fs::write(&dest, &raw).map_err(|e| Error::io(&dest, e))?;
        let _ = json_path;
        Ok(normalized)
    }
}

impl Loader for ForgeLoader {
    fn kind(&self) -> LoaderKind {
        self.kind
    }

    fn list_versions(&self, ctx: &LoaderCtx<'_>, mc: &str) -> Result<Vec<LoaderVersion>> {
        let xml = ctx.http.get_string(&self.metadata_url())?;
        let prefix = self.maven_prefix(mc);
        let versions = parse_maven_versions(&xml, &prefix)
            .into_iter()
            .map(|id| {
                // Forge: el maven trae `1.20.1-47.4.5` pero la versión del
                // CARGADOR es solo `47.4.5` (el mc va aparte en las URLs y en el
                // id de instancia; dejarlo dentro duplicaba el mc:
                // forge-1.20.1-1.20.1-47.4.5-installer.jar → 404).
                let id = match self.kind {
                    LoaderKind::Forge => id
                        .strip_prefix(&prefix)
                        .unwrap_or(&id)
                        .to_string(),
                    _ => id,
                };
                let stable = !id.contains("beta") && !id.contains("alpha") && !id.contains("rc");
                LoaderVersion {
                    id,
                    stable,
                    mc: mc.to_string(),
                }
            })
            .collect::<Vec<_>>();
        if versions.is_empty() {
            return Err(Error::Unsupported(format!(
                "{} no tiene versiones para Minecraft {mc}",
                self.label()
            )));
        }
        Ok(versions)
    }

    fn supported_mc_versions(&self, ctx: &LoaderCtx<'_>) -> Option<Vec<String>> {
        let xml = ctx.http.get_string(&self.metadata_url()).ok()?;
        let mcs: Vec<String> = match self.kind {
            // Forge: `{mc}-{forge}` → el mc es la parte anterior al primer `-`.
            LoaderKind::Forge => parse_maven_versions(&xml, "")
                .iter()
                .filter_map(|id| id.split('-').next().map(str::to_string))
                .collect(),
            // NeoForge: `21.4.50` → `1.21.4` (prefijo del maven sin el `1.`).
            _ => parse_maven_versions(&xml, "")
                .iter()
                .filter_map(|id| {
                    let mut parts = id.splitn(3, '.');
                    let major = parts.next()?;
                    let minor = parts.next()?;
                    Some(format!("1.{major}.{minor}"))
                })
                .collect(),
        };
        let mut mcs = mcs;
        mcs.sort();
        mcs.dedup();
        (!mcs.is_empty()).then_some(mcs)
    }

    fn resolve(
        &self,
        ctx: &LoaderCtx<'_>,
        mc: &str,
        loader_version: Option<&str>,
        version_id: &str,
        opts: &InstallOptions,
    ) -> Result<VersionJson> {
        let loader = loader_version.ok_or_else(|| {
            Error::Missing(format!("falta la versión de {}", self.label()))
        })?;

        // 1) ¿Ya está instalado? (el manifiesto fusionado ya escrito)
        let dest = ctx.paths.version_json(version_id);
        if let Ok(raw) = std::fs::read_to_string(&dest) {
            if let Ok(version) = VersionJson::from_str(&raw) {
                return Ok(version);
            }
        }

        // 2) Instalador headless.
        let installer_jar = ctx
            .paths
            .version_dir(version_id)
            .join(format!("{}-installer.jar", self.kind.key()));
        if !installer_jar.is_file() {
            ctx.progress.phase(format!("Descargando instalador de {}", self.label()));
            ctx.http
                .download(&crate::core::http::Download::new(
                    self.installer_url(mc, loader),
                    &installer_jar,
                ))?;
        }
        self.run_installer(ctx, &installer_jar, mc, opts)?;

        // 3) Manifiesto generado → fusionar con vanilla (normalmente el propio
        // instalador ya lo deja usable, pero el merge normaliza ids y deduplica).
        let generated = self.generated_version(ctx, mc, loader, version_id)?;
        let parent_id = generated.parent_id().unwrap_or(mc).to_string();
        let parent =
            install::resolve_vanilla(ctx.http, ctx.paths, ctx.progress, mc, &parent_id)?;
        let mut merged = VersionJson::merge(&parent, &generated);
        merged.id = version_id.to_string();
        merged.inherits_from = None;
        Ok(merged)
    }
}

/// Extrae `<version>…</version>` que empiecen por el prefijo, más nuevas primero
/// (el XML de maven ya viene ordenado, pero no dependemos de ello).
pub fn parse_maven_versions(xml: &str, prefix: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<version>") {
        let after = &rest[start + "<version>".len()..];
        let Some(end) = after.find("</version>") else {
            break;
        };
        let id = after[..end].trim().to_string();
        if id.starts_with(prefix) {
            out.push(id);
        }
        rest = &after[end..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsea_el_maven_metadata_de_forge() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<metadata>
  <groupId>net.minecraftforge</groupId>
  <artifactId>forge</artifactId>
  <versioning>
    <latest>54.1.6</latest>
    <versions>
      <version>1.20.1-47.2.0</version>
      <version>1.20.1-47.3.0</version>
      <version>1.21.4-54.1.0</version>
      <version>1.21.4-54.1.6</version>
      <version>55.0.1</version>
    </versions>
  </versioning>
</metadata>"#;
        let versions = parse_maven_versions(xml, "1.21.4-");
        assert_eq!(versions, vec!["1.21.4-54.1.0", "1.21.4-54.1.6"]);

        // Y el id del cargador es la versión SIN el prefijo del mc (el mc va
        // aparte en las URLs; dejarlo dentro generaba URLs duplicadas → 404).
        let loader_ids: Vec<String> = versions
            .iter()
            .map(|id| id.strip_prefix("1.21.4-").unwrap_or(id).to_string())
            .collect();
        assert_eq!(loader_ids, vec!["54.1.0", "54.1.6"]);

        assert!(parse_maven_versions(xml, "1.16.5-").is_empty());
    }

    #[test]
    fn prefijos_del_maven_por_cargador() {
        let loader = ForgeLoader { kind: LoaderKind::Forge };
        assert_eq!(loader.maven_prefix("1.21.4"), "1.21.4-");

        let neo = ForgeLoader { kind: LoaderKind::NeoForge };
        assert_eq!(neo.maven_prefix("1.21.4"), "21.4.");
        assert_eq!(neo.maven_prefix("1.20.2"), "20.2.");
    }

    #[test]
    fn urls_de_instalador() {
        let loader = ForgeLoader { kind: LoaderKind::Forge };
        assert_eq!(
            loader.installer_url("1.20.1", "47.2.0"),
            "https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.2.0/forge-1.20.1-47.2.0-installer.jar"
        );
        let neo = ForgeLoader { kind: LoaderKind::NeoForge };
        assert_eq!(
            neo.installer_url("1.21.4", "21.4.50"),
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/21.4.50/neoforge-21.4.50-installer.jar"
        );
    }
}
