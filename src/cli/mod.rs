//! Parser de argumentos y utilidades compartidas por la CLI.
//!
//! La CLI es una capa fina sobre la biblioteca `mclite`: no duplica lógica,
//! solo traduce argumentos a llamadas del core. Este módulo concentra esa
//! traducción (banderas, posiciones, filtros) para que los comandos sean
//! funciones cortas y legibles.

mod args;
mod commands;
pub(crate) mod progress;
pub(crate) mod report;

/// Ayuda de la CLI, reexportada para el arranque sin GUI.
pub(crate) use commands::HELP;

pub use args::Cli;
pub use report::{collect_logs, human_mb};

use mclite::core::manifest::VersionFilter;

impl Cli {
    /// Filtro de versiones deducido de las banderas `--snapshots` / `--old`.
    pub fn version_filter(&self) -> VersionFilter {
        VersionFilter {
            show_snapshots: self.has("snapshots"),
            show_old: self.has("old"),
        }
    }
}

/// Ejecuta la CLI con `args` ya recortados (sin el nombre del programa).
pub(crate) fn run(args: &[String]) -> mclite::Result<()> {
    let cli = Cli::parse(args);

    if cli.has("help") {
        println!("{}", commands::HELP);
        return Ok(());
    }

    commands::dispatch(&cli)
}

#[cfg(test)]
mod tests {
    use super::Cli;

    #[test]
    fn command_and_positionals() {
        let cli = Cli::parse(&["new".into(), "Mi mundo".into(), "1.21.4".into()]);
        assert_eq!(cli.command, "new");
        assert_eq!(cli.positional, ["Mi mundo", "1.21.4"]);
    }

    #[test]
    fn no_args_means_help() {
        let cli = Cli::parse(&[]);
        assert_eq!(cli.command, "help");
    }

    #[test]
    fn flags_with_value_and_equals() {
        let cli = Cli::parse(&[
            "launch".into(),
            "1.21.4".into(),
            "--ram".into(),
            "8192".into(),
            "--user=Steve".into(),
            "--dry-run".into(),
        ]);
        assert_eq!(cli.flag("ram"), Some("8192"));
        assert_eq!(cli.number("ram"), Some(8192));
        assert_eq!(cli.flag("user"), Some("Steve"));
        assert!(cli.has("dry-run"));
    }

    #[test]
    fn help_flag_anywhere() {
        let cli = Cli::parse(&["--help".into()]);
        assert!(cli.has("help"));
    }

    #[test]
    fn bad_number_is_none_not_panic() {
        let cli = Cli::parse(&["install".into(), "--ram".into(), "mucho".into()]);
        assert_eq!(cli.number("ram"), None);
    }

    #[test]
    fn required_positional_errors_with_usage() {
        let cli = Cli::parse(&["install".into()]);
        let err = cli.required_positional("uso: mclite install <mc>").unwrap_err();
        assert!(err.to_string().contains("uso: mclite install <mc>"));
    }
}
