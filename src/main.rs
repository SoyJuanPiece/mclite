//! McLite — CLI (y arranque de la GUI).
//!
//! La GUI (feature `gui`) es la cara del launcher, pero toda la lógica vive en la
//! biblioteca. Esta CLI usa exactamente las mismas funciones que usa la GUI: es la
//! forma de verificar el flujo completo en cualquier sistema y de dejar el launcher
//! usable desde consola o desde un script. Sin argumentos, `main` abre la ventana.

// En Windows, la build de release no debe dejar consola huérfana al abrir la
// ventana (egui pinta la UI); en debug sí se quiere consola para la CLI.
#![cfg_attr(
    all(feature = "gui", windows, not(debug_assertions)),
    windows_subsystem = "windows"
)]

use std::collections::BTreeMap;
use std::process::ExitCode;
use std::sync::atomic::{AtomicUsize, Ordering};

use mclite::core::config::LauncherConfig;
use mclite::core::http::HttpClient;
use mclite::core::install::{self, InstallOptions, PlayRequest};
use mclite::core::instance::{Instance, InstanceStore};
use mclite::core::paths::Paths;
use mclite::core::progress::{Progress, ProgressEvent, ProgressSink};
use mclite::core::rules::Environment;
use mclite::core::shell;
use mclite::loaders::{self, LoaderCtx, LoaderKind, LoaderVersion};

const HELP: &str = r#"McLite — launcher lite de Minecraft

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

EJEMPLOS:
  mclite versions --snapshots
  mclite loaders 1.21.4 --loader fabric
  mclite new "Mi mundo" 1.21.4 --loader fabric
  mclite launch 1.21.4 --user Steve
"#;

/// Flags que consumen el argumento siguiente. El resto son booleanos.
const FLAGS_WITH_VALUE: [&str; 10] = [
    "root",
    "loader",
    "loader-version",
    "user",
    "ram",
    "width",
    "height",
    "java",
    "name",
    "mc",
];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // Sin argumentos, la GUI (feature `gui`) es la cara del launcher; si se
    // compiló sin ella o se pasa cualquier argumento, manda la CLI. Así una
    // sola build sirve para uso interactivo y para scripts.
    if args.is_empty() {
        #[cfg(feature = "gui")]
        return mclite::app::run();
        #[cfg(not(feature = "gui"))]
        {
            println!("{HELP}");
            return ExitCode::SUCCESS;
        }
    }

    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("\nerror: {err}");
            ExitCode::FAILURE
        }
    }
}

struct Cli {
    command: String,
    positional: Vec<String>,
    flags: BTreeMap<String, String>,
}

impl Cli {
    fn parse(args: &[String]) -> Self {
        let mut command = "help".to_string();
        let mut positional = Vec::new();
        let mut flags = BTreeMap::new();
        let mut iter = args.iter().peekable();
        let mut seen_command = false;

        while let Some(arg) = iter.next() {
            if arg == "-h" || arg == "--help" {
                flags.insert("help".into(), String::new());
                continue;
            }
            if let Some(raw) = arg.strip_prefix("--") {
                let (name, value) = match raw.split_once('=') {
                    Some((name, value)) => (name.to_string(), value.to_string()),
                    None => {
                        let value = if FLAGS_WITH_VALUE.contains(&raw) {
                            iter.next().cloned().unwrap_or_default()
                        } else {
                            String::new()
                        };
                        (raw.to_string(), value)
                    }
                };
                flags.insert(name, value);
                continue;
            }
            if !seen_command {
                command = arg.clone();
                seen_command = true;
            } else {
                positional.push(arg.clone());
            }
        }

        Self {
            command,
            positional,
            flags,
        }
    }

    fn flag(&self, name: &str) -> Option<&str> {
        self.flags.get(name).map(String::as_str)
    }

    fn has(&self, name: &str) -> bool {
        self.flags.contains_key(name)
    }

    fn number(&self, name: &str) -> Option<u32> {
        self.flag(name).and_then(|value| value.parse().ok())
    }

    fn paths(&self) -> mclite::Result<Paths> {
        match self.flag("root") {
            Some(root) => Ok(Paths::with_root(root)),
            None => Paths::discover(),
        }
    }

    fn loader(&self) -> mclite::Result<LoaderKind> {
        match self.flag("loader") {
            Some(raw) => LoaderKind::parse(raw).ok_or_else(|| {
                mclite::Error::Unsupported(format!(
                    "cargador desconocido «{raw}»: usa vanilla, fabric, quilt, forge, neoforge u optifine"
                ))
            }),
            None => Ok(LoaderKind::Vanilla),
        }
    }
}

/// Progreso para consola: fases, avisos y un contador cada 25 ficheros.
struct CliSink {
    done: AtomicUsize,
}

impl ProgressSink for CliSink {
    fn emit(&self, event: ProgressEvent) {
        match event {
            ProgressEvent::Phase(phase) => println!("\n▸ {phase}"),
            ProgressEvent::Total(total) if total > 0 => {
                println!("  {total} ficheros por revisar");
            }
            ProgressEvent::Advance(step) => {
                let done = self.done.fetch_add(step as usize, Ordering::Relaxed) + step as usize;
                if done % 25 == 0 {
                    println!("  … {done}");
                }
            }
            ProgressEvent::Message(message) => println!("  · {message}"),
            _ => {}
        }
    }
}

fn cli_progress() -> Progress {
    Progress::new(CliSink {
        done: AtomicUsize::new(0),
    })
}

fn run(args: &[String]) -> mclite::Result<()> {
    let cli = Cli::parse(args);

    if cli.has("help") {
        println!("{HELP}");
        return Ok(());
    }

    match cli.command.as_str() {
        "help" | "ayuda" => {
            println!("{HELP}");
            Ok(())
        }
        "versions" => cmd_versions(&cli),
        "java" => cmd_java(&cli),
        "loaders" => cmd_loaders(&cli),
        "install" => cmd_install(&cli),
        "new" => cmd_new(&cli),
        "instances" => cmd_instances(&cli),
        "launch" => cmd_launch(&cli),
        "open" => {
            let paths = cli.paths()?;
            paths.ensure()?;
            shell::open_in_explorer(paths.root())
        }
        "crashes" => cmd_crashes(&cli),
        other => Err(mclite::Error::Unsupported(format!(
            "comando desconocido «{other}». Prueba con `mclite help`."
        ))),
    }
}

fn filter(cli: &Cli) -> mclite::core::manifest::VersionFilter {
    mclite::core::manifest::VersionFilter {
        show_snapshots: cli.has("snapshots"),
        show_old: cli.has("old"),
    }
}

fn cmd_versions(cli: &Cli) -> mclite::Result<()> {
    let paths = cli.paths()?;
    let http = HttpClient::new();
    let manifest = install::fetch_manifest(&http, &paths)?;
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
        for entry in grouped.snapshots.iter().take(30) {
            let tag = if manifest.is_latest_snapshot(&entry.id) {
                "  ← último"
            } else {
                ""
            };
            println!("  {}{tag}", entry.id);
        }
        if grouped.snapshots.len() > 30 {
            println!("  … y {} más", grouped.snapshots.len() - 30);
        }
    } else {
        println!("\n(los snapshots están ocultos: usa --snapshots)");
    }

    if cli.has("old") {
        println!("\nBeta/Alpha ({})", grouped.old.len());
        for entry in grouped.old.iter().take(20) {
            println!("  {}", entry.id);
        }
    }

    Ok(())
}

fn cmd_java(_cli: &Cli) -> mclite::Result<()> {
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

fn cmd_loaders(cli: &Cli) -> mclite::Result<()> {
    let mc = cli
        .positional
        .first()
        .cloned()
        .or_else(|| cli.flag("mc").map(str::to_string))
        .ok_or_else(|| mclite::Error::Missing("uso: mclite loaders <mc>".into()))?;

    let kind = cli.loader()?;
    if kind == LoaderKind::Vanilla {
        return Err(mclite::Error::Unsupported(
            "Vanilla no tiene versiones de cargador: pasa --loader <k>".into(),
        ));
    }

    let paths = cli.paths()?;
    let http = HttpClient::new();
    let progress = cli_progress();
    let ctx = LoaderCtx {
        http: &http,
        paths: &paths,
        progress: &progress,
        filter: filter(cli),
    };

    let versions = loaders::get(kind).list_versions(&ctx, &mc)?;
    println!("{} para Minecraft {mc} ({} versiones):\n", kind.label(), versions.len());
    for version in versions.iter().take(40) {
        let tag = if version.stable { "" } else { "  (inestable)" };
        println!("  {}{tag}", version.id);
    }
    if versions.len() > 40 {
        println!("  … y {} más", versions.len() - 40);
    }
    Ok(())
}

/// Última versión estable del cargador (o la primera si no hay ninguna marcada).
fn pick_loader_version(
    cli: &Cli,
    paths: &Paths,
    http: &HttpClient,
    progress: &Progress,
    kind: LoaderKind,
    mc: &str,
) -> mclite::Result<Option<String>> {
    if kind == LoaderKind::Vanilla {
        return Ok(None);
    }
    if let Some(explicit) = cli.flag("loader-version") {
        return Ok(Some(explicit.to_string()));
    }
    let ctx = LoaderCtx {
        http,
        paths,
        progress,
        filter: filter(cli),
    };
    let versions: Vec<LoaderVersion> = loaders::get(kind).list_versions(&ctx, mc)?;
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
    println!(
        "  {} {} para Minecraft {mc}",
        kind.label(),
        chosen.id
    );
    Ok(Some(chosen.id.clone()))
}

fn install_options(cli: &Cli) -> InstallOptions {
    InstallOptions {
        dry_run: cli.has("dry-run"),
        skip_assets: cli.has("no-assets"),
        ..InstallOptions::default()
    }
}

fn cmd_install(cli: &Cli) -> mclite::Result<()> {
    let mc = cli
        .positional
        .first()
        .cloned()
        .ok_or_else(|| mclite::Error::Missing("uso: mclite install <mc>".into()))?;
    let paths = cli.paths()?;
    let http = HttpClient::new();
    let progress = cli_progress();
    let kind = cli.loader()?;
    let loader_version = pick_loader_version(cli, &paths, &http, &progress, kind, &mc)?;

    let opts = install_options(cli);

    let (version_id, version) = loaders::resolve(
        &http,
        &paths,
        &progress,
        filter(cli),
        kind,
        &mc,
        loader_version.as_deref(),
        &opts,
    )?;
    let installed = install::download(&http, &paths, &version, &version_id, &opts, &progress)?;

    println!("\n✔ {version_id} listo");
    println!("  main class      {}", installed.version.main_class);
    println!("  client jar      {}", installed.client_jar.display());
    println!("  librerías       {}", installed.classpath.len());
    println!("  natives         {}", installed.natives_dir.display());
    println!("  assets          {} ({})", installed.assets_index_name, mb(installed.assets_total_bytes));
    println!("  assets nuevos   {}", installed.assets_pending);
    if let Some(log) = &installed.log_config {
        println!("  config de log   {}", log.display());
    }
    if opts.dry_run {
        println!("\n(--dry-run: no se descargó nada pesado)");
    }
    Ok(())
}

fn cmd_new(cli: &Cli) -> mclite::Result<()> {
    let name = cli
        .positional
        .first()
        .cloned()
        .ok_or_else(|| mclite::Error::Missing("uso: mclite new <nombre> <mc>".into()))?;
    let mc = cli
        .positional
        .get(1)
        .cloned()
        .ok_or_else(|| mclite::Error::Missing("uso: mclite new <nombre> <mc>".into()))?;

    let paths = cli.paths()?;
    let http = HttpClient::new();
    let progress = cli_progress();
    let kind = cli.loader()?;
    let loader_version = pick_loader_version(cli, &paths, &http, &progress, kind, &mc)?;

    let mut store = InstanceStore::load(&paths);
    let mut instance = Instance::new(&name, &mc, kind);
    instance.loader_version = loader_version.clone();
    if let Some(ram) = cli.number("ram") {
        instance.ram_mb = ram;
    }
    instance.width = cli.number("width").unwrap_or(instance.width);
    instance.height = cli.number("height").unwrap_or(instance.height);
    if let Some(java) = cli.flag("java") {
        instance.java_path = Some(java.into());
    }

    let slug = store.add(instance, &paths)?;
    store.save(&paths)?;
    println!("✔ instancia «{name}» creada como {slug}");
    println!("  gameDir  {}", paths.instance_dir(&slug).display());

    if cli.has("no-install") {
        return Ok(());
    }

    let opts = install_options(cli);
    let (version_id, version) = loaders::resolve(
        &http,
        &paths,
        &progress,
        filter(cli),
        kind,
        &mc,
        loader_version.as_deref(),
        &opts,
    )?;
    install::download(&http, &paths, &version, &version_id, &opts, &progress)?;
    println!("\n✔ {version_id} instalado. Lanza con: mclite launch {slug}");
    Ok(())
}

fn cmd_instances(cli: &Cli) -> mclite::Result<()> {
    let paths = cli.paths()?;
    let store = InstanceStore::load(&paths);
    if store.is_empty() {
        println!("No hay instancias. Crea una con: mclite new \"Mi mundo\" 1.21.4");
        return Ok(());
    }
    println!("Instancias en {}:\n", paths.root().display());
    for instance in &store.instances {
        println!(
            "  {} — {} {} RAM {} MB{}",
            instance.slug,
            instance.version_id(),
            instance.loader.label(),
            instance.ram_clamped(),
            instance
                .last_played
                .as_ref()
                .map(|when| format!("  (última vez {when})"))
                .unwrap_or_default()
        );
    }
    Ok(())
}

fn cmd_launch(cli: &Cli) -> mclite::Result<()> {
    let target = cli
        .positional
        .first()
        .cloned()
        .ok_or_else(|| mclite::Error::Missing("uso: mclite launch <mc|instancia>".into()))?;

    let paths = cli.paths()?;
    let http = HttpClient::new();
    let progress = cli_progress();
    let config = LauncherConfig::load(&paths);
    let mut store = InstanceStore::load(&paths);

    // Si el objetivo es una instancia guardada, salen de ahí la versión, el cargador y
    // el gameDir; si no, se interpreta como una versión de Minecraft suelta.
    let existing = if cli.flag("loader").is_some() {
        None
    } else {
        store.find_loose(&target).cloned()
    };

    let (kind, mc, loader_version, game_dir, ram, width, height, java, instance_slug) =
        match &existing {
            Some(instance) => (
                instance.loader,
                instance.mc_version.clone(),
                instance.loader_version.clone(),
                instance.game_dir(&paths),
                instance.ram_clamped(),
                instance.width,
                instance.height,
                instance.java_path.clone().or_else(|| config.java_path.clone()),
                Some(instance.slug.clone()),
            ),
            None => (
                cli.loader()?,
                target.clone(),
                pick_loader_version(cli, &paths, &http, &progress, cli.loader()?, &target)?,
                paths.root().join("instances").join(&target),
                cli.number("ram").unwrap_or_else(|| config.clamped_ram()),
                cli.number("width").unwrap_or(480 * 2),
                cli.number("height").unwrap_or(270 * 2),
                cli.flag("java").map(Into::into),
                None,
            ),
        };

    std::fs::create_dir_all(&game_dir)
        .map_err(|e| mclite::Error::io(&game_dir, e))?;

    let username = cli
        .flag("user")
        .map(str::to_string)
        .unwrap_or_else(|| config.username_or_default());

    let request = PlayRequest {
        kind,
        mc_version: mc,
        loader_version,
        game_dir: game_dir.clone(),
        username,
        // La CLI lanza offline (sin login Microsoft).
        account: None,
        memory_mb: ram,
        width,
        height,
        java: java.clone(),
        extra_jvm_args: Vec::new(),
        filter: filter(cli),
    };

    let opts = install_options(cli);

    println!("McLite {} · {}", mclite::LAUNCHER_VERSION, Environment::current().os_name);
    mclite::core::logging::init(&paths);
    let prepared = install::prepare(&http, &paths, &request, &opts, &progress)?;

    println!("\n▸ Línea de comandos ({} {})", prepared.version_id, prepared.account.username);
    println!("{}", prepared.plan.display_string());

    if opts.dry_run {
        println!("\n(--dry-run: no se lanzó el juego)");
        return Ok(());
    }

    let (sender, receiver) = std::sync::mpsc::channel();
    let mut child = prepared.plan.spawn(Some(sender))?;
    for line in receiver {
        println!("{line}");
    }
    let status = child
        .wait()
        .map_err(|e| mclite::Error::Launch(e.to_string()))?;
    println!("\nel juego terminó con {status}");

    if let Some(slug) = instance_slug {
        if let Some(instance) = store.instances.iter_mut().find(|i| i.slug == slug) {
            instance.last_played = Some(mclite::core::instance::now());
            let _ = store.save(&paths);
        }
    }

    Ok(())
}

fn cmd_crashes(cli: &Cli) -> mclite::Result<()> {
    let paths = cli.paths()?;
    let dir = paths.logs().join("crash");
    let entries: Vec<_> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "log"))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if entries.is_empty() {
        println!("No hay logs de crash en {}", dir.display());
        return Ok(());
    }
    println!("Logs de crash en {}:\n", dir.display());
    for path in entries {
        println!("  {}", path.display());
    }
    Ok(())
}

fn mb(bytes: u64) -> String {
    format!("{:.0} MB", bytes as f64 / 1_048_576.0)
}
