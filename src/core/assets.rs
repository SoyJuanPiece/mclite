//! Assets: índice + objetos. Un objeto se identifica por su hash, así que varias
//! entradas pueden compartir fichero (y se descarga una sola vez).

use std::collections::{BTreeMap, HashSet};

use serde::Deserialize;

use crate::core::endpoints;
use crate::core::http::Download;
use crate::core::paths::Paths;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AssetIndex {
    #[serde(default)]
    pub objects: BTreeMap<String, AssetObject>,
    /// Índices antiguos (< 1.7): los assets hay que copiarlos al gameDir.
    #[serde(default, rename = "virtual")]
    pub is_virtual: bool,
    #[serde(default)]
    pub map_to_resources: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}

impl AssetIndex {
    pub fn parse(raw: &str) -> crate::Result<Self> {
        Ok(serde_json::from_str(raw)?)
    }

    /// Bytes totales del índice (lo que va a ocupar en disco).
    pub fn total_size(&self) -> u64 {
        self.objects.values().map(|o| o.size).sum()
    }
}

/// URL de un objeto en el CDN de Mojang: `{raíz}/{2 primeros}/{hash}`.
pub fn object_url(hash: &str) -> String {
    let prefix = hash.get(..2).unwrap_or("_");
    format!("{}/{}/{}", endpoints::ASSET_OBJECTS_ROOT, prefix, hash)
}

/// Descargas pendientes para un índice, sin repetir hashes.
pub fn plan(index: &AssetIndex, paths: &Paths) -> Vec<Download> {
    let mut seen = HashSet::new();
    let mut jobs = Vec::new();
    for object in index.objects.values() {
        if !seen.insert(object.hash.as_str()) {
            continue;
        }
        jobs.push(
            Download::new(object_url(&object.hash), paths.asset_object_file(&object.hash))
                .with_sha1(&object.hash)
                .with_size(object.size),
        );
    }
    jobs
}

/// Cuántos objetos faltan (para no pedirle al usuario una descarga que no hace falta).
pub fn missing_count(jobs: &[Download]) -> usize {
    jobs.iter().filter(|job| !job.is_complete()).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_de_objeto_usa_el_prefijo() {
        assert_eq!(
            object_url("abcdef0123456789"),
            "https://resources.download.minecraft.net/ab/abcdef0123456789"
        );
    }

    #[test]
    fn plan_deduplica_hashes_repetidos() {
        let raw = r#"{"objects":{
            "minecraft/sounds/a.ogg":{"hash":"aabb","size":10},
            "minecraft/sounds/b.ogg":{"hash":"aabb","size":10},
            "minecraft/textures/x.png":{"hash":"ccdd","size":20}
        }}"#;
        let index = AssetIndex::parse(raw).unwrap();
        assert_eq!(index.total_size(), 40);

        let paths = Paths::with_root("/tmp/no-existe-mclite");
        let jobs = plan(&index, &paths);
        assert_eq!(jobs.len(), 2, "el hash repetido se descarga una sola vez");
        assert!(jobs
            .iter()
            .any(|j| j.dest.ends_with("assets/objects/aa/aabb")));
    }

    #[test]
    fn detecta_indices_antiguos_virtuales() {
        let index = AssetIndex::parse(r#"{"virtual":true,"objects":{}}"#).unwrap();
        assert!(index.is_virtual);
    }
}
