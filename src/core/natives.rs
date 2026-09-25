//! Extracción de las natives a `java.library.path`.
//!
//! Se aplanan las rutas (el DLL tiene que quedar directamente en la carpeta de
//! natives o `System.loadLibrary` no lo encuentra) y se descarta `META-INF/`.

use std::path::Path;

use crate::core::error::{Error, Result};

/// Extrae el contenido de `jar` a `dest`. Devuelve cuántos ficheros escribió.
pub fn extract(jar: &Path, dest: &Path, exclude: &[String]) -> Result<usize> {
    let file = std::fs::File::open(jar).map_err(|e| Error::io(jar, e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| Error::Zip {
        path: jar.to_path_buf(),
        reason: e.to_string(),
    })?;

    std::fs::create_dir_all(dest).map_err(|e| Error::io(dest, e))?;

    let mut written = 0usize;
    for index in 0..archive.len() {
        let mut entry = match archive.by_index(index) {
            Ok(entry) => entry,
            Err(err) => {
                return Err(Error::Zip {
                    path: jar.to_path_buf(),
                    reason: err.to_string(),
                })
            }
        };
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        if is_excluded(&name, exclude) {
            continue;
        }
        // Aplanamos: `foo/bar/lwjgl.dll` → `dest/lwjgl.dll`.
        let Some(file_name) = Path::new(&name).file_name() else {
            continue;
        };
        let target = dest.join(file_name);
        let mut out = std::fs::File::create(&target).map_err(|e| Error::io(&target, e))?;
        std::io::copy(&mut entry, &mut out).map_err(|e| Error::io(&target, e))?;
        written += 1;
    }
    Ok(written)
}

/// `META-INF/` siempre fuera: solo trae firmas que invalidan el jar al modificarlo.
fn is_excluded(name: &str, exclude: &[String]) -> bool {
    if name.starts_with("META-INF/") || name == "META-INF" {
        return true;
    }
    exclude
        .iter()
        .any(|pattern| !pattern.is_empty() && name.starts_with(pattern.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excluye_meta_inf_siempre() {
        assert!(is_excluded("META-INF/MANIFEST.MF", &[]));
        assert!(is_excluded("META-INF/MANIFEST.MF", &["META-INF/".into()]));
        assert!(!is_excluded("lwjgl.dll", &[]));
        assert!(!is_excluded("lwjgl.dll", &["META-INF/".into()]));
    }

    #[test]
    fn respeta_exclusiones_extra() {
        assert!(is_excluded("licenses/x.txt", &["licenses/".into()]));
        assert!(!is_excluded("licenses/x.txt", &[]));
    }
}
