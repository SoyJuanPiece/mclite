//! Carga de iconos para la UI, decodificados en local.
//!
//! `egui::Image::from_uri` delega en el loader HTTP de egui_extras y ahí se
//! quedaba en triángulo rojo con los .webp de Modrinth. Este módulo baja el
//! icono a la caché de disco (`icons::resolve`), lo decodifica con la crate
//! `image` (png/webp/jpg) y lo sirve como `TextureHandle` cacheada por URL.

use egui::TextureHandle;

use crate::core::paths::Paths;

/// Decodifica los bytes de un icono a RGBA (png, webp o jpg; adivina formato).
fn decode(bytes: &[u8]) -> Option<(Vec<u8>, usize, usize)> {
    let img = image::load_from_memory(bytes).ok()?;
    let rgba = img.to_rgba8();
    let (width, height) = (rgba.width() as usize, rgba.height() as usize);
    Some((rgba.into_raw(), width, height))
}

/// Textura lista para `egui::Image::from_texture`. Sincrona: el caller la pide
/// con el icono ya en caché (`icons::resolve` lo baja en background), y si
/// aún no está pinta el placeholder.
pub fn texture_for_url(ui: &egui::Ui, paths: &Paths, url: &str) -> Option<TextureHandle> {
    if url.is_empty() {
        return None;
    }
    let name = format!("icon:{}", crate::core::icons::cache_path(paths, url).display());
    if let Some(handle) = ui
        .ctx()
        .data_mut(|data| data.get_temp::<TextureHandle>(egui::Id::new(name.clone())))
    {
        return Some(handle);
    }
    // Solo decodifica si el fichero ya está en caché (si no, resolve() lo baja
    // en un hilo y en la próxima pasada aparecerá).
    let bytes = std::fs::read(crate::core::icons::cache_path(paths, url)).ok()?;
    let (pixels, width, height) = decode(&bytes)?;
    let image = egui::ColorImage::from_rgba_unmultiplied([width, height], &pixels);
    let handle = ui.ctx().load_texture(name.clone(), image, egui::TextureOptions::LINEAR);
    ui.ctx().data_mut(|data| {
        data.insert_temp(egui::Id::new(name), handle.clone());
    });
    Some(handle)
}

/// Quita emojis y símbolos fuera de BMP que la fuente Inter no tiene (se ven
/// como cuadritos □ en las descripciones de Modrinth).
pub fn strip_emojis(text: &str) -> String {
    text.chars()
        .filter(|c| {
            let code = *c as u32;
            !(code >= 0x1F000)                 // emojis y símbolos nuevos
                && !(0x2600..=0x27BF).contains(&code) // misc symbols + dingbats
                && !(0xFE00..=0xFE0F).contains(&code) // variation selectors
                && !(0x1F1E6..=0x1F1FF).contains(&code) // banderas
                && *c != '\u{200D}'             // zero-width joiner
                && *c != '\u{FE0F}'
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quita_emojis_pero_deja_texto_normal() {
        assert_eq!(strip_emojis("Hola ▃ 100 Days 🔥 mundo"), "Hola ▃ 100 Days  mundo");
        assert_eq!(strip_emojis("Battle Armory ⚔️ TACZ"), "Battle Armory  TACZ");
        assert_eq!(strip_emojis("400+ mods"), "400+ mods");
        // Conserva acentos y caracteres latinos.
        assert_eq!(strip_emojis("configuración ✅ rápida"), "configuración  rápida");
    }

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
