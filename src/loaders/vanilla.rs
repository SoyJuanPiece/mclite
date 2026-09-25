//! Vanilla: las versiones oficiales de Mojang.
//!
//! "Listar versiones del cargador" aquí significa listar versiones de Minecraft, y
//! el filtro de snapshots es el mismo `VersionFilter` que usa el resto del launcher.

use crate::core::error::Result;
use crate::core::install::{self, InstallOptions};
use crate::core::version_json::VersionJson;
use crate::loaders::{Loader, LoaderCtx, LoaderKind, LoaderVersion};

pub struct VanillaLoader;

impl Loader for VanillaLoader {
    fn kind(&self) -> LoaderKind {
        LoaderKind::Vanilla
    }

    fn list_versions(&self, ctx: &LoaderCtx<'_>, _mc: &str) -> Result<Vec<LoaderVersion>> {
        let manifest = install::fetch_manifest(ctx.http, ctx.paths)?;
        Ok(manifest
            .versions
            .iter()
            .filter(|entry| ctx.filter.allows(entry.version_type))
            .map(|entry| LoaderVersion {
                id: entry.id.clone(),
                stable: entry.version_type == crate::core::manifest::VersionType::Release,
                mc: entry.id.clone(),
            })
            .collect())
    }

    fn resolve(
        &self,
        ctx: &LoaderCtx<'_>,
        mc: &str,
        _loader_version: Option<&str>,
        version_id: &str,
        _opts: &InstallOptions,
    ) -> Result<VersionJson> {
        install::resolve_vanilla(ctx.http, ctx.paths, ctx.progress, mc, version_id)
    }
}
