//! `version_manifest_v2.json` y el filtro de versiones.
//!
//! El toggle de snapshots vive aquí: la lista que ve el usuario sale de
//! `VersionFilter`, no de un `if` suelto en la UI.

use serde::{Deserialize, Serialize};

use crate::core::error::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionManifest {
    pub latest: Latest,
    pub versions: Vec<ManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Latest {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub version_type: VersionType,
    pub url: String,
    #[serde(default)]
    pub time: Option<String>,
    #[serde(default)]
    pub release_time: Option<String>,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub compliance_level: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionType {
    Release,
    Snapshot,
    OldBeta,
    OldAlpha,
}

impl VersionType {
    pub fn label(&self) -> &'static str {
        match self {
            VersionType::Release => "Oficial",
            VersionType::Snapshot => "Snapshot",
            VersionType::OldBeta => "Beta",
            VersionType::OldAlpha => "Alpha",
        }
    }
}

/// Qué se muestra en el selector de versiones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VersionFilter {
    /// Muestra snapshots. Apagado por defecto: es el requisito del launcher.
    pub show_snapshots: bool,
    /// Muestra beta/alpha históricas. Apagado por defecto.
    pub show_old: bool,
}

impl Default for VersionFilter {
    fn default() -> Self {
        Self {
            show_snapshots: false,
            show_old: false,
        }
    }
}

impl VersionFilter {
    pub fn allows(&self, version_type: VersionType) -> bool {
        match version_type {
            VersionType::Release => true,
            VersionType::Snapshot => self.show_snapshots,
            VersionType::OldBeta | VersionType::OldAlpha => self.show_old,
        }
    }

    /// Aplica el filtro conservando el orden del manifiesto (más nuevas primero).
    pub fn apply<'a>(&self, versions: &'a [ManifestEntry]) -> Vec<&'a ManifestEntry> {
        versions
            .iter()
            .filter(|entry| self.allows(entry.version_type))
            .collect()
    }
}

impl VersionManifest {
    pub fn parse(raw: &str) -> Result<Self> {
        serde_json::from_str(raw).map_err(Error::from)
    }

    pub fn find(&self, id: &str) -> Option<&ManifestEntry> {
        self.versions.iter().find(|entry| entry.id == id)
    }

    /// Igual que `find`, pero con un mensaje de error que dice qué versión falta.
    pub fn require(&self, id: &str) -> Result<&ManifestEntry> {
        self.find(id)
            .ok_or_else(|| Error::Missing(format!("la versión «{id}» no está en el manifiesto")))
    }

    pub fn is_latest_release(&self, id: &str) -> bool {
        self.latest.release == id
    }

    pub fn is_latest_snapshot(&self, id: &str) -> bool {
        self.latest.snapshot == id
    }

    /// Las versiones listas para pintar, agrupadas como las quiere la UI.
    pub fn grouped(&self, filter: &VersionFilter) -> GroupedVersions<'_> {
        let mut releases = Vec::new();
        let mut snapshots = Vec::new();
        let mut old = Vec::new();
        for entry in self.versions.iter().filter(|e| filter.allows(e.version_type)) {
            match entry.version_type {
                VersionType::Release => releases.push(entry),
                VersionType::Snapshot => snapshots.push(entry),
                VersionType::OldBeta | VersionType::OldAlpha => old.push(entry),
            }
        }
        GroupedVersions {
            releases,
            snapshots,
            old,
        }
    }
}

pub struct GroupedVersions<'a> {
    pub releases: Vec<&'a ManifestEntry>,
    pub snapshots: Vec<&'a ManifestEntry>,
    pub old: Vec<&'a ManifestEntry>,
}

impl GroupedVersions<'_> {
    pub fn is_empty(&self) -> bool {
        self.releases.is_empty() && self.snapshots.is_empty() && self.old.is_empty()
    }

    pub fn len(&self) -> usize {
        self.releases.len() + self.snapshots.len() + self.old.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, version_type: VersionType) -> ManifestEntry {
        ManifestEntry {
            id: id.into(),
            version_type,
            url: format!("https://example.com/{id}.json"),
            time: None,
            release_time: None,
            sha1: None,
            compliance_level: None,
        }
    }

    fn manifest() -> VersionManifest {
        VersionManifest {
            latest: Latest {
                release: "26.3".into(),
                snapshot: "26.4-snapshot-1".into(),
            },
            versions: vec![
                entry("26.4-snapshot-1", VersionType::Snapshot),
                entry("26.3", VersionType::Release),
                entry("1.21.4", VersionType::Release),
                entry("b1.7.3", VersionType::OldBeta),
                entry("a1.2.6", VersionType::OldAlpha),
            ],
        }
    }

    #[test]
    fn por_defecto_solo_oficiales() {
        let m = manifest();
        let filter = VersionFilter::default();
        let grouped = m.grouped(&filter);
        let ids: Vec<&str> = grouped.releases.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["26.3", "1.21.4"]);
        // Sin snapshots y sin beta/alpha.
        assert!(grouped.snapshots.is_empty());
        assert!(grouped.old.is_empty());
    }

    #[test]
    fn con_snapshots_activados_aparecen() {
        let m = manifest();
        let filter = VersionFilter {
            show_snapshots: true,
            show_old: false,
        };
        let grouped = m.grouped(&filter);
        let ids: Vec<&str> = grouped.snapshots.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["26.4-snapshot-1"]);
        // Las oficiales siguen estando.
        assert_eq!(grouped.releases.len(), 2);
        // Y las alpha siguen fuera hasta que se pidan.
        assert!(grouped.old.is_empty());
    }

    #[test]
    fn beta_y_alpha_son_independientes_de_snapshots() {
        let m = manifest();
        let filter = VersionFilter {
            show_snapshots: false,
            show_old: true,
        };
        let grouped = m.grouped(&filter);
        assert_eq!(grouped.old.len(), 2);
        assert!(grouped.snapshots.is_empty());
        assert_eq!(grouped.releases.len(), 2);
    }

    #[test]
    fn reconoce_las_ultimas() {
        let m = manifest();
        assert!(m.is_latest_release("26.3"));
        assert!(m.is_latest_snapshot("26.4-snapshot-1"));
        assert!(!m.is_latest_release("1.21.4"));
    }

    #[test]
    fn deserializa_el_formato_real() {
        let raw = r#"{
            "latest": {"release": "26.3", "snapshot": "26.4-snapshot-1"},
            "versions": [
                {"id": "26.3", "type": "release", "url": "https://x", "time": "t", "releaseTime": "r"},
                {"id": "b1.7.3", "type": "old_beta", "url": "https://y"}
            ]
        }"#;
        let m = VersionManifest::parse(raw).unwrap();
        assert_eq!(m.versions.len(), 2);
        assert_eq!(m.versions[1].version_type, VersionType::OldBeta);
        assert_eq!(m.grouped(&VersionFilter::default()).len(), 1);
    }
}
