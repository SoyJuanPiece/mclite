//! Comandos informativos: `versions`, `java`, `loaders`, `instances`,
//! `crashes` y `open`.

use super::Context;
use crate::cli::{collect_logs, Cli};
use mclite::core::{instance::InstanceStore, manifest::VersionFilter, shell};

pub(crate) const HELP: &str = r#"McLite — launcher lite de Minecraft

USO:
  mclite <comando> [opciones]

COMANDOS:
  versions              Lista las versiones de Minecraft (oficiales por defecto).
  java                  Muestra los Java detectados en el sistema.
  loaders <mc>          Lista las versiones de cargador compatibles con <mc>.
  install <mc>          Descarga y deja lista una versión (sin lanzarla).
  new <nombre> <mc>     Crea una instancia (y la instala, salvo --no-install).
  instances             Lista las instancias guardadas.
  launch <mc|instancia> Lanza el juego.
  open                  Abre la carpeta de datos en el explorador.
  crashes               Lista los logs de crash guardados.
  help                  Esta ayuda.

OPCIONES:
  --loader <k>          vanilla | fabric | quilt | forge | neoforge | optifine
  --loader-version <v>  Versión del cargador. Por defecto, la última estable.
  --user <nick>         Cuenta offline (3-16 caracteres).
  --ram <mb>            Memoria para el juego (por defecto 4096).
  --width/--height <n>  Resolución de la ventana.
  --root <dir>          Raíz de datos (por defecto %APPDATA%\mclite).
  --java <ruta>         Java concreto, en vez de autodetectarlo.
  --snapshots           Incluye snapshots en las listas (por defecto no).
  --old                 Incluye beta/alpha históricas.
  --dry-run             Resuelve y muestra el plan, sin descargar nada pesado.
  --no-assets           Omite los assets (en 1.21 son ~1 GB).
  --no-install          (new) Crea la instancia sin descargar el juego.
  --renderer <modo>     Backend de la ventana: auto | gl | wgpu-gl.
  --gl                  Atajo de --renderer gl (si la UI se ve sin letras).

EJEMPLOS:
  mclite versions --snapshots
  mclite loaders 1.21.4 --loader fabric
  mclite new "Mi mundo" 1.21.4 --loader fabric
  mclite launch 1.21.4 --user Steve
"#;

/// Límite de líneas impresas en las listas largas.
const LIST_LIMIT: usize = 30;

fn filter(cli: &Cli) -> VersionFilter {
    cli.version_filter()
}

/// Versiones oficiales (+ snapshots/betas si se piden).
pub(crate) fn versions(cli: &Cli) -> mclite::Result<()> {
    let ctx = Context::new(cli)?;
    let manifest = mclite::core::install::fetch_manifest(&ctx.http, &ctx.paths)?;
    let grouped = manifest.grouped(&filter(cli));

    println!(
        "Última oficial: {}   ·   último snapshot: {}\n",
        manifest.latest.release, manifest.latest.snapshot
    );

    println!("Oficiales ({})", grouped.releases.len());
    for entry in &grouped.releases {
        let tag = if manifest.is_latest_release(&entry.id) {
            "  ← última"
        } else {
            ""
        };
        println!("  {}{tag}", entry.id);
    }

    if cli.has("snapshots") {
        println!("\nSnapshots ({})", grouped.snapshots.len());
        for entry in grouped.snapshots.iter().take(LIST_LIMIT) {
            let tag = if manifest.is_latest_snapshot(&entry.id) {
                "  ← último"
            } else {
                ""
            };
            println!("  {}{tag}", entry.id);
        }
        print_hidden(grouped.snapshots.len(), LIST_LIMIT);
    } else {
        println!("\n(los snapshots están ocultos: usa --snapshots)");
    }

    if cli.has("old") {
        println!("\nBeta/Alpha ({})", grouped.old.len());
        for entry in grouped.old.iter().take(LIST_LIMIT) {
            println!("  {}", entry.id);
        }
        print_hidden(grouped.old.len(), LIST_LIMIT);
    }

    Ok(())
}

/// Java instalados que el launcher puede usar.
pub(crate) fn java() -> mclite::Result<()> {
    let found = mclite::core::java::detect_all();
    if found.is_empty() {
        println!("No encontré ningún Java instalado.");
        println!("Las versiones modernas piden Java 17 o 21 (la fase 3 baja el runtime de Mojang).");
        return Ok(());
    }
    println!("Java detectados ({}):", found.len());
    for installation in &found {
        println!(
            "  Java {} — {} [{}]",
            installation.major,
            installation.path.display(),
            installation.source
        );
    }
    Ok(())
}

/// Versiones de un cargador para una versión de Minecraft.
pub(crate) fn loaders(cli: &Cli) -> mclite::Result<()> {
    let mc = cli
        .positional
        .first()
        .cloned()
        .or_else(|| cli.flag("mc").map(str::to_string))
        .ok_or_else(|| mclite::Error::Missing("uso: mclite loaders <mc>".into()))?;

    let kind = cli.loader()?;
    if kind == mclite::loaders::LoaderKind::Vanilla {
        return Err(mclite::Error::Unsupported(
            "Vanilla no tiene versiones de cargador: pasa --loader <k>".into(),
        ));
    }

    let ctx = Context::new(cli)?;
    let loader_ctx = mclite::loaders::LoaderCtx {
        http: &ctx.http,
        paths: &ctx.paths,
        progress: &ctx.progress,
        filter: filter(cli),
    };

    let versions = mclite::loaders::get(kind).list_versions(&loader_ctx, &mc)?;
    println!(
        "{} para Minecraft {mc} ({} versiones):\n",
        kind.label(),
        versions.len()
    );
    for version in versions.iter().take(LIST_LIMIT + 10) {
        let tag = if version.stable { "" } else { "  (inestable)" };
        println!("  {}{tag}", version.id);
    }
    print_hidden(versions.len(), LIST_LIMIT + 10);
    Ok(())
}

/// Instancias guardadas.
pub(crate) fn instances(cli: &Cli) -> mclite::Result<()> {
    let paths = cli.paths()?;
    let store = InstanceStore::load(&paths);
    if store.is_empty() {
        println!("No hay instancias. Crea una con: mclite new \"Mi mundo\" 1.21.4");
        return Ok(());
    }
    println!("Instancias en {}:\n", paths.root().display());
    for instance in &store.instances {
        let last_played = instance
            .last_played
            .as_ref()
            .map(|when| format!("  (última vez {when})"))
            .unwrap_or_default();
        println!(
            "  {} — {} {} RAM {} MB{last_played}",
            instance.slug,
            instance.version_id(),
            instance.loader.label(),
            instance.ram_clamped(),
        );
    }
    Ok(())
}

/// Expedientes de crash, más recientes primero.
pub(crate) fn crashes(cli: &Cli) -> mclite::Result<()> {
    let paths = cli.paths()?;
    let root = paths.crash_logs_root();
    let entries = collect_logs(&root);

    if entries.is_empty() {
        println!("No hay logs de crash en {}", root.display());
        println!("En cuanto una partida falle, su expediente aparecerá aquí.");
        return Ok(());
    }

    println!("Logs de crash en {}:\n", root.display());
    for path in entries.iter().take(LIST_LIMIT + 10) {
        let rel = path.strip_prefix(&root).unwrap_or(path);
        println!("  {}", rel.display());
    }
    print_hidden(entries.len(), LIST_LIMIT + 10);
    println!("\nSesiones de juego (OK y CRASH): {}", paths.game_logs_root().display());
    println!("Logs de mods:                   {}", paths.mod_logs_root().display());
    Ok(())
}

/// Abre la carpeta de datos en el explorador del sistema.
pub(crate) fn open_folder(cli: &Cli) -> mclite::Result<()> {
    let paths = cli.paths()?;
    paths.ensure()?;
    shell::open_in_explorer(paths.root())
}

/// Aviso «… y N más» cuando una lista se cortó.
fn print_hidden(total: usize, shown: usize) {
    if total > shown {
        println!("  … y {} más", total - shown);
    }
}
