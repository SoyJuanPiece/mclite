//! Gestor de mods: listar, activar/desactivar y borrar los .jar de una instancia.
//!
//! Un mod desactivado se renombra a `*.disabled` (convención de Fabric/Quilt y
//! la que usa Forge moderno): el juego lo ignora y el launcher lo puede
//! reactivar renombrándolo de vuelta. Sin descargas: instalar mods es cosa de
//! Modrinth (pestaña Modpacks) o de arrastrar un .jar a la ventana.

use std::path::{Path, PathBuf};

use crate::core::error::{Error, Result};

/// Un mod de la carpeta `mods/` (activo o desactivado).
#[derive(Debug, Clone)]
pub struct ModEntry {
    /// Nombre del fichero (con o sin `.disabled`).
    pub file_name: String,
    /// `true` = el juego lo carga.
    pub enabled: bool,
    /// Tamaño en bytes (para mostrarlo).
    pub size: u64,
}

impl ModEntry {
    /// Nombre "limpio": sin la extensión `.disabled` y sin la ruta.
    pub fn display_name(&self) -> &str {
        let name = self.file_name.as_str();
        match name.strip_suffix(".disabled") {
            Some(base) => base,
            None => name,
        }
    }
}

/// Lista los mods de una instancia (carpeta `mods/`). `Ok(vec![])` si no existe.
pub fn list(game_dir: &Path) -> Result<Vec<ModEntry>> {
    let dir = game_dir.join("mods");
    let mut mods = Vec::new();
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(mods),
        Err(err) => return Err(Error::io(&dir, err)),
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name.to_string(),
            None => continue,
        };
        let is_jar = name.to_lowercase().ends_with(".jar");
        let is_disabled = name
            .to_lowercase()
            .ends_with(".jar.disabled");
        if !is_jar && !is_disabled {
            continue;
        }
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        mods.push(ModEntry {
            file_name: name,
            enabled: is_jar,
            size,
        });
    }
    mods.sort_by(|a, b| a.display_name().to_lowercase().cmp(&b.display_name().to_lowercase()));
    Ok(mods)
}

fn mods_path(game_dir: &Path, file_name: &str) -> Result<PathBuf> {
    // El nombre viene de la UI (de list()); aun así se valida la cadena CRUDA:
    // sin rutas, sin ".." y debe quedar idéntica al extraer el nombre final.
    let clean = file_name.trim();
    let simple = Path::new(clean)
        .file_name()
        .and_then(|n| n.to_str())
        == Some(clean);
    if clean.is_empty()
        || !simple
        || clean.contains("..")
        || clean.contains('/')
        || clean.contains('\\')
    {
        return Err(Error::Unsupported(format!(
            "nombre de mod inválido: {file_name}"
        )));
    }
    Ok(game_dir.join("mods").join(clean))
}

/// Activa un mod desactivado (`x.jar.disabled` → `x.jar`).
pub fn enable(game_dir: &Path, file_name: &str) -> Result<()> {
    let from = mods_path(game_dir, file_name)?;
    let target = from
        .to_string_lossy()
        .trim_end_matches(".disabled")
        .to_string();
    let to = PathBuf::from(&target);
    if !from.exists() {
        return Err(Error::Unsupported(format!("no está: {file_name}")));
    }
    std::fs::rename(&from, &to).map_err(|e| Error::io(&to, e))
}

/// Desactiva un mod activo (`x.jar` → `x.jar.disabled`).
pub fn disable(game_dir: &Path, file_name: &str) -> Result<()> {
    let from = mods_path(game_dir, file_name)?;
    let mut to = from.clone().into_os_string();
    to.push(".disabled");
    let to = PathBuf::from(to);
    if !from.exists() {
        return Err(Error::Unsupported(format!("no está: {file_name}")));
    }
    std::fs::rename(&from, &to).map_err(|e| Error::io(&to, e))
}

/// Borra el mod (activo o desactivado).
pub fn delete(game_dir: &Path, file_name: &str) -> Result<()> {
    let path = mods_path(game_dir, file_name)?;
    if !path.exists() {
        return Err(Error::Unsupported(format!("no está: {file_name}")));
    }
    std::fs::remove_file(&path).map_err(|e| Error::io(&path, e))
}

/// Abre la carpeta `mods/` (creándola si falta).
pub fn ensure_dir(game_dir: &Path) -> Result<PathBuf> {
    let dir = game_dir.join("mods");
    std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mclite-mods-{tag}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(dir.join("mods")).unwrap();
        dir
    }

    #[test]
    fn lista_activo_y_desactivado() {
        let dir = temp_dir("list");
        std::fs::write(dir.join("mods/sodium.jar"), b"jar").unwrap();
        std::fs::write(dir.join("mods/optifine.jar.disabled"), b"jar").unwrap();
        let mods = list(&dir).unwrap();
        assert_eq!(mods.len(), 2);
        let sodium = mods.iter().find(|m| m.display_name() == "sodium.jar").unwrap();
        assert!(sodium.enabled);
        let opti = mods
            .iter()
            .find(|m| m.display_name() == "optifine.jar")
            .unwrap();
        assert!(!opti.enabled);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn desactiva_y_reactiva() {
        let dir = temp_dir("toggle");
        std::fs::write(dir.join("mods/mod.jar"), b"jar").unwrap();
        disable(&dir, "mod.jar").unwrap();
        assert!(!dir.join("mods/mod.jar").exists());
        assert!(dir.join("mods/mod.jar.disabled").exists());
        enable(&dir, "mod.jar.disabled").unwrap();
        assert!(dir.join("mods/mod.jar").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn borra_y_rechaza_rutas_raras() {
        let dir = temp_dir("del");
        std::fs::write(dir.join("mods/mod.jar"), b"jar").unwrap();
        delete(&dir, "mod.jar").unwrap();
        assert_eq!(list(&dir).unwrap().len(), 0);
        // ../ o subrutas no pasan el saneo.
        assert!(mods_path(&dir, "../etc.jar").is_err());
        assert!(mods_path(&dir, "a\\b.jar").is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
