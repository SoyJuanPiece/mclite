//! Parser manual de argumentos: sin dependencias, orden determinista.

use std::collections::BTreeMap;
use std::path::PathBuf;

use mclite::core::install::InstallOptions;
use mclite::core::manifest::VersionFilter;
use mclite::core::paths::Paths;
use mclite::loaders::LoaderKind;

/// Banderas que consumen el argumento siguiente. El resto son booleanas.
const FLAGS_WITH_VALUE: &[&str] = &[
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
    "renderer",
];

/// Argumentos ya clasificados: comando, posiciones y banderas.
pub struct Cli {
    pub command: String,
    pub positional: Vec<String>,
    pub flags: BTreeMap<String, String>,
}

impl Cli {
    /// Clasifica `args` en comando + posicionales + banderas.
    ///
    /// Soporta `--flag`, `--flag valor` (si está en `FLAGS_WITH_VALUE`) y la
    /// forma compacta `--flag=valor`. `-h`/`--help` se guardan como bandera
    /// `help` para poder responder desde cualquier posición.
    pub fn parse(args: &[String]) -> Self {
        let mut command = String::new();
        let mut positional = Vec::new();
        let mut flags = BTreeMap::new();
        let mut iter = args.iter();

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

            // El primer no-flag es el comando; el resto, posicionales.
            if command.is_empty() {
                command = arg.clone();
            } else {
                positional.push(arg.clone());
            }
        }

        if command.is_empty() {
            command = "help".to_string();
        }

        Self {
            command,
            positional,
            flags,
        }
    }

    /// Valor literal de una bandera (`Some("")` si es booleana presente).
    pub fn flag(&self, name: &str) -> Option<&str> {
        self.flags.get(name).map(String::as_str)
    }

    /// ¿Está la bandera presente (con o sin valor)?
    pub fn has(&self, name: &str) -> bool {
        self.flags.contains_key(name)
    }

    /// Bandera numérica; `None` si falta o no parsesable (sin pánico).
    pub fn number(&self, name: &str) -> Option<u32> {
        self.flag(name).and_then(|value| value.parse().ok())
    }

    /// Primer posicional no vacío, con el mensaje de uso del llamador.
    pub fn required_positional(&self, usage: &str) -> mclite::Result<String> {
        self.positional
            .first()
            .filter(|value| !value.trim().is_empty())
            .cloned()
            .ok_or_else(|| mclite::Error::Missing(usage.into()))
    }

    /// Raíz de datos: `--root` explícito o la descubierta por el sistema.
    pub fn paths(&self) -> mclite::Result<Paths> {
        match self.flag("root") {
            Some(root) => Ok(Paths::with_root(root)),
            None => Paths::discover(),
        }
    }

    /// Cargador pedido con `--loader` (vanilla si no se dijo nada).
    pub fn loader(&self) -> mclite::Result<LoaderKind> {
        match self.flag("loader") {
            Some(raw) => LoaderKind::parse(raw).ok_or_else(|| {
                mclite::Error::Unsupported(format!(
                    "cargador desconocido «{raw}»: usa vanilla, fabric, quilt, forge, neoforge u optifine"
                ))
            }),
            None => Ok(LoaderKind::Vanilla),
        }
    }

    /// Opciones de instalación deducidas de las banderas.
    pub fn install_options(&self) -> InstallOptions {
        InstallOptions {
            dry_run: self.has("dry-run"),
            skip_assets: self.has("no-assets"),
            ..InstallOptions::default()
        }
    }

    /// Java forzado con `--java`, si se indicó.
    pub fn forced_java(&self) -> Option<PathBuf> {
        self.flag("java").map(PathBuf::from)
    }

    /// Filtro de versiones deducido de las banderas `--snapshots` / `--old`.
    pub fn version_filter(&self) -> VersionFilter {
        VersionFilter {
            show_snapshots: self.has("snapshots"),
            show_old: self.has("old"),
        }
    }
}
