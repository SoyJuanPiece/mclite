//! Cargadores (Vanilla, Fabric, Quilt, Forge, NeoForge, OptiFine).
//!
//! La interfaz es a propósito mínima: listar versiones del cargador y **devolver un
//! `VersionJson` ya fusionado con el vanilla** (`inheritsFrom` resuelto). A partir de
//! ahí, `install.rs` y `launch.rs` tratan a los seis igual, y por eso el resto del
//! launcher no tiene ni un `if` de cargador.

use serde::{Deserialize, Serialize};

use crate::core::error::{Error, Result};
use crate::core::http::HttpClient;
use crate::core::install::InstallOptions;
use crate::core::manifest::VersionFilter;
use crate::core::paths::Paths;
use crate::core::progress::Progress;
use crate::core::version_json::VersionJson;

pub mod fabric;
pub mod forge;
pub mod optifine;
pub mod vanilla;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoaderKind {
    Vanilla,
    Fabric,
    Quilt,
    Forge,
    NeoForge,
    OptiFine,
}

pub const ALL_KINDS: [LoaderKind; 6] = [
    LoaderKind::Vanilla,
    LoaderKind::Fabric,
    LoaderKind::Quilt,
    LoaderKind::Forge,
    LoaderKind::NeoForge,
    LoaderKind::OptiFine,
];

impl LoaderKind {
    pub fn label(&self) -> &'static str {
        match self {
            LoaderKind::Vanilla => "Vanilla",
            LoaderKind::Fabric => "Fabric",
            LoaderKind::Quilt => "Quilt",
            LoaderKind::Forge => "Forge",
            LoaderKind::NeoForge => "NeoForge",
            LoaderKind::OptiFine => "OptiFine",
        }
    }

    /// Clave estable para `config.json` / la CLI (`--loader fabric`).
    pub fn key(&self) -> &'static str {
        match self {
            LoaderKind::Vanilla => "vanilla",
            LoaderKind::Fabric => "fabric",
            LoaderKind::Quilt => "quilt",
            LoaderKind::Forge => "forge",
            LoaderKind::NeoForge => "neoforge",
            LoaderKind::OptiFine => "optifine",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        let key = raw.trim().to_ascii_lowercase();
        ALL_KINDS.into_iter().find(|kind| kind.key() == key)
    }

    /// ¿Ya está implementado? La UI deshabilita (no oculta) lo que falta, así el
    /// usuario ve que está previsto.
    pub fn is_implemented(&self) -> bool {
        true
    }

    /// Nota para la UI cuando la combinación no tiene sentido.
    pub fn note(&self) -> Option<&'static str> {
        match self {
            LoaderKind::Vanilla | LoaderKind::Fabric => None,
            LoaderKind::Quilt => Some("Quilt va un paso por detrás de Fabric en versiones nuevas."),
            LoaderKind::Forge => Some("La primera instalación ejecuta el instalador oficial de Forge; tarda un poco más."),
            LoaderKind::NeoForge => Some("Solo existe desde Minecraft 1.20.2."),
            LoaderKind::OptiFine => {
                Some("OptiFine es un parche del client jar. Con Fabric se usa OptiFabric o Sodium.")
            }
        }
    }
}

/// Una versión concreta de un cargador.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoaderVersion {
    /// `0.19.5` | `54.1.6` | `HD_U_J3`
    pub id: String,
    /// Marcada como estable por el propio cargador.
    pub stable: bool,
    /// Versión de Minecraft a la que corresponde.
    pub mc: String,
}

pub struct LoaderCtx<'a> {
    pub http: &'a HttpClient,
    pub paths: &'a Paths,
    pub progress: &'a Progress,
    /// El toggle de snapshots también aplica al listado de versiones de Minecraft.
    pub filter: VersionFilter,
}

pub trait Loader {
    fn kind(&self) -> LoaderKind;

    /// Versiones del cargador compatibles con `mc`, más nuevas primero.
    fn list_versions(&self, ctx: &LoaderCtx<'_>, mc: &str) -> Result<Vec<LoaderVersion>>;

    /// Manifiesto final de la instancia, **ya con la herencia resuelta**.
    /// `version_id` es el id con el que se guardará en `versions/<id>/`.
    /// `opts` hace falta para los cargadores que ejecutan instaladores (Forge,
    /// OptiFine): ahí se decide el Java a usar y se respetan dry-run/no-assets.
    fn resolve(
        &self,
        ctx: &LoaderCtx<'_>,
        mc: &str,
        loader_version: Option<&str>,
        version_id: &str,
        opts: &InstallOptions,
    ) -> Result<VersionJson>;

    /// Versiones de Minecraft para las que este cargador tiene algo. `None` =
    /// «no lo sé» (no filtrar la lista de la GUI: vanilla, o consulta fallida).
    fn supported_mc_versions(&self, _ctx: &LoaderCtx<'_>) -> Option<Vec<String>> {
        None
    }
}

/// Versiones de MC soportadas por el cargador (para filtrar la lista de la GUI).
pub fn supported_mcs(ctx: &LoaderCtx<'_>, kind: LoaderKind) -> Option<Vec<String>> {
    get(kind).supported_mc_versions(ctx)
}

/// Id de `versions/<id>/` y valor de `--version` para cada cargador.
pub fn version_id_for(kind: LoaderKind, mc: &str, loader_version: Option<&str>) -> String {
    let Some(loader) = loader_version.filter(|v| !v.trim().is_empty()) else {
        return mc.to_string();
    };
    match kind {
        LoaderKind::Vanilla => mc.to_string(),
        LoaderKind::Fabric => format!("fabric-loader-{loader}-{mc}"),
        LoaderKind::Quilt => format!("quilt-loader-{loader}-{mc}"),
        LoaderKind::Forge => format!("{mc}-forge-{loader}"),
        LoaderKind::NeoForge => format!("neoforge-{loader}"),
        LoaderKind::OptiFine => format!("OptiFine-{mc}-{loader}"),
    }
}

/// Devuelve el cargador pedido. Los que aún no están implementados se comportan
/// igual (listan y fallan con un mensaje claro) para que la UI y la CLI no tengan
/// que saber cuáles existen.
pub fn get(kind: LoaderKind) -> Box<dyn Loader> {
    match kind {
        LoaderKind::Vanilla => Box::new(vanilla::VanillaLoader),
        LoaderKind::Fabric => Box::new(fabric::FabricLoader { kind: LoaderKind::Fabric }),
        LoaderKind::Quilt => Box::new(fabric::FabricLoader { kind: LoaderKind::Quilt }),
        LoaderKind::Forge => Box::new(forge::ForgeLoader { kind: LoaderKind::Forge }),
        LoaderKind::NeoForge => Box::new(forge::ForgeLoader { kind: LoaderKind::NeoForge }),
        LoaderKind::OptiFine => Box::new(optifine::OptiFineLoader),
    }
}

/// Resuelve el manifiesto final de una instancia (vanilla + cargador).
pub fn resolve(
    http: &HttpClient,
    paths: &Paths,
    progress: &Progress,
    filter: VersionFilter,
    kind: LoaderKind,
    mc: &str,
    loader_version: Option<&str>,
    opts: &InstallOptions,
) -> Result<(String, VersionJson)> {
    if kind != LoaderKind::Vanilla && loader_version.is_none() {
        return Err(Error::Missing(format!(
            "falta la versión de {} (por ejemplo `--loader-version latest`)",
            kind.label()
        )));
    }
    let ctx = LoaderCtx {
        http,
        paths,
        progress,
        filter,
    };
    let version_id = version_id_for(kind, mc, loader_version);
    let version = get(kind).resolve(&ctx, mc, loader_version, &version_id, opts)?;
    Ok((version_id, version))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_de_version_por_cargador() {
        assert_eq!(version_id_for(LoaderKind::Vanilla, "1.21.4", None), "1.21.4");
        assert_eq!(
            version_id_for(LoaderKind::Fabric, "1.21.4", Some("0.19.5")),
            "fabric-loader-0.19.5-1.21.4"
        );
        assert_eq!(
            version_id_for(LoaderKind::Quilt, "1.21.4", Some("0.27.1")),
            "quilt-loader-0.27.1-1.21.4"
        );
        assert_eq!(
            version_id_for(LoaderKind::Forge, "1.20.1", Some("47.2.0")),
            "1.20.1-forge-47.2.0"
        );
        assert_eq!(
            version_id_for(LoaderKind::NeoForge, "1.21.4", Some("21.4.50")),
            "neoforge-21.4.50"
        );
        assert_eq!(
            version_id_for(LoaderKind::OptiFine, "1.21.4", Some("HD_U_J3")),
            "OptiFine-1.21.4-HD_U_J3"
        );
    }

    #[test]
    fn las_claves_van_y_vuelven() {
        for kind in ALL_KINDS {
            assert_eq!(LoaderKind::parse(kind.key()), Some(kind));
        }
        assert_eq!(LoaderKind::parse("Fabric"), Some(LoaderKind::Fabric));
        assert_eq!(LoaderKind::parse("neo-forge"), None);
    }

    #[test]
    fn los_seis_cargadores_estan_implementados() {
        for kind in ALL_KINDS {
            assert!(kind.is_implemented(), "{} no implementado", kind.label());
        }
    }
}
