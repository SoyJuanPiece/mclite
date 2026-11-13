//! Comandos de la CLI, uno por función corta. Cada comando abre su propio
//! `Context` (rutas + HTTP + progreso) y delega todo el trabajo en `mclite::core`.

mod info;
mod install;
mod launch;

use super::Cli;
use mclite::core::{http::HttpClient, paths::Paths, progress::Progress};

pub(crate) use info::HELP;

/// Todo lo que un comando necesita, ya construido. Evita repetir tres líneas
/// idénticas en cada `cmd_*` y garantiza contadores de progreso limpios.
pub(crate) struct Context {
    pub paths: Paths,
    pub http: HttpClient,
    pub progress: Progress,
}

impl Context {
    fn new(cli: &Cli) -> mclite::Result<Self> {
        Ok(Self {
            paths: cli.paths()?,
            http: HttpClient::new(),
            progress: super::progress::cli_progress(),
        })
    }
}

/// Tabla de comandos → handler. Añadir uno es una línea aquí y una función.
pub(crate) fn dispatch(cli: &Cli) -> mclite::Result<()> {
    match cli.command.as_str() {
        "help" | "ayuda" => {
            println!("{HELP}");
            Ok(())
        }
        "versions" => info::versions(cli),
        "java" => info::java(),
        "loaders" => info::loaders(cli),
        "instances" => info::instances(cli),
        "crashes" => info::crashes(cli),
        "open" => info::open_folder(cli),
        "install" => install::install(cli),
        "new" => install::new_instance(cli),
        "launch" => launch::launch(cli),
        other => Err(mclite::Error::Unsupported(format!(
            "comando desconocido «{other}». Prueba con `mclite help`."
        ))),
    }
}
