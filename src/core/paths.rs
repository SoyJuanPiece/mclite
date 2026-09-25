use std::path::{Path, PathBuf};

use crate::core::error::Result;

/// Layout en disco (modo portátil):
///
/// ```text
/// <carpeta del exe>/mclite/     junto a mclite.exe
/// ├── config.json
/// ├── instances.json
/// ├── versions/<id>/<id>.json | <id>.jar | natives/
/// ├── libraries/...
/// ├── assets/indexes/ | objects/ | log_configs/
/// ├── runtime/<componente-java>/
/// └── instances/<slug>/        gameDir real de cada instancia
/// ```
///
/// Si la carpeta del ejecutable no es escribible (p. ej. Program Files),
/// cae a `%APPDATA%\mclite` (en Linux: `~/.local/share/mclite`). `--root`
/// en la CLI siempre manda sobre ambas.
#[derive(Debug, Clone)]
pub struct Paths {
    root: PathBuf,
}

impl Paths {
    /// Detecta la raíz de datos: portátil (junto al exe) si se puede escribir,
    /// si no la carpeta estándar del usuario.
    pub fn discover() -> Result<Self> {
        if let Some(portable) = Self::portable() {
            return Ok(portable);
        }
        let base = dirs::data_dir().ok_or_else(|| {
            crate::Error::Unsupported(
                "no pude resolver el directorio de datos del usuario (¿%APPDATA% sin definir?)"
                    .into(),
            )
        })?;
        Ok(Self::with_root(base.join("mclite")))
    }

    /// Raíz portátil: `<carpeta del exe>/mclite`. Solo si la carpeta existe y
    /// dejamos escribir en ella (probe de escritura: Documentos o Descargas
    /// suelen permitirlo; Program Files no).
    fn portable() -> Option<Self> {
        let dir = std::env::current_exe()
            .ok()?
            .parent()?
            .join("mclite");
        std::fs::create_dir_all(&dir).ok()?;
        let probe = dir.join(".portable-probe");
        std::fs::write(&probe, b"ok").ok()?;
        let _ = std::fs::remove_file(&probe);
        Some(Self::with_root(dir))
    }

    /// Raíz explícita. Útil para tests y para `--root` en la CLI.
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Crea la estructura base si no existe.
    pub fn ensure(&self) -> Result<()> {
        for dir in [
            self.versions(),
            self.libraries(),
            self.asset_indexes(),
            self.asset_objects(),
            self.log_configs(),
            self.runtime(),
            self.instances(),
            self.logs(),
        ] {
            std::fs::create_dir_all(&dir).map_err(|e| crate::Error::io(dir, e))?;
        }
        Ok(())
    }

    pub fn versions(&self) -> PathBuf {
        self.root.join("versions")
    }

    pub fn version_dir(&self, id: &str) -> PathBuf {
        self.versions().join(sanitize(id))
    }

    pub fn version_json(&self, id: &str) -> PathBuf {
        self.version_dir(id).join(format!("{}.json", sanitize(id)))
    }

    pub fn version_jar(&self, id: &str) -> PathBuf {
        self.version_dir(id).join(format!("{}.jar", sanitize(id)))
    }

    pub fn natives_dir(&self, id: &str) -> PathBuf {
        self.version_dir(id).join("natives")
    }

    pub fn libraries(&self) -> PathBuf {
        self.root.join("libraries")
    }

    /// Ruta local de una librería maven, p. ej. `org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar`.
    pub fn library_file(&self, maven_path: &str) -> PathBuf {
        self.libraries().join(maven_path)
    }

    pub fn assets(&self) -> PathBuf {
        self.root.join("assets")
    }

    pub fn asset_indexes(&self) -> PathBuf {
        self.assets().join("indexes")
    }

    pub fn asset_index_file(&self, id: &str) -> PathBuf {
        self.asset_indexes().join(format!("{}.json", sanitize(id)))
    }

    pub fn asset_objects(&self) -> PathBuf {
        self.assets().join("objects")
    }

    /// Ruta local de un objeto de asset a partir de su hash.
    pub fn asset_object_file(&self, hash: &str) -> PathBuf {
        let prefix = hash.get(..2).unwrap_or("_");
        self.asset_objects().join(prefix).join(hash)
    }

    pub fn log_configs(&self) -> PathBuf {
        self.assets().join("log_configs")
    }

    pub fn runtime(&self) -> PathBuf {
        self.root.join("runtime")
    }

    /// Carpeta de un componente de Java de Mojang (p. ej. `java-runtime-delta`).
    pub fn runtime_component(&self, component: &str) -> PathBuf {
        self.runtime().join(sanitize(component))
    }

    pub fn instances(&self) -> PathBuf {
        self.root.join("instances")
    }

    pub fn instance_dir(&self, slug: &str) -> PathBuf {
        self.instances().join(sanitize(slug))
    }

    pub fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }

    /// `.mrpack` descargados de Modrinth, antes de instalarlos.
    pub fn packs_dir(&self) -> PathBuf {
        self.root.join("packs")
    }

    pub fn config_file(&self) -> PathBuf {
        self.root.join("config.json")
    }

    pub fn instances_file(&self) -> PathBuf {
        self.root.join("instances.json")
    }
}

/// Evita que un id externo (o un nombre de instancia) se salga del directorio.
pub fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => c,
            _ => '_',
        })
        .collect();
    // "." y ".." son rutas, no nombres.
    if cleaned.is_empty() || cleaned.chars().all(|c| c == '.') {
        "_".to_string()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_bloquea_traversal() {
        assert_eq!(sanitize("../etc/passwd"), ".._etc_passwd");
        assert_eq!(sanitize(".."), "_");
        assert_eq!(sanitize(""), "_");
        assert_eq!(sanitize("1.21.4"), "1.21.4");
    }

    #[test]
    fn ubicaciones_basicas() {
        let p = Paths::with_root("/tmp/mclite-test");
        assert_eq!(p.version_json("1.21.4"), PathBuf::from("/tmp/mclite-test/versions/1.21.4/1.21.4.json"));
        assert_eq!(
            p.library_file("org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar"),
            PathBuf::from("/tmp/mclite-test/libraries/org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar")
        );
        // El prefijo de un objeto de asset son sus 2 primeros caracteres.
        assert_eq!(
            p.asset_object_file("abcdef0123"),
            PathBuf::from("/tmp/mclite-test/assets/objects/ab/abcdef0123")
        );
    }
}
