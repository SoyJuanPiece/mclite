//! Fabric y Quilt. Es el mismo código con dos `meta` distintos.
//!
//! El trabajo real lo hace `/profile/json`: Fabric publica ahí un manifiesto con
//! `inheritsFrom`, y aquí se fusiona con el vanilla. Esa fusión es la que después
//! reutiliza Forge, por eso vive en `VersionJson::merge` y no en este módulo.

use serde::Deserialize;

use crate::core::endpoints;
use crate::core::error::{Error, Result};
use crate::core::http::HttpClient;
use crate::core::install::{self, InstallOptions};
use crate::core::version_json::VersionJson;
use crate::loaders::{Loader, LoaderCtx, LoaderKind, LoaderVersion};

pub struct FabricLoader {
    pub kind: LoaderKind,
}

impl FabricLoader {
    fn meta_base(&self) -> &'static str {
        match self.kind {
            LoaderKind::Quilt => endpoints::QUILT_META,
            _ => endpoints::FABRIC_META,
        }
    }

    /// Fabric usa `/v2`, Quilt `/v3`.
    fn api_version(&self) -> &'static str {
        match self.kind {
            LoaderKind::Quilt => "v3",
            _ => "v2",
        }
    }
}

#[derive(Debug, Deserialize)]
struct GameEntry {
    version: String,
}

/// Lista global de versiones de MC con mapeo de intermediario: exactamente las
/// que este cargador soporta.
fn game_versions(meta: &str, api: &str, http: &HttpClient) -> Option<Vec<String>> {
    let raw = http
        .get_string(&format!("{meta}/{api}/versions/game"))
        .ok()?;
    let entries: Vec<GameEntry> = serde_json::from_str(&raw).ok()?;
    (!entries.is_empty()).then(|| entries.into_iter().map(|e| e.version).collect())
}

#[derive(Debug, Deserialize)]
struct LoaderInfo {
    version: String,
}

#[derive(Debug, Deserialize)]
struct LoaderEntry {
    loader: LoaderInfo,
}

impl Loader for FabricLoader {
    fn kind(&self) -> LoaderKind {
        self.kind
    }

    fn supported_mc_versions(&self, ctx: &LoaderCtx<'_>) -> Option<Vec<String>> {
        game_versions(self.meta_base(), self.api_version(), ctx.http)
    }

    fn list_versions(&self, ctx: &LoaderCtx<'_>, mc: &str) -> Result<Vec<LoaderVersion>> {
        let url = format!(
            "{}/{}/versions/loader/{}",
            self.meta_base(),
            self.api_version(),
            mc
        );
        let raw = ctx.http.get_string(&url)?;
        let versions = parse_versions(&raw, mc)?;
        if versions.is_empty() {
            return Err(Error::Unsupported(format!(
                "{} no tiene versiones de cargador para Minecraft {mc}",
                self.kind.label()
            )));
        }
        Ok(versions)
    }

    fn resolve(
        &self,
        ctx: &LoaderCtx<'_>,
        mc: &str,
        loader_version: Option<&str>,
        version_id: &str,
        _opts: &InstallOptions,
    ) -> Result<VersionJson> {
        let loader = loader_version.ok_or_else(|| {
            Error::Missing(format!("falta la versión de {}", self.kind.label()))
        })?;
        let url = format!(
            "{}/{}/versions/loader/{}/{}/profile/json",
            self.meta_base(),
            self.api_version(),
            mc,
            loader
        );
        let raw = ctx.http.get_string(&url)?;
        let child = VersionJson::from_str(&raw)?;

        let parent_id = child.parent_id().unwrap_or(mc).to_string();
        let parent = install::resolve_vanilla(ctx.http, ctx.paths, ctx.progress, &parent_id, &parent_id)?;

        let mut merged = VersionJson::merge(&parent, &child);
        merged.id = version_id.to_string();
        // El id manda el nuestro: de ahí salen `versions/<id>/` y `--version`.
        merged.inherits_from = None;
        Ok(merged)
    }
}

/// Acepta tanto el formato de Fabric (`[{loader:{version}}]`) como el de Quilt.
fn parse_versions(raw: &str, mc: &str) -> Result<Vec<LoaderVersion>> {
    let value: serde_json::Value = serde_json::from_str(raw)?;
    let list = value.as_array().ok_or_else(|| {
        Error::Unsupported("la lista de loaders no es un array".to_string())
    })?;

    let mut versions = Vec::new();
    for item in list {
        // Fabric: {"loader":{"version":"0.19.5","stable":true}, ...}
        if let Ok(entry) = serde_json::from_value::<LoaderEntry>(item.clone()) {
            let stable = item
                .get("loader")
                .and_then(|loader| loader.get("stable"))
                .and_then(|stable| stable.as_bool())
                .unwrap_or(true);
            versions.push(LoaderVersion {
                id: entry.loader.version,
                stable,
                mc: mc.to_string(),
            });
            continue;
        }
        // Quilt: {"version":"0.27.1","separator":"."} o directamente una cadena.
        let id = item
            .as_str()
            .map(str::to_string)
            .or_else(|| {
                item.get("version")
                    .and_then(|version| version.as_str())
                    .map(str::to_string)
            });
        if let Some(id) = id {
            let stable = !id.contains("beta") && !id.contains("alpha") && !id.contains("rc");
            versions.push(LoaderVersion {
                id,
                stable,
                mc: mc.to_string(),
            });
        }
    }

    // Más nuevas primero: la API de Fabric ya viene ordenada, pero no dependemos de eso.
    versions.sort_by(|a, b| compare_versions(&b.id, &a.id));
    Ok(versions)
}

/// Comparación de versiones numéricas por componentes (`0.16.10` > `0.16.9`).
pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |value: &str| -> Vec<u64> {
        value
            .split(['.', '-', '_', '+'])
            .map(|part| {
                let digits: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
                digits.parse::<u64>().unwrap_or(0)
            })
            .collect()
    };
    let (left, right) = (parse(a), parse(b));
    for index in 0..left.len().max(right.len()) {
        let l = left.get(index).copied().unwrap_or(0);
        let r = right.get(index).copied().unwrap_or(0);
        match l.cmp(&r) {
            std::cmp::Ordering::Equal => continue,
            other => return other,
        }
    }
    std::cmp::Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsea_la_lista_de_fabric() {
        // Forma real de https://meta.fabricmc.net/v2/versions/loader/1.21.4
        let raw = r#"[
            {"loader":{"separator":".","build":13,"maven":"net.fabricmc:fabric-loader:0.16.10",
                      "version":"0.16.10","stable":true},
             "intermediary":{"version":"1.21.4","stable":true},
             "launcherMeta":{"version":2,"libraries":{}}},
            {"loader":{"separator":".","build":12,"maven":"net.fabricmc:fabric-loader:0.16.9",
                      "version":"0.16.9","stable":false},
             "intermediary":{"version":"1.21.4","stable":true},
             "launcherMeta":{"version":2,"libraries":{}}}
        ]"#;
        let versions = parse_versions(raw, "1.21.4").unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].id, "0.16.10");
        assert!(versions[0].stable);
        assert!(!versions[1].stable);
        assert_eq!(versions[0].mc, "1.21.4");
    }

    #[test]
    fn ordena_la_lista_de_mas_nueva_a_mas_vieja() {
        // 0.16.9 vs 0.16.10: comparar como cadenas daría el orden equivocado.
        let raw = r#"[{"loader":{"version":"0.16.9"}},{"loader":{"version":"0.16.10"}}]"#;
        let versions = parse_versions(raw, "1.21.4").unwrap();
        assert_eq!(versions[0].id, "0.16.10");
        assert_eq!(versions[1].id, "0.16.9");
    }

    #[test]
    fn parsea_la_lista_de_quilt() {
        let raw = r#"[{"version":"0.27.1","separator":"."},{"version":"0.27.0-beta.1"}]"#;
        let versions = parse_versions(raw, "1.20.1").unwrap();
        assert_eq!(versions.len(), 2);
        assert!(versions.iter().any(|v| v.id == "0.27.1" && v.stable));
        assert!(versions.iter().any(|v| v.id == "0.27.0-beta.1" && !v.stable));
    }

    #[test]
    fn compara_versiones_numericas() {
        use std::cmp::Ordering::*;
        assert_eq!(compare_versions("0.16.10", "0.16.9"), Greater);
        assert_eq!(compare_versions("1.0.0", "1.0"), Equal);
        assert_eq!(compare_versions("0.15.0", "0.16.0"), Less);
    }
}
