//! Caché de iconos en disco (Modrinth y cualquier URL remota).
//!
//! `resolve()` devuelve una ruta `file://…` lista para `egui::Image::from_uri`:
//! si el icono ya está en `cache/icons/<sha1(url)>`, es instantáneo (y funciona
//! sin red); si no, lo baja en un hilo de fondo y la próxima vez que se pinte
//! la imagen estará. Mientras tanto se devuelve la URL remota tal cual, así el
//! egui_extras la carga por HTTP como siempre hizo.

use sha1::{Digest, Sha1};
use std::path::PathBuf;

use crate::core::http::{Download, HttpClient};
use crate::core::paths::Paths;

/// Nombre de fichero cacheado para una URL: sha1 corto + extensión si la hay.
fn cache_name(url: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(url.as_bytes());
    let digest = hasher.finalize();
    let hash: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    let extension = url
        .rsplit('.')
        .next()
        .filter(|ext| matches!(*ext, "png" | "webp" | "jpg" | "jpeg" | "gif"))
        .map(|ext| format!(".{ext}"))
        .unwrap_or_default();
    format!("{hash}{extension}")
}

/// Ruta local en caché para una URL.
pub fn cache_path(paths: &Paths, url: &str) -> PathBuf {
    paths.cache_icons().join(cache_name(url))
}

/// Convierte una ruta local en un URI que `egui::Image::from_uri` entienda.
fn file_uri(path: &PathBuf) -> String {
    #[cfg(windows)]
    {
        let text = path.to_string_lossy().replace('\\', "/");
        format!("file:///{text}")
    }
    #[cfg(not(windows))]
    {
        format!("file://{}", path.display())
    }
}

/// Resuelve el mejor URI para pintar el icono de una URL:
/// - si ya está en caché → `file://…` (instantáneo, offline);
/// - si no → la URL remota, y en un hilo de fondo lo baja para la próxima.
pub fn resolve(paths: &Paths, url: &str) -> String {
    if url.is_empty() {
        return url.to_string();
    }
    let cached = cache_path(paths, url);
    if cached.is_file() {
        return file_uri(&cached);
    }
    // Descarga silenciosa para la próxima pasada de pintado.
    let dest = cached.clone();
    let url_owned = url.to_string();
    std::thread::spawn(move || {
        let http = HttpClient::new();
        let download = Download::new(&url_owned, &dest);
        let _ = http.download(&download);
    });
    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_nombre_de_cache_es_estable_y_con_extension() {
        let a = cache_name("https://cdn.modrinth.com/icon.webp");
        let b = cache_name("https://cdn.modrinth.com/icon.webp");
        assert_eq!(a, b);
        assert!(a.ends_with(".webp"));
        assert_eq!(a.len(), 40 + 5); // sha1 hex + extensión
        let other = cache_name("https://cdn.modrinth.com/otro.png");
        assert_ne!(a, other);
        assert!(other.ends_with(".png"));
    }
}
