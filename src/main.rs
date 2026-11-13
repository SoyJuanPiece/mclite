//! McLite — punto de entrada del binario.
//!
//! El ejecutable tiene dos caras que comparten la misma biblioteca:
//!  - GUI (`feature = "gui"`): sin argumentos, abre la ventana.
//!  - CLI: cualquier comando (`versions`, `launch`, …) usa exactamente las
//!    mismas funciones del core que la GUI; es la forma de automatizar y de
//!    verificar el flujo completo desde consola.
//!
//! Toda la lógica vive en `mclite::core` / `mclite::loaders`; aquí solo se
//! decide qué cara mostrar. Los detalles de la CLI están en `src/cli/`.

// En Windows, la build de release no debe dejar consola huérfana al abrir la
// ventana (egui pinta la UI); en debug sí se quiere consola para la CLI.
#![cfg_attr(
    all(feature = "gui", windows, not(debug_assertions)),
    windows_subsystem = "windows"
)]

use std::process::ExitCode;

#[cfg(not(feature = "gui"))]
use cli::HELP;

mod cli;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    #[cfg(feature = "gui")]
    if let Some(code) = gui_code(&args) {
        return code;
    }

    #[cfg(not(feature = "gui"))]
    if args.is_empty() {
        println!("{}", HELP);
        return ExitCode::SUCCESS;
    }

    to_exit_code(cli::run(&args))
}

/// ¿Estos argumentos piden abrir la ventana? Sin argumentos, o solo con
/// banderas de render (`--renderer <modo>`, `--gl`). Devuelve el código de
/// salida de la GUI una vez cerrada, o `None` si hay que ir a la CLI.
#[cfg(feature = "gui")]
fn gui_code(args: &[String]) -> Option<ExitCode> {
    if !cli::report::wants_gui(args) {
        return None;
    }
    Some(mclite::app::run(renderer_from_args(args)))
}

/// Modo de render pedido en la línea de órdenes. `None` = no se dijo nada:
/// mandan `MCLITE_RENDERER` y luego `config.json` (ver `app::run`).
#[cfg(feature = "gui")]
fn renderer_from_args(args: &[String]) -> Option<mclite::render::Mode> {
    let cli = cli::Cli::parse(args);
    if cli.has("gl") {
        return Some(mclite::render::Mode::Gl);
    }
    cli.flag("renderer")
        .filter(|raw| !raw.trim().is_empty())
        .and_then(mclite::render::Mode::parse)
}

/// Éxito silencioso o error impreso en stderr con código de fallo.
fn to_exit_code(result: mclite::Result<()>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("\nerror: {err}");
            ExitCode::FAILURE
        }
    }
}
