//! Comandos que dejan cosas listas en disco: `install` y `new`.

use super::Context;
use crate::cli::{human_mb, Cli};
use mclite::core::{
    install::{download, Installed},
    instance::{Instance, InstanceStore},
    loaders::{self, LoaderKind, LoaderVersion},
    version_json::VersionJson,
};

/// Descarga y deja lista una versión de Minecraft (con su cargador).
pub(crate) fn install(cli: &Cli) -> mclite::Result<()> {
    let mc = cli.required_positional("uso: mclite install <mc>")?;
    let ctx = Context::new(cli)?;
    let kind = cli.loader()?;
    let loader_version = pick_loader_version(cli, &ctx, kind, &mc)?;
    let opts = cli.install_options();

    let (version_id, version) = resolve(cli, &ctx, kind, &mc, loader_version.as_deref(), &opts)?;
    let installed = download(
        &ctx.http,
        &ctx.paths,
        &version,
        &version_id,
        &opts,
        &ctx.progress,
    )?;

    print_installed(&version_id, &installed, &opts);
    Ok(())
}

/// Resuelve la versión final (manifiesto + cargador) según las banderas.
fn resolve(
    cli: &Cli,
    ctx: &Context,
    kind: LoaderKind,
    mc: &str,
    loader_version: Option<&str>,
    opts: &mclite::core::install::InstallOptions,
) -> mclite::Result<(String, VersionJson)> {
    loaders::resolve(
        &ctx.http,
        &ctx.paths,
        &ctx.progress,
        cli.version_filter(),
        kind,
        mc,
        loader_version,
        opts,
    )
}

/// Resumen de lo que quedó en disco tras una instalación.
fn print_installed(version_id: &str, installed: &Installed, opts: &mclite::core::install::InstallOptions) {
    println!("\n✔ {version_id} listo");
    println!("  main class      {}", installed.version.main_class);
    println!("  client jar      {}", installed.client_jar.display());
    println!("  librerías       {}", installed.classpath.len());
    println!("  natives         {}", installed.natives_dir.display());
    println!(
        "  assets          {} ({})",
        installed.assets_index_name,
        human_mb(installed.assets_total_bytes)
    );
    println!("  assets nuevos   {}", installed.assets_pending);
    if let Some(log) = &installed.log_config {
        println!("  config de log   {}", log.display());
    }
    if opts.dry_run {
        println!("\n(--dry-run: no se descargó nada pesado)");
    }
}

/// Crea una instancia (y la instala, salvo `--no-install`).
pub(crate) fn new_instance(cli: &Cli) -> mclite::Result<()> {
    let name = cli.required_positional("uso: mclite new <nombre> <mc>")?;
    let mc = cli
        .positional
        .get(1)
        .cloned()
        .ok_or_else(|| mclite::Error::Missing("uso: mclite new <nombre> <mc>".into()))?;

    let ctx = Context::new(cli)?;
    let kind = cli.loader()?;
    let loader_version = pick_loader_version(cli, &ctx, kind, &mc)?;

    let slug = create_instance(cli, &ctx, &name, &mc, kind, loader_version.clone())?;
    if cli.has("no-install") {
        return Ok(());
    }

    let opts = cli.install_options();
    let (version_id, version) = resolve(cli, &ctx, kind, &mc, loader_version.as_deref(), &opts)?;
    download(
        &ctx.http,
        &ctx.paths,
        &version,
        &version_id,
        &opts,
        &ctx.progress,
    )?;
    println!("\n✔ {version_id} instalado. Lanza con: mclite launch {slug}");
    Ok(())
}

/// Guarda la instancia con las opciones de la CLI; devuelve su slug.
fn create_instance(
    cli: &Cli,
    ctx: &Context,
    name: &str,
    mc: &str,
    kind: LoaderKind,
    loader_version: Option<String>,
) -> mclite::Result<String> {
    let mut store = InstanceStore::load(&ctx.paths);
    let mut instance = Instance::new(name, mc, kind);
    instance.loader_version = loader_version;
    if let Some(ram) = cli.number("ram") {
        instance.ram_mb = ram;
    }
    instance.width = cli.number("width").unwrap_or(instance.width);
    instance.height = cli.number("height").unwrap_or(instance.height);
    instance.java_path = cli.forced_java();

    let slug = store.add(instance, &ctx.paths)?;
    store.save(&ctx.paths)?;
    println!("✔ instancia «{name}» creada como {slug}");
    println!("  gameDir  {}", ctx.paths.instance_dir(&slug).display());
    Ok(slug)
}

/// Última versión estable del cargador (o la primera si no hay ninguna marcada).
/// `None` para vanilla, que no tiene versiones propias.
fn pick_loader_version(
    cli: &Cli,
    ctx: &Context,
    kind: LoaderKind,
    mc: &str,
) -> mclite::Result<Option<String>> {
    if kind == LoaderKind::Vanilla {
        return Ok(None);
    }
    if let Some(explicit) = cli.flag("loader-version") {
        return Ok(Some(explicit.to_string()));
    }

    let loader_ctx = loaders::LoaderCtx {
        http: &ctx.http,
        paths: &ctx.paths,
        progress: &ctx.progress,
        filter: cli.version_filter(),
    };
    let versions: Vec<LoaderVersion> = loaders::get(kind).list_versions(&loader_ctx, mc)?;
    let chosen = versions
        .iter()
        .find(|version| version.stable)
        .or_else(|| versions.first())
        .ok_or_else(|| {
            mclite::Error::Unsupported(format!(
                "{} no publica ninguna versión para Minecraft {mc}",
                kind.label()
            ))
        })?;

    println!("  {} {} para Minecraft {mc}", kind.label(), chosen.id);
    Ok(Some(chosen.id.clone()))
}
