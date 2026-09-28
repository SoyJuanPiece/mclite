//! Carga de iconos para la UI, decodificados en local.
//!
//! `egui::Image::from_uri` delega en el loader HTTP de egui_extras y ahí se
//! quedaba en triángulo rojo con los .webp remotos. Este módulo baja el
//! icono a la caché de disco (`core::icons`), lo decodifica con la crate
//! `image` (png/webp/jpg) y lo sirve como `TextureHandle` cacheada por URL.
//!
//! Clave: la clave de caché SIEMPRE es la URL original. Resolver antes de
//! llamar aquí (file://…) rompía la correspondencia y los iconos quedaban en
//! placeholder para siempre aunque la descarga llegara al disco (bug 0.9.0).

use egui::TextureHandle;

use crate::core::icons::cache_path;
use crate::core::paths::Paths;

/// Decodifica los bytes de un icono a RGBA (png, webp o jpg; adivina formato).
fn decode(bytes: &[u8]) -> Option<(Vec<u8>, usize, usize)> {
    let img = image::load_from_memory(bytes).ok()?;
    let rgba = img.to_rgba8();
    let (width, height) = (rgba.width() as usize, rgba.height() as usize);
    Some((rgba.into_raw(), width, height))
}

/// Textura lista para `egui::Image::from_texture`, a partir de la URL ORIGINAL
/// del icono. Orden: textura en memoria → fichero en caché de disco → lanza
/// la descarga (una sola vez por URL) y pinta placeholder hasta que caiga.
pub fn texture_for_url(ui: &egui::Ui, paths: &Paths, url: &str) -> Option<TextureHandle> {
    if url.is_empty() {
        return None;
    }
    let id = egui::Id::new(format!("icon:{url}"));
    if let Some(handle) = ui.ctx().data_mut(|data| data.get_temp::<TextureHandle>(id)) {
        return Some(handle);
    }
    let path = cache_path(paths, url);
    if let Ok(bytes) = std::fs::read(&path) {
        if let Some((pixels, width, height)) = decode(&bytes) {
            let image = egui::ColorImage::from_rgba_unmultiplied([width, height], &pixels);
            let handle = ui.ctx().load_texture(format!("icon:{url}"), image, egui::TextureOptions::LINEAR);
            ui.ctx().data_mut(|data| data.insert_temp(egui::Id::new(format!("icon:{url}")), handle.clone()));
            return Some(handle);
        }
    }
    // No está en caché: baja en un hilo de fondo UNA vez (guardia en temp para
    // no lanzar un spawn por frame) y la próxima pasada
    // entrará por el fichero de arriba.
    let guard = egui::Id::new(format!("icon-dl:{url}"));
    let already = ui
        .ctx()
        .data_mut(|data| data.get_temp::<bool>(guard).unwrap_or(false));
    if !already {
        ui.ctx()
            .data_mut(|data| data.insert_temp(guard, true));
        let dest = path;
        let url_owned = url.to_string();
        std::thread::spawn(move || {
            let http = crate::core::http::HttpClient::new();
            let _ = http.download(&crate::core::http::Download::new(&url_owned, &dest));
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodifica_un_png_pequeno() {
        let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([1, 2, 3, 255]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        let (pixels, w, h) = decode(&out.into_inner()).unwrap();
        assert_eq!((w, h), (2, 2));
        assert_eq!(pixels.len(), 16);
    }
}
