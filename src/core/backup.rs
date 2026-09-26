//! Copias de seguridad: una instancia ↔ un `.zip`.
//!
//! El zip lleva un manifiesto (`mclite-backup.json`) con versión de Minecraft,
//! cargador y nombre, y los datos del juego bajo `game/`: mundos, mods (con los
//! `.disabled`), resourcepacks, shaderpacks, screenshots y configuración
//! (options.txt, servers.dat, config/). Con eso, importar en otra PC deja la
//! instancia lista para `Reparar` (que baja el juego base si falta).

use std::io::{Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::core::error::{Error, Result};

/// Identificación mínima de la instancia dentro del zip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    /// Marca de formato; si algún día cambia, `import` sabrá avisar.
    pub format: u32,
    pub name: String,
    pub mc_version: String,
    /// Clave del cargador (texto, para no acoplar el zip a este binario).
    pub loader: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loader_version: Option<String>,
    /// Instante de creación del backup (epoch segundos).
    pub created: i64,
}

/// Resumen al inspeccionar/importar un backup.
#[derive(Debug, Clone)]
pub struct BackupInfo {
    pub manifest: BackupManifest,
    /// Cuántos ficheros de juego trae (sin contar el manifiesto).
    pub files: u64,
}

const FORMAT: u32 = 1;
const MANIFEST_PATH: &str = "mclite-backup.json";

/// Carpetas y ficheros del game_dir que viajan en el backup.
const INCLUDE_DIRS: &[&str] = &[
    "worlds",
    "mods",
    "resourcepacks",
    "shaderpacks",
    "screenshots",
    "config",
];
const INCLUDE_FILES: &[&str] = &["options.txt", "servers.dat", "servers.dat_old", "usercache.json"];

/// Exporta la instancia a `dest.zip`. Sobreescribe si existe.
pub fn export(
    game_dir: &Path,
    manifest: &BackupManifest,
    dest: &Path,
) -> Result<u64> {
    let file = std::fs::File::create(dest).map_err(|e| Error::io(dest, e))?;
    let mut zip = zip::ZipWriter::new(file);
    let options: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default();

    let manifest_json = serde_json::to_string_pretty(manifest)?;
    zip.start_file(MANIFEST_PATH, options)
        .map_err(|e| Error::Zip { path: dest.into(), reason: e.to_string() })?;
    zip.write_all(manifest_json.as_bytes())
        .map_err(|e| Error::io(dest, e))?;

    let mut count: u64 = 0;
    let push_file = |zip: &mut zip::ZipWriter<std::fs::File>,
                     count: &mut u64,
                     arc: String,
                     path: &Path|
     -> Result<()> {
        let bytes = std::fs::read(path).map_err(|e| Error::io(path, e))?;
        zip.start_file(arc.clone(), options)
            .map_err(|e| Error::Zip { path: path.into(), reason: e.to_string() })?;
        zip.write_all(&bytes).map_err(|e| Error::io(path, e))?;
        *count += 1;
        Ok(())
    };

    for dir_name in INCLUDE_DIRS {
        let dir = game_dir.join(dir_name);
        if !dir.is_dir() {
            continue;
        }
        // Recursivo de una vez (walk_files también incluye los ficheros que
        // cuelgan directamente de la carpeta; así no hay duplicados).
        for entry in walk_files(&dir, dir_name)? {
            push_file(&mut zip, &mut count, format!("game/{}", entry.arc), &entry.path)?;
        }
    }
    for file_name in INCLUDE_FILES {
        let path = game_dir.join(file_name);
        if path.is_file() {
            push_file(&mut zip, &mut count, format!("game/{file_name}"), &path)?;
        }
    }

    zip.finish().map_err(|e| Error::Zip {
        path: dest.into(),
        reason: e.to_string(),
    })?;
    Ok(count)
}

/// Fichero relativo encontrado al recorrer recursivamente.
struct Walked {
    arc: String,
    path: std::path::PathBuf,
}

/// Recorre recursivamente (profundidad acotada) lo que `list_dir` se saltó.
fn walk_files(root: &Path, prefix: &str) -> Result<Vec<Walked>> {
    let mut out = Vec::new();
    walk_inner(root, prefix, &mut out, 0)?;
    Ok(out)
}

fn walk_inner(dir: &Path, arc_prefix: &str, out: &mut Vec<Walked>, depth: usize) -> Result<()> {
    if depth > 6 {
        return Ok(()); // suficiente para mundos/config; evita recorridos locos
    }
    for entry in std::fs::read_dir(dir).map_err(|e| Error::io(dir, e))?.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            walk_inner(&path, &format!("{arc_prefix}/{name}"), out, depth + 1)?;
        } else {
            out.push(Walked {
                arc: format!("{arc_prefix}/{name}"),
                path,
            });
        }
    }
    Ok(())
}

/// Lee el manifiesto y cuenta los ficheros, sin extraer nada.
pub fn inspect(zip_path: &Path) -> Result<BackupInfo> {
    let (manifest, files) = read_manifest(zip_path)?;
    Ok(BackupInfo { manifest, files })
}

fn read_manifest(zip_path: &Path) -> Result<(BackupManifest, u64)> {
    let file = std::fs::File::open(zip_path).map_err(|e| Error::io(zip_path, e))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| Error::Zip {
        path: zip_path.into(),
        reason: e.to_string(),
    })?;
    let mut manifest_json = String::new();
    zip.by_name(MANIFEST_PATH)
        .map_err(|_| {
            Error::Unsupported("no parece un backup de McLite (falta el manifiesto)".into())
        })?
        .read_to_string(&mut manifest_json)
        .map_err(|e| Error::io(zip_path, e))?;
    let manifest: BackupManifest = serde_json::from_str(&manifest_json)?;
    if manifest.format != FORMAT {
        return Err(Error::Unsupported(format!(
            "backup de formato {} (este launcher entiende el {FORMAT})",
            manifest.format
        )));
    }
    let files = zip
        .file_names()
        .filter(|name| name.starts_with("game/"))
        .count() as u64;
    Ok((manifest, files))
}

/// Extrae el backup en `game_dir` (creándolo). Devuelve el resumen.
pub fn import(zip_path: &Path, game_dir: &Path) -> Result<BackupInfo> {
    let (manifest, files) = read_manifest(zip_path)?;
    std::fs::create_dir_all(game_dir).map_err(|e| Error::io(game_dir, e))?;
    let file = std::fs::File::open(zip_path).map_err(|e| Error::io(zip_path, e))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| Error::Zip {
        path: zip_path.into(),
        reason: e.to_string(),
    })?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|e| Error::Zip {
            path: zip_path.into(),
            reason: e.to_string(),
        })?;
        let Some(rel) = entry.enclosed_name() else {
            continue; // zip-slip: fuera
        };
        if !rel.starts_with("game") {
            continue; // solo datos de juego; el manifiesto no se extrae
        }
        let dest = game_dir.join(rel.strip_prefix("game").unwrap());
        if entry.is_dir() {
            std::fs::create_dir_all(&dest).map_err(|e| Error::io(&dest, e))?;
            continue;
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        let mut out = std::fs::File::create(&dest).map_err(|e| Error::io(&dest, e))?;
        std::io::copy(&mut entry, &mut out).map_err(|e| Error::io(&dest, e))?;
    }
    Ok(BackupInfo { manifest, files })
}

/// Nombre sugerido para el zip: `Mi instancia-2026-09-26.zip`.
pub fn suggested_file_name(instance_name: &str) -> String {
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0);
    // Fecha civil desde epoch (sin dependencias: algoritmo de días → y/m/d).
    let (y, m, d) = civil_from_days(days as i64);
    let safe = crate::core::paths::sanitize(instance_name);
    format!("{safe}-{y:04}-{m:02}-{d:02}.zip")
}

/// Howard Hinnant's `civil_from_days`: días desde epoch → (año, mes, día).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mclite-backup-{tag}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn manifest() -> BackupManifest {
        BackupManifest {
            format: FORMAT,
            name: "Mi Mundo".into(),
            mc_version: "1.21.4".into(),
            loader: "fabric".into(),
            loader_version: Some("0.16.14".into()),
            created: 1_790_000_000,
        }
    }

    #[test]
    fn exporta_inspecta_e_importa() {
        let base = temp_dir("roundtrip");
        let game = base.join("game");
        std::fs::create_dir_all(game.join("worlds/mundo")).unwrap();
        std::fs::write(game.join("worlds/mundo/level.dat"), b"dat").unwrap();
        std::fs::create_dir_all(game.join("mods")).unwrap();
        std::fs::write(game.join("mods/sodium.jar"), b"jar").unwrap();
        std::fs::write(game.join("options.txt"), b"music=0").unwrap();

        let zip_path = base.join("backup.zip");
        let count = export(&game, &manifest(), &zip_path).unwrap();
        assert!(count >= 3);

        let info = inspect(&zip_path).unwrap();
        assert_eq!(info.manifest.name, "Mi Mundo");
        assert_eq!(info.manifest.mc_version, "1.21.4");
        assert_eq!(info.files, count);

        let restored = base.join("restored");
        let info = import(&zip_path, &restored).unwrap();
        assert_eq!(info.manifest.name, "Mi Mundo");
        assert_eq!(
            std::fs::read(restored.join("worlds/mundo/level.dat")).unwrap(),
            b"dat"
        );
        assert_eq!(std::fs::read(restored.join("mods/sodium.jar")).unwrap(), b"jar");
        assert_eq!(std::fs::read(restored.join("options.txt")).unwrap(), b"music=0");

        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn rechaza_zips_ajenos() {
        let base = temp_dir("foreign");
        let zip_path = base.join("no.zip");
        let file = std::fs::File::create(&zip_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file("cualquier-cosa.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"hola").unwrap();
        zip.finish().unwrap();
        assert!(inspect(&zip_path).is_err());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn nombre_sugerido_con_fecha() {
        let name = suggested_file_name("Mi Instancia");
        assert!(name.starts_with("Mi_Instancia-2"));
        assert!(name.ends_with(".zip"));
    }
}
