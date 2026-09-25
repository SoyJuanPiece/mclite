//! Instancias y su persistencia (`instances.json` + `instances/<slug>/instance.json`).
//!
//! Una instancia es lo que el usuario ve en la lista: nombre, versión de Minecraft,
//! cargador y ajustes de esa partida. Las librerías, assets y el runtime de Java son
//! **compartidos** entre instancias; lo único por instancia es el gameDir.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::core::config::{DEFAULT_RAM_MB, MAX_RAM_MB, MIN_RAM_MB};
use crate::core::error::{Error, Result};
use crate::core::paths::{sanitize, Paths};
use crate::loaders::LoaderKind;

pub const DEFAULT_WIDTH: u32 = 854;
pub const DEFAULT_HEIGHT: u32 = 480;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instance {
    pub name: String,
    /// Carpeta dentro de `instances/`. Se deriva del nombre y es única.
    pub slug: String,
    pub mc_version: String,
    pub loader: LoaderKind,
    /// Versión del cargador. `None` en Vanilla.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loader_version: Option<String>,
    #[serde(default = "default_ram")]
    pub ram_mb: u32,
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default = "default_height")]
    pub height: u32,
    /// Java de esta instancia (si no, el de la config global, si no el detectado).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_played: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

fn default_ram() -> u32 {
    DEFAULT_RAM_MB
}

fn default_width() -> u32 {
    DEFAULT_WIDTH
}

fn default_height() -> u32 {
    DEFAULT_HEIGHT
}

impl Instance {
    pub fn new(name: &str, mc_version: &str, loader: LoaderKind) -> Self {
        let slug = sanitize(name);
        Self {
            name: name.to_string(),
            slug,
            mc_version: mc_version.to_string(),
            loader,
            loader_version: None,
            ram_mb: DEFAULT_RAM_MB,
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            java_path: None,
            created: None,
            last_played: None,
            notes: None,
        }
    }

    /// Id bajo `versions/` y valor de `--version`. Tiene que coincidir con el id que
    /// publica el cargador, porque el juego lo muestra y algunos mods lo leen.
    pub fn version_id(&self) -> String {
        crate::loaders::version_id_for(self.loader, &self.mc_version, self.loader_version.as_deref())
    }

    pub fn game_dir(&self, paths: &Paths) -> PathBuf {
        paths.instance_dir(&self.slug)
    }

    /// Nombre sugerido para una instancia nueva: `Fabric 1.21.4`, `Vanilla 1.21.4`...
    pub fn suggested_name(loader: LoaderKind, mc_version: &str) -> String {
        match loader {
            LoaderKind::Vanilla => format!("Minecraft {mc_version}"),
            other => format!("{} {mc_version}", other.label()),
        }
    }

    pub fn ram_clamped(&self) -> u32 {
        self.ram_mb.clamp(MIN_RAM_MB, MAX_RAM_MB)
    }
}

/// Contenido de `instances.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceStore {
    #[serde(default)]
    pub instances: Vec<Instance>,
}

impl InstanceStore {
    pub fn load(paths: &Paths) -> Self {
        match std::fs::read_to_string(paths.instances_file()) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        paths.ensure()?;
        let file = paths.instances_file();
        let raw = serde_json::to_string_pretty(self)?;
        std::fs::write(&file, raw).map_err(|e| Error::io(&file, e))
    }

    pub fn find(&self, slug: &str) -> Option<&Instance> {
        self.instances.iter().find(|instance| instance.slug == slug)
    }

    /// Busca por slug y, si no, por nombre exacto. Es como resuelve la CLI.
    pub fn find_loose(&self, needle: &str) -> Option<&Instance> {
        self.find(needle).or_else(|| {
            self.instances
                .iter()
                .find(|instance| instance.name.eq_ignore_ascii_case(needle))
        })
    }

    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    /// Añade una instancia resolviendo un slug libre y creando su gameDir.
    pub fn add(&mut self, mut instance: Instance, paths: &Paths) -> Result<String> {
        instance.slug = self.unique_slug(&instance.slug);
        if instance.created.is_none() {
            instance.created = Some(now());
        }
        let slug = instance.slug.clone();

        let game_dir = paths.instance_dir(&slug);
        std::fs::create_dir_all(&game_dir).map_err(|e| Error::io(&game_dir, e))?;
        // Las carpetas que cualquier jugador espera encontrar al abrir la instancia.
        for dir in ["mods", "resourcepacks", "shaderpacks", "saves"] {
            let path = game_dir.join(dir);
            std::fs::create_dir_all(&path).map_err(|e| Error::io(&path, e))?;
        }

        // Copia del manifiesto dentro del gameDir: así la instancia se puede mover
        // de carpeta sin perder de qué versión es.
        let manifest = game_dir.join("instance.json");
        let raw = serde_json::to_string_pretty(&instance)?;
        std::fs::write(&manifest, raw).map_err(|e| Error::io(&manifest, e))?;

        self.instances.push(instance);
        Ok(slug)
    }

    pub fn remove(&mut self, slug: &str, delete_files: bool, paths: &Paths) -> Result<Option<Instance>> {
        let Some(index) = self.instances.iter().position(|i| i.slug == slug) else {
            return Ok(None);
        };
        let removed = self.instances.remove(index);
        if delete_files {
            let dir = paths.instance_dir(&removed.slug);
            if dir.exists() {
                std::fs::remove_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
            }
        }
        Ok(Some(removed))
    }

    /// `Mi Instancia` → `Mi_Instancia`, y si ya existe, `Mi_Instancia-2`.
    fn unique_slug(&self, base: &str) -> String {
        let base = sanitize(base.trim());
        if self.find(&base).is_none() {
            return base;
        }
        for suffix in 2..1000 {
            let candidate = format!("{base}-{suffix}");
            if self.find(&candidate).is_none() {
                return candidate;
            }
        }
        format!("{base}-{}", now())
    }
}

/// Marca de tiempo ISO-8601 en UTC, sin dependencias externas.
pub fn now() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    civil_from_unix(seconds)
}

/// Días desde 1970 → (año, mes, día). Algoritmo de Howard Hinnant.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

fn civil_from_unix(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let rest = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        (rest % 3600) / 60,
        rest % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_paths(name: &str) -> Paths {
        let root = std::env::temp_dir().join(format!("mclite-instance-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        Paths::with_root(root)
    }

    #[test]
    fn version_id_por_cargador() {
        let mut instance = Instance::new("Prueba", "1.21.4", LoaderKind::Vanilla);
        assert_eq!(instance.version_id(), "1.21.4");

        instance.loader = LoaderKind::Fabric;
        instance.loader_version = Some("0.19.5".into());
        assert_eq!(instance.version_id(), "fabric-loader-0.19.5-1.21.4");

        instance.loader = LoaderKind::Forge;
        instance.loader_version = Some("54.1.6".into());
        assert_eq!(instance.version_id(), "1.21.4-forge-54.1.6");

        instance.loader = LoaderKind::OptiFine;
        instance.loader_version = Some("HD_U_J3".into());
        assert_eq!(instance.version_id(), "OptiFine-1.21.4-HD_U_J3");
    }

    #[test]
    fn sugerencias_de_nombre() {
        assert_eq!(
            Instance::suggested_name(LoaderKind::Vanilla, "1.21.4"),
            "Minecraft 1.21.4"
        );
        assert_eq!(
            Instance::suggested_name(LoaderKind::Fabric, "1.21.4"),
            "Fabric 1.21.4"
        );
    }

    #[test]
    fn crea_game_dir_y_slug_unico() {
        let paths = temp_paths("add");
        let mut store = InstanceStore::default();

        let slug = store
            .add(Instance::new("Mi Instancia", "1.21.4", LoaderKind::Vanilla), &paths)
            .unwrap();
        assert_eq!(slug, "Mi_Instancia");
        assert!(paths.instance_dir(&slug).join("mods").is_dir());
        assert!(paths.instance_dir(&slug).join("instance.json").is_file());

        // El segundo con el mismo nombre no pisa al primero.
        let slug2 = store
            .add(Instance::new("Mi Instancia", "1.20.1", LoaderKind::Vanilla), &paths)
            .unwrap();
        assert_eq!(slug2, "Mi_Instancia-2");
        assert_eq!(store.instances.len(), 2);

        store.save(&paths).unwrap();
        let reloaded = InstanceStore::load(&paths);
        assert_eq!(reloaded.instances.len(), 2);
        assert!(reloaded.find_loose("mi instancia").is_some());
    }

    #[test]
    fn quitar_una_instancia_borra_sus_ficheros() {
        let paths = temp_paths("remove");
        let mut store = InstanceStore::default();
        let slug = store
            .add(Instance::new("Temporal", "1.21.4", LoaderKind::Vanilla), &paths)
            .unwrap();

        let removed = store.remove(&slug, true, &paths).unwrap().unwrap();
        assert_eq!(removed.name, "Temporal");
        assert!(!paths.instance_dir(&slug).exists());
        assert!(store.is_empty());
        // Quitar algo que no existe no es un error.
        assert!(store.remove("nada", true, &paths).unwrap().is_none());
    }

    #[test]
    fn la_fecha_es_iso() {
        // 2026-09-24T00:00:00Z → 1_790_208_000 s desde el epoch.
        let stamp = civil_from_unix(1_790_208_000);
        assert!(stamp.starts_with("2026-09-24T00:00:00"), "{stamp}");
        assert_eq!(now().len(), 20);
    }
}
