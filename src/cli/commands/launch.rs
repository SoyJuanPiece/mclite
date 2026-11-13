//! Comando `launch`: resuelve la instancia (o versión suelta), prepara el
//! juego y lo ejecuta en primer plano retransmitiendo su salida.

use super::{install::pick_loader_version, Context};
use crate::cli::Cli;
use mclite::core::{
    config::LauncherConfig,
    install::{prepare, InstallOptions, PlayRequest, Prepared},
    instance::{now, Instance, InstanceStore},
    loaders::LoaderKind,
    rules::Environment,
};
use std::path::PathBuf;

/// Resolución del objetivo de `launch`: una instancia guardada o una versión
/// suelta con los valores por defecto del config.
struct Target {
    kind: LoaderKind,
    mc_version: String,
    loader_version: Option<String>,
    game_dir: PathBuf,
    memory_mb: u32,
    width: u32,
    height: u32,
    java: Option<PathBuf>,
    slug: Option<String>,
}

pub(crate) fn launch(cli: &Cli) -> mclite::Result<()> {
    let target_name = cli.required_positional("uso: mclite launch <mc|instancia>")?;

    let ctx = Context::new(cli)?;
    let config = LauncherConfig::load(&ctx.paths);
    let mut store = InstanceStore::load(&ctx.paths);

    let target = resolve_target(cli, &ctx, &config, &mut store, &target_name)?;

    std::fs::create_dir_all(&target.game_dir)
        .map_err(|e| mclite::Error::io(&target.game_dir, e))?;

    let request = PlayRequest {
        kind: target.kind,
        mc_version: target.mc_version.clone(),
        loader_version: target.loader_version.clone(),
        game_dir: target.game_dir.clone(),
        username: cli
            .flag("user")
            .map(str::to_string)
            .unwrap_or_else(|| config.username_or_default()),
        // La CLI lanza offline (sin login Microsoft).
        account: None,
        memory_mb: target.memory_mb,
        width: target.width,
        height: target.height,
        java: target.java.clone(),
        extra_jvm_args: Vec::new(),
        optimize_jvm: true,
        filter: cli.version_filter(),
    };

    let opts = cli.install_options();
    println!(
        "McLite {} · {}",
        mclite::LAUNCHER_VERSION,
        Environment::current().os_name
    );
    mclite::core::logging::init(&ctx.paths);

    let prepared = prepare(&ctx.http, &ctx.paths, &request, &opts, &ctx.progress)?;

    println!(
        "\n▸ Línea de comandos ({} {})",
        prepared.version_id, prepared.account.username
    );
    println!("{}", prepared.plan.display_string());

    if opts.dry_run {
        println!("\n(--dry-run: no se lanzó el juego)");
        return Ok(());
    }

    run_game(&prepared)?;
    remember_played(&mut store, &ctx, target.slug.as_deref());
    Ok(())
}

/// ¿De dónde salen los parámetros: de una instancia o de las banderas sueltas?
fn resolve_target(
    cli: &Cli,
    ctx: &Context,
    config: &LauncherConfig,
    store: &mut InstanceStore,
    needle: &str,
) -> mclite::Result<Target> {
    // Con `--loader` explícito se fuerza la interpretación como versión suelta.
    let existing = if cli.flag("loader").is_some() {
        None
    } else {
        store.find_loose(needle).cloned()
    };

    if let Some(instance) = existing.as_ref() {
        return Ok(target_from_instance(ctx, config, instance));
    }

    let kind = cli.loader()?;
    let loader_version = pick_loader_version(cli, ctx, kind, needle)?;
    Ok(Target {
        kind,
        mc_version: needle.to_string(),
        loader_version,
        game_dir: ctx.paths.instances().join(needle),
        memory_mb: cli.number("ram").unwrap_or_else(|| config.clamped_ram()),
        width: cli.number("width").unwrap_or(DEFAULT_WIDTH),
        height: cli.number("height").unwrap_or(DEFAULT_HEIGHT),
        java: cli.forced_java().or_else(|| config.java_path.clone()),
        slug: None,
    })
}

const DEFAULT_WIDTH: u32 = 960;
const DEFAULT_HEIGHT: u32 = 540;

fn target_from_instance(ctx: &Context, config: &LauncherConfig, instance: &Instance) -> Target {
    Target {
        kind: instance.loader,
        mc_version: instance.mc_version.clone(),
        loader_version: instance.loader_version.clone(),
        game_dir: instance.game_dir(ctx.paths),
        memory_mb: instance.ram_clamped(),
        width: instance.width,
        height: instance.height,
        java: instance
            .java_path
            .clone()
            .or_else(|| config.java_path.clone()),
        slug: Some(instance.slug.clone()),
    }
}

/// Lanza el proceso y espera a que termine, imprimiendo su salida.
fn run_game(prepared: &Prepared) -> mclite::Result<()> {
    let (sender, receiver) = std::sync::mpsc::channel();
    let mut child = prepared.plan.spawn(Some(sender))?;
    for line in receiver {
        println!("{line}");
    }
    let status = child
        .wait()
        .map_err(|e| mclite::Error::Launch(e.to_string()))?;
    println!("\nel juego terminó con {status}");
    Ok(())
}

/// Marca la instancia como recién jugada (mejor esfuerzo).
fn remember_played(store: &mut InstanceStore, ctx: &Context, slug: Option<&str>) {
    let Some(slug) = slug else { return };
    if let Some(instance) = store.instances.iter_mut().find(|i| i.slug == slug) {
        instance.last_played = Some(now());
        let _ = store.save(&ctx.paths);
    }
}
