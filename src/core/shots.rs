//! Galería de capturas de la instancia: lista `screenshots/*.png` ordenado por
//! fecha (más nueva primero) y decodifica miniaturas para la UI.

use std::path::{Path, PathBuf};

use crate::core::error::{Error, Result};

/// Una captura de la carpeta `screenshots/` de la instancia.
#[derive(Debug, Clone)]
pub struct Shot {
    pub file_name: String,
    pub path: PathBuf,
    /// Marca de tiempo de modificación (epoch segundos), para ordenar.
    pub modified: i64,
    pub size: u64,
}

/// Lista las capturas de la instancia. `Ok(vec![])` si la carpeta no existe.
pub fn list(game_dir: &Path) -> Result<Vec<Shot>> {
    let dir = game_dir.join("screenshots");
    let mut shots = Vec::new();
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(shots),
        Err(err) => return Err(Error::io(&dir, err)),
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name.to_string(),
            None => continue,
        };
        if !name.to_lowercase().ends_with(".png") {
            continue;
        }
        let meta = match entry.metadata() {
            Ok(meta) => meta,
            Err(_) => continue,
        };
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        shots.push(Shot {
            file_name: name,
            path,
            modified,
            size: meta.len(),
        });
    }
    shots.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(shots)
}

/// Decodifica un PNG a píxeles RGBA planos (para `ColorImage` de egui).
pub fn decode_rgba(path: &Path) -> Result<(Vec<u8>, u32, u32)> {
    let bytes = std::fs::read(path).map_err(|e| Error::io(path, e))?;
    let img = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
        .map_err(|err| Error::Unsupported(format!("PNG inválido ({}): {err}", path.display())))?;
    let rgba = img.to_rgba8();
    let (width, height) = (rgba.width(), rgba.height());
    Ok((rgba.into_raw(), width, height))
}

/// Borra una captura.
pub fn delete(path: &Path) -> Result<()> {
    std::fs::remove_file(path).map_err(|e| Error::io(path, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mclite-shots-{tag}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(dir.join("screenshots")).unwrap();
        dir
    }

    /// PNG de 1x1 rojo, generado con la misma crate `image` del launcher.
    fn tiny_png() -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn lista_solo_png_ordenado_por_fecha() {
        let dir = temp_dir("list");
        let shots_dir = dir.join("screenshots");
        std::fs::write(shots_dir.join("b.png"), tiny_png()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(shots_dir.join("a.png"), tiny_png()).unwrap();
        std::fs::write(shots_dir.join("nota.txt"), b"x").unwrap();
        let shots = list(&dir).unwrap();
        assert_eq!(shots.len(), 2);
        // La más nueva primero.
        assert_eq!(shots[0].file_name, "a.png");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn decodifica_png_valido() {
        let dir = temp_dir("decode");
        let path = dir.join("screenshots/a.png");
        std::fs::write(&path, tiny_png()).unwrap();
        let (pixels, w, h) = decode_rgba(&path).unwrap();
        assert_eq!((w, h), (1, 1));
        assert_eq!(pixels.len(), 4);
        assert_eq!(&pixels[0..3], &[255, 0, 0]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
