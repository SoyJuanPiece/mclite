//! Armado de la línea de comandos y arranque del juego.
//!
//! Aquí se juntan el manifiesto (ya heredado, si hay cargador), las rutas locales y
//! la cuenta offline para producir un `LaunchPlan`. El plan es datos puros: se puede
//! imprimir, loguear o pasar a un test sin arrancar nada.
//!
//! Trampas del formato que están resueltas aquí a propósito:
//!
//! * **`-cp` ya viene en el manifiesto** (`arguments.jvm`), no se añade a mano.
//! * El **client jar va al final** del classpath.
//! * Las versiones **< 1.13** no traen `arguments`: usan `minecraftArguments` (una
//!   sola cadena) y hay que sintetizar los args de la JVM.
//! * `${resolution_width}`/`${resolution_height}` **siempre** se sustituyen; si no,
//!   queda un `--width` sin valor y el juego no arranca.
//! * `logging.client.argument` (`-Dlog4j.configurationFile=${path}`) lo añade el
//!   launcher, no está en `arguments.jvm`.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::Sender;

use crate::core::auth::OfflineAccount;
use crate::core::error::{Error, Result};
use crate::core::libraries::classpath_string;
use crate::core::paths::Paths;
use crate::core::process::game_command;
use crate::core::rules::{Environment, Features};
use crate::core::version_json::VersionJson;
use crate::{LAUNCHER_NAME, LAUNCHER_VERSION};

/// Todo lo que hace falta para construir la línea de comandos.
pub struct LaunchContext<'a> {
    /// Manifiesto final (vanilla o ya fusionado con el cargador).
    pub version: &'a VersionJson,
    /// Nombre con el que se lanza (`--version`), p. ej. `fabric-loader-0.19.5-1.21.4`.
    pub version_id: &'a str,
    pub account: &'a OfflineAccount,
    pub paths: &'a Paths,
    /// gameDir de la instancia.
    pub game_dir: &'a Path,
    pub java: &'a Path,
    /// Librerías resueltas. El client jar se añade al final aquí dentro.
    pub classpath: &'a [PathBuf],
    pub client_jar: PathBuf,
    pub natives_dir: PathBuf,
    /// `assets`/`assetIndex.id` del manifiesto.
    pub assets_index_name: String,
    /// Config de log4j ya descargada, si la versión la usa.
    pub logging_config: Option<PathBuf>,
    pub memory_mb: u32,
    pub width: u32,
    pub height: u32,
    pub extra_jvm_args: Vec<String>,
}

/// Línea de comandos ya resuelta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchPlan {
    pub program: PathBuf,
    pub main_class: String,
    pub jvm_args: Vec<String>,
    pub game_args: Vec<String>,
    pub game_dir: PathBuf,
}

impl LaunchPlan {
    pub fn command(&self) -> Command {
        let mut command = game_command(&self.program);
        command.args(&self.jvm_args);
        command.arg(&self.main_class);
        command.args(&self.game_args);
        command.current_dir(&self.game_dir);
        command.stdin(Stdio::null());
        command
    }

    /// Arranca el juego. Con `log` se reenvía stdout/stderr línea a línea al canal
    /// (es lo que alimenta el log colapsable de la UI).
    pub fn spawn(&self, log: Option<Sender<String>>) -> Result<Child> {
        let mut command = self.command();
        match &log {
            Some(_) => {
                command.stdout(Stdio::piped());
                command.stderr(Stdio::piped());
            }
            None => {
                command.stdout(Stdio::null());
                command.stderr(Stdio::null());
            }
        }

        let mut child = command.spawn().map_err(|e| {
            Error::Launch(format!("no pude ejecutar {}: {e}", self.program.display()))
        })?;

        if let Some(sender) = log {
            if let Some(stdout) = child.stdout.take() {
                forward(stdout, sender.clone());
            }
            if let Some(stderr) = child.stderr.take() {
                forward(stderr, sender);
            }
        }
        Ok(child)
    }

    /// La línea de comandos, ya con comillas para lo que tiene espacios. Sirve para
    /// el log y para depurar sin lanzar nada.
    pub fn display_string(&self) -> String {
        let mut parts = vec![quote(&self.program.display().to_string())];
        parts.extend(self.jvm_args.iter().map(|arg| quote(arg)));
        parts.push(self.main_class.clone());
        parts.extend(self.game_args.iter().map(|arg| quote(arg)));
        parts.join(" ")
    }
}

fn forward(stream: impl std::io::Read + Send + 'static, sender: Sender<String>) {
    use std::io::BufRead;
    std::thread::spawn(move || {
        let reader = std::io::BufReader::new(stream);
        for line in reader.lines().map_while(|line| line.ok()) {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
}

fn quote(value: &str) -> String {
    if value.contains(' ') {
        format!("\"{value}\"")
    } else {
        value.to_string()
    }
}

/// Construye el plan de lanzamiento.
pub fn build(ctx: &LaunchContext<'_>) -> Result<LaunchPlan> {
    if ctx.version.main_class.trim().is_empty() {
        return Err(Error::Missing(
            "mainClass: el manifiesto no dice qué clase arrancar".into(),
        ));
    }

    let env = Environment::current();
    // La resolución se pasa siempre, así que activamos el feature que hace que el
    // manifiesto emita `--width`/`--height` con valores reales.
    let features = Features {
        has_custom_resolution: true,
        ..Features::default()
    };

    let mut classpath: Vec<PathBuf> = ctx.classpath.to_vec();
    classpath.push(ctx.client_jar.clone());
    let classpath = classpath_string(&classpath);

    let vars = placeholders(ctx, &classpath);

    let mut jvm_args: Vec<String> = Vec::new();
    let mut game_args: Vec<String> = Vec::new();

    let manifest_arguments = ctx.version.arguments.as_ref();

    match manifest_arguments {
        Some(arguments) if !arguments.jvm.is_empty() => {
            for arg in &arguments.jvm {
                for value in arg.expand(&env, &features) {
                    jvm_args.push(substitute(&value, &vars));
                }
            }
        }
        // Pre-1.13 (o un perfil de cargador que solo trae `minecraftArguments`).
        _ => {
            for value in legacy_jvm_args() {
                jvm_args.push(substitute(&value, &vars));
            }
        }
    }

    if let Some(arguments) = manifest_arguments {
        for arg in &arguments.game {
            for value in arg.expand(&env, &features) {
                game_args.push(substitute(&value, &vars));
            }
        }
    }

    if game_args.is_empty() {
        if let Some(legacy) = &ctx.version.minecraft_arguments {
            for token in legacy.split_whitespace() {
                game_args.push(substitute(token, &vars));
            }
            if !legacy.contains("--userProperties") {
                game_args.push("--userProperties".into());
                game_args.push("{}".into());
            }
        }
    }

    if game_args.is_empty() {
        return Err(Error::Missing(format!(
            "argumentos del juego de «{}»: no hay `arguments.game` ni `minecraftArguments`",
            ctx.version_id
        )));
    }

    // Args de JVM que añade el launcher (van antes de los del manifiesto).
    let mut launcher_jvm_args: Vec<String> = Vec::new();
    launcher_jvm_args.push(format!("-Xmx{}M", ctx.memory_mb.max(512)));

    if let Some(config) = &ctx.logging_config {
        let template = ctx
            .version
            .logging
            .as_ref()
            .and_then(|logging| logging.client.as_ref())
            .map(|client| client.argument.clone())
            .unwrap_or_else(|| "-Dlog4j.configurationFile=${path}".to_string());
        for token in template.replace("${path}", &config.display().to_string()).split_whitespace() {
            launcher_jvm_args.push(token.to_string());
        }
    }

    // Log4Shell: inofensivo en versiones nuevas, obligatorio en 1.7–1.16.
    if log4shell_risk(ctx.version_id) {
        launcher_jvm_args.push("-Dlog4j2.formatMsgNoLookups=true".into());
    }
    launcher_jvm_args.extend(ctx.extra_jvm_args.iter().cloned());
    launcher_jvm_args.append(&mut jvm_args);

    Ok(LaunchPlan {
        program: ctx.java.to_path_buf(),
        main_class: ctx.version.main_class.clone(),
        jvm_args: launcher_jvm_args,
        game_args,
        game_dir: ctx.game_dir.to_path_buf(),
    })
}

/// Argumentos de JVM de las versiones antiguas, que no los traen en el manifiesto.
fn legacy_jvm_args() -> Vec<&'static str> {
    vec![
        "-Djava.library.path=${natives_directory}",
        "-cp",
        "${classpath}",
        "-Dminecraft.launcher.brand=${launcher_name}",
        "-Dminecraft.launcher.version=${launcher_version}",
    ]
}

/// Tabla de sustituciones. Se aplica en este orden, y como cada clave incluye las
/// llaves (`${...}`) no hay riesgo de que una clave pise a otra.
fn placeholders(ctx: &LaunchContext<'_>, classpath: &str) -> Vec<(&'static str, String)> {
    let separator = if cfg!(windows) { ";" } else { ":" };
    vec![
        ("${auth_player_name}", ctx.account.username.clone()),
        ("${version_name}", ctx.version_id.to_string()),
        ("${game_directory}", ctx.game_dir.display().to_string()),
        ("${assets_root}", ctx.paths.assets().display().to_string()),
        ("${assets_index_name}", ctx.assets_index_name.clone()),
        ("${auth_uuid}", ctx.account.uuid.clone()),
        ("${auth_access_token}", ctx.account.access_token.clone()),
        // Algunas versiones antiguas piden `--session` en vez de `--accessToken`.
        ("${auth_session}", ctx.account.access_token.clone()),
        ("${user_type}", ctx.account.user_type().to_string()),
        (
            "${version_type}",
            ctx.version
                .version_type
                .clone()
                .unwrap_or_else(|| "release".to_string()),
        ),
        ("${natives_directory}", ctx.natives_dir.display().to_string()),
        ("${classpath}", classpath.to_string()),
        // Las usan Forge y algunos perfiles antiguos.
        ("${classpath_separator}", separator.to_string()),
        ("${library_directory}", ctx.paths.libraries().display().to_string()),
        ("${launcher_name}", LAUNCHER_NAME.to_string()),
        ("${launcher_version}", LAUNCHER_VERSION.to_string()),
        ("${resolution_width}", ctx.width.to_string()),
        ("${resolution_height}", ctx.height.to_string()),
        ("${user_properties}", "{}".to_string()),
        ("${profile_name}", LAUNCHER_NAME.to_string()),
        (
            "${game_assets}",
            ctx.paths.assets().join("virtual").join("legacy").display().to_string(),
        ),
        // Cuenta offline: estos no tienen equivalente y van vacíos. El launcher
        // oficial también los manda vacíos cuando no hay sesión de Microsoft.
        ("${clientid}", String::new()),
        ("${auth_xuid}", String::new()),
        ("${xuid}", String::new()),
        ("${user_properties}", "{}".to_string()),
    ]
}

fn substitute(raw: &str, vars: &[(&'static str, String)]) -> String {
    let mut out = raw.to_string();
    for (key, value) in vars {
        if out.contains(key) {
            out = out.replace(key, value);
        }
    }
    out
}

/// ¿Hay que blindar el log4j de esta versión? Los 1.7–1.16 son vulnerables a
/// Log4Shell; a partir de 1.17 Mojang ya actualizó la librería.
pub fn log4shell_risk(version_id: &str) -> bool {
    let mut parts = version_id.split(['.', '-', '_']);
    let major = parts.next().and_then(|p| p.parse::<u32>().ok());
    let minor = parts.next().and_then(|p| p.parse::<u32>().ok());
    matches!((major, minor), (Some(1), Some(minor)) if (7..=16).contains(&minor))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths() -> Paths {
        Paths::with_root("/tmp/mclite-launch-test")
    }

    fn account() -> OfflineAccount {
        OfflineAccount::new("Steve").unwrap()
    }

    fn context<'a>(
        version: &'a VersionJson,
        account: &'a OfflineAccount,
        paths: &'a Paths,
        classpath: &'a [PathBuf],
    ) -> LaunchContext<'a> {
        LaunchContext {
            version,
            version_id: &version.id,
            account,
            paths,
            game_dir: Path::new("/tmp/game"),
            java: Path::new("/jdk/bin/java"),
            classpath,
            client_jar: PathBuf::from("/libs/client.jar"),
            natives_dir: PathBuf::from("/natives"),
            assets_index_name: "19".into(),
            logging_config: None,
            memory_mb: 4096,
            width: 854,
            height: 480,
            extra_jvm_args: Vec::new(),
        }
    }

    #[test]
    fn arma_una_version_moderna() {
        let version = VersionJson::from_str(
            r#"{
                "id": "1.21.4",
                "mainClass": "net.minecraft.client.main.Main",
                "assets": "19",
                "arguments": {
                    "jvm": ["-Djava.library.path=${natives_directory}", "-cp", "${classpath}"],
                    "game": ["--username", "${auth_player_name}", "--version", "${version_name}",
                             "--gameDir", "${game_directory}", "--assetsDir", "${assets_root}",
                             "--assetIndex", "${assets_index_name}", "--uuid", "${auth_uuid}",
                             "--accessToken", "${auth_access_token}", "--clientId", "${clientid}",
                             "--xuid", "${auth_xuid}", "--userType", "${user_type}"]
                }
            }"#,
        )
        .unwrap();

        let paths = paths();
        let account = account();
        let classpath = vec![PathBuf::from("/libs/a.jar"), PathBuf::from("/libs/b.jar")];
        let plan = build(&context(&version, &account, &paths, &classpath)).unwrap();

        assert_eq!(plan.main_class, "net.minecraft.client.main.Main");
        assert_eq!(plan.program, PathBuf::from("/jdk/bin/java"));
        // Xmx primero y `-cp` tal cual venía del manifiesto.
        assert_eq!(plan.jvm_args[0], "-Xmx4096M");
        assert!(plan.jvm_args.contains(&"-cp".to_string()));

        // El client jar va al final del classpath.
        let cp = plan
            .jvm_args
            .iter()
            .position(|arg| arg == "-cp")
            .map(|index| plan.jvm_args[index + 1].clone())
            .unwrap();
        assert!(cp.ends_with("client.jar"), "classpath: {cp}");
        assert!(cp.contains("a.jar") && cp.contains("b.jar"));

        // No queda ningún placeholder.
        assert!(!plan.jvm_args.concat().contains("${"));
        assert!(!plan.game_args.concat().contains("${"));

        // Y los args del juego llevan los valores reales.
        let username = plan.game_args.iter().position(|a| a == "--username").unwrap();
        assert_eq!(plan.game_args[username + 1], "Steve");
        let uuid = plan.game_args.iter().position(|a| a == "--uuid").unwrap();
        assert_eq!(plan.game_args[uuid + 1], account.uuid);
        let assets = plan.game_args.iter().position(|a| a == "--assetsDir").unwrap();
        assert_eq!(
            plan.game_args[assets + 1],
            paths.assets().display().to_string()
        );
    }

    #[test]
    fn sintetiza_los_args_de_una_version_antigua() {
        let version = VersionJson::from_str(
            r#"{
                "id": "1.8.9",
                "mainClass": "net.minecraft.client.main.Main",
                "assets": "1.8",
                "minecraftArguments": "--username ${auth_player_name} --version ${version_name} --gameDir ${game_directory} --assetsDir ${assets_root} --assetIndex ${assets_index_name} --uuid ${auth_uuid} --accessToken ${auth_access_token} --userProperties ${user_properties} --userType ${user_type} --width ${resolution_width} --height ${resolution_height}"
            }"#,
        )
        .unwrap();

        let paths = paths();
        let account = account();
        let plan = build(&context(&version, &account, &paths, &[])).unwrap();

        // Sin `arguments` hay que fabricar los de la JVM.
        assert!(plan.jvm_args.iter().any(|a| a == "-Djava.library.path=/natives"));
        let cp = plan.jvm_args.iter().position(|a| a == "-cp").unwrap();
        assert!(plan.jvm_args[cp + 1].ends_with("client.jar"));
        // Y siempre hay -Xmx.
        assert_eq!(plan.jvm_args[0], "-Xmx4096M");
        // `--width` con valor, que es lo que rompe el arranque si falta.
        let width = plan.game_args.iter().position(|a| a == "--width").unwrap();
        assert_eq!(plan.game_args[width + 1], "854");
        // `--userProperties {}` no se duplica.
        assert_eq!(
            plan.game_args.iter().filter(|a| *a == "--userProperties").count(),
            1
        );
        assert!(!plan.game_args.concat().contains("${"));
    }

    #[test]
    fn el_log_de_log4j_se_pasa_como_argumento() {
        let version = VersionJson::from_str(
            r#"{
                "id": "1.12.2",
                "mainClass": "net.minecraft.client.main.Main",
                "minecraftArguments": "--username ${auth_player_name}",
                "logging": {"client": {"argument": "-Dlog4j.configurationFile=${path}",
                    "file": {"id": "client-1.12", "url": "http://x"}, "type": "log4j2-xml"}}
            }"#,
        )
        .unwrap();

        let paths = paths();
        let account = account();
        let mut ctx = context(&version, &account, &paths, &[]);
        ctx.logging_config = Some(PathBuf::from("/configs/client-1.12.xml"));
        let plan = build(&ctx).unwrap();

        assert!(plan
            .jvm_args
            .contains(&"-Dlog4j.configurationFile=/configs/client-1.12.xml".to_string()));
        // 1.12 es vulnerable a Log4Shell.
        assert!(plan.jvm_args.contains(&"-Dlog4j2.formatMsgNoLookups=true".to_string()));
    }

    #[test]
    fn detecta_las_versiones_con_log4shell() {
        assert!(log4shell_risk("1.7.10"));
        assert!(log4shell_risk("1.12.2"));
        assert!(log4shell_risk("1.16.5"));
        assert!(!log4shell_risk("1.17.1"));
        assert!(!log4shell_risk("1.21.4"));
        assert!(!log4shell_risk("26.3"));
        assert!(!log4shell_risk("b1.7.3"));
    }

    #[test]
    fn sin_argumentos_de_juego_es_un_error_explicito() {
        let version = VersionJson::from_str(r#"{"id":"raro","mainClass":"X"}"#).unwrap();
        let paths = paths();
        let account = account();
        let err = build(&context(&version, &account, &paths, &[])).unwrap_err();
        assert!(err.to_string().contains("argumentos del juego"));
    }

    #[test]
    fn sin_main_class_tambien() {
        let version = VersionJson::from_str(
            r#"{"id":"raro","minecraftArguments":"--username ${auth_player_name}"}"#,
        )
        .unwrap();
        let paths = paths();
        let account = account();
        let err = build(&context(&version, &account, &paths, &[])).unwrap_err();
        assert!(err.to_string().contains("mainClass"));
    }
}
