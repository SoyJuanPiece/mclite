//! Configuración global del launcher (`config.json`).
//!
//! Es lo que NO es por instancia: el nick, el Java elegido, la RAM por defecto y el
//! filtro de versiones. Se guarda en la raíz de `Paths` porque se comparte entre
//! todas las instancias.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::core::error::{Error, Result};
use crate::core::http;
use crate::core::manifest::VersionFilter;
use crate::core::paths::Paths;

pub const DEFAULT_USERNAME: &str = "Player";
pub const DEFAULT_RAM_MB: u32 = 4096;
pub const MIN_RAM_MB: u32 = 512;
pub const MAX_RAM_MB: u32 = 32768;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LauncherConfig {
    /// Nick de la cuenta offline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// Client ID de Azure para el login con cuenta Microsoft (device code).
    /// `None` = la UI de MSA no se muestra. Ver docs/MICROSOFT-ACCOUNT.md.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub msa_client_id: Option<String>,
    #[serde(default = "default_ram")]
    pub ram_mb: u32,
    /// Toggle de snapshots. Apagado por defecto.
    #[serde(default)]
    pub show_snapshots: bool,
    /// Beta/alpha históricas. Apagado por defecto.
    #[serde(default)]
    pub show_old_versions: bool,
    /// Java elegido a mano. Si es `None`, se autodetecta.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java_path: Option<PathBuf>,
    /// Si no hay un Java del sistema que cubra lo que pide la versión, bajar el
    /// runtime oficial de Mojang (fase 3). Activado por defecto.
    #[serde(default = "default_true")]
    pub use_mojang_runtime: bool,
    /// Conexiones simultáneas de descarga.
    #[serde(default = "default_threads")]
    pub threads: usize,
    /// Última instancia seleccionada (para reabrirla al arrancar).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_instance: Option<String>,
    /// Último tamaño/posición de la ventana, para restaurar la sesión.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<WindowSize>,
    /// Color de acento de la interfaz: clave del preset ("green", "blue",
    /// "violet", "rose", "amber"). `None` = verde por defecto.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent: Option<String>,
    /// Avisar de versiones nuevas consultando GitHub al arrancar.
    #[serde(default = "default_true")]
    pub check_updates: bool,
}

/// Tamaño de ventana guardado entre sesiones.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WindowSize {
    pub width: f32,
    pub height: f32,
}

impl Default for WindowSize {
    fn default() -> Self {
        Self {
            width: 980.0,
            height: 620.0,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_ram() -> u32 {
    DEFAULT_RAM_MB
}

fn default_threads() -> usize {
    http::DEFAULT_THREADS
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            username: None,
            msa_client_id: None,
            ram_mb: DEFAULT_RAM_MB,
            show_snapshots: false,
            show_old_versions: false,
            java_path: None,
            use_mojang_runtime: true,
            threads: http::DEFAULT_THREADS,
            last_instance: None,
            window: None,
            accent: None,
            check_updates: true,
        }
    }
}

impl LauncherConfig {
    /// Lee `config.json`. Un fichero ausente o corrupto no es un error fatal:
    /// se devuelven los valores por defecto (y el fichero se reescribe al guardar).
    pub fn load(paths: &Paths) -> Self {
        match std::fs::read_to_string(paths.config_file()) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        paths.ensure()?;
        let file = paths.config_file();
        let raw = serde_json::to_string_pretty(self)?;
        std::fs::write(&file, raw).map_err(|e| Error::io(&file, e))
    }

    pub fn username_or_default(&self) -> String {
        self.username
            .clone()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_USERNAME.to_string())
    }

    /// El toggle de snapshots de la UI, que es exactamente el filtro del core.
    pub fn version_filter(&self) -> VersionFilter {
        VersionFilter {
            show_snapshots: self.show_snapshots,
            show_old: self.show_old_versions,
        }
    }

    pub fn clamped_ram(&self) -> u32 {
        self.ram_mb.clamp(MIN_RAM_MB, MAX_RAM_MB)
    }
}

/// Reemplazaste el exe y "desapareció" el usuario? Casi siempre es esto: el
/// navegador guardó el exe nuevo en OTRA carpeta (p. ej. `Descargas\mclite.exe`
/// en vez de `Descargas\mclite\`), así que la raíz portable cambia y ahí no hay
/// config. Migración: si la raíz actual está virgen, buscamos configs de
/// instalaciones anteriores cerca (carpeta anidada, carpeta hermana, %APPDATA%)
/// y copiamos config + índice de instancias. Devuelve de dónde se importó.
pub fn migrate_previous(paths: &Paths) -> Option<String> {
    if paths.config_file().is_file() || paths.instances_file().is_file() {
        return None; // aquí ya hay datos: nada que migrar
    }

    let root = paths.root();
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    // Caso clásico: exe nuevo descargado a la carpeta PADRE de la data vieja
    // (Descargas\mclite.exe con data en Descargas\mclite\mclite\).
    if let Some(child) = root.join("mclite").canonicalize().ok() {
        candidates.push(child);
    }
    // Caso inverso: exe nuevo en una subcarpeta de la data vieja.
    if let Some(parent) = root.parent() {
        if parent != root {
            candidates.push(parent.to_path_buf());
        }
    }
    // Y la época pre-portable.
    if let Some(appdata) = dirs::data_dir() {
        candidates.push(appdata.join("mclite"));
    }

    for candidate in candidates {
        if !candidate.join("config.json").is_file() && !candidate.join("instances.json").is_file()
        {
            continue;
        }
        if candidate == root {
            continue;
        }
        if paths.ensure().is_err() {
            continue;
        }
        for name in ["config.json", "instances.json"] {
            let from = candidate.join(name);
            if from.is_file() {
                let _ = std::fs::copy(&from, root.join(name));
            }
        }
        return Some(candidate.display().to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> Paths {
        let root = std::env::temp_dir().join(format!("mclite-config-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        Paths::with_root(root)
    }

    #[test]
    fn sin_fichero_usa_los_defaults() {
        let paths = temp_root("defaults");
        let config = LauncherConfig::load(&paths);
        assert_eq!(config.ram_mb, DEFAULT_RAM_MB);
        // El requisito del launcher: snapshots apagados por defecto.
        assert!(!config.show_snapshots);
        assert!(!config.version_filter().show_snapshots);
        assert_eq!(config.username_or_default(), "Player");
    }

    #[test]
    fn guarda_y_relee() {
        let paths = temp_root("roundtrip");
        let config = LauncherConfig {
            username: Some("Steve".into()),
            show_snapshots: true,
            ram_mb: 8192,
            ..Default::default()
        };
        config.save(&paths).unwrap();
        assert_eq!(LauncherConfig::load(&paths), config);
    }

    #[test]
    fn un_json_roto_no_rompe_el_arranque() {
        let paths = temp_root("roto");
        paths.ensure().unwrap();
        std::fs::write(paths.config_file(), "{no es json").unwrap();
        assert_eq!(LauncherConfig::load(&paths), LauncherConfig::default());
    }

    #[test]
    fn la_ram_se_limita() {
        let config = LauncherConfig {
            ram_mb: 999_999,
            ..Default::default()
        };
        assert_eq!(config.clamped_ram(), MAX_RAM_MB);
    }

    #[test]
    fn migra_la_config_de_la_carpeta_anidada() {
        // Caso real: el exe nuevo queda en <root> y la data vieja en <root>/mclite.
        let root = std::env::temp_dir().join("mclite-config-migra");
        let _ = std::fs::remove_dir_all(&root);
        let old = root.join("mclite");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(
            old.join("config.json"),
            r#"{"username":"Alex","ram_mb":6144}"#,
        )
        .unwrap();
        std::fs::write(old.join("instances.json"), r#"{"instances":[]}"#).unwrap();

        let paths = Paths::with_root(&root);
        let from = migrate_previous(&paths);
        assert!(from.is_some(), "debería importar de {}", old.display());

        // El nick (y el resto) sobrevive al reemplazo del exe.
        let config = LauncherConfig::load(&paths);
        assert_eq!(config.username.as_deref(), Some("Alex"));
        assert_eq!(config.ram_mb, 6144);
        assert!(paths.instances_file().is_file());

        // Segunda llamada: ya hay config aquí, no vuelve a migrar.
        assert!(migrate_previous(&paths).is_none());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn no_migra_si_la_raiz_ya_tiene_datos() {
        let paths = temp_root("sin-migra");
        paths.ensure().unwrap();
        LauncherConfig::default().save(&paths).unwrap();
        assert!(migrate_previous(&paths).is_none());
    }

    #[test]
    fn check_updates_por_defecto_activo_y_relee() {
        let paths = temp_root("check-updates");
        assert!(LauncherConfig::load(&paths).check_updates);
        let mut config = LauncherConfig::default();
        config.check_updates = false;
        config.save(&paths).unwrap();
        assert!(!LauncherConfig::load(&paths).check_updates);
    }

    #[test]
    fn el_acento_se_guarda_y_relee() {
        let paths = temp_root("accento");
        let mut config = LauncherConfig::default();
        config.accent = Some("violet".into());
        config.save(&paths).unwrap();
        assert_eq!(LauncherConfig::load(&paths).accent.as_deref(), Some("violet"));
        // Una config vieja sin el campo sigue cargando (default None).
        let paths2 = temp_root("accento-viejo");
        LauncherConfig::default().save(&paths2).unwrap();
        assert_eq!(LauncherConfig::load(&paths2).accent, None);
    }
}
