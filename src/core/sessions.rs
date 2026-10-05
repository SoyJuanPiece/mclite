//! Historial de sesiones de juego (`sessions.json`).
//!
//! Complementa `Instance::playtime_secs` (que solo acumula un total): aquí se
//! guarda una entrada por partida jugada, para que la UI pueda mostrar un
//! historial real ("jugaste 2h el martes") en vez de solo un acumulado.

use serde::{Deserialize, Serialize};

use crate::core::error::{Error, Result};
use crate::core::paths::Paths;

/// Cuántas entradas por instancia se conservan como máximo (evita que el
/// archivo crezca sin límite en cuentas con años de uso).
const MAX_ENTRIES_PER_INSTANCE: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaySession {
    pub slug: String,
    pub mc_version: String,
    /// Fecha/hora ISO-8601 de inicio de la sesión (mismo formato que
    /// `Instance::last_played`, vía `core::instance::now()`).
    pub started: String,
    pub secs: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionHistory {
    pub sessions: Vec<PlaySession>,
}

impl SessionHistory {
    fn file(paths: &Paths) -> std::path::PathBuf {
        paths.root().join("sessions.json")
    }

    pub fn load(paths: &Paths) -> Self {
        match std::fs::read_to_string(Self::file(paths)) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        paths.ensure()?;
        let file = Self::file(paths);
        let raw = serde_json::to_string_pretty(self)?;
        std::fs::write(&file, raw).map_err(|e| Error::io(&file, e))
    }

    /// Registra una sesión jugada y recorta el historial de esa instancia si
    /// se pasa de `MAX_ENTRIES_PER_INSTANCE`.
    pub fn record(&mut self, slug: &str, mc_version: &str, started: &str, secs: u64) {
        self.sessions.push(PlaySession {
            slug: slug.to_string(),
            mc_version: mc_version.to_string(),
            started: started.to_string(),
            secs,
        });
        let count = self.sessions.iter().filter(|s| s.slug == slug).count();
        if count > MAX_ENTRIES_PER_INSTANCE {
            let mut seen = 0usize;
            self.sessions.retain(|session| {
                if session.slug != slug {
                    return true;
                }
                seen += 1;
                seen > count - MAX_ENTRIES_PER_INSTANCE
            });
        }
    }

    /// Últimas `limit` sesiones de una instancia, más recientes primero.
    pub fn for_instance(&self, slug: &str, limit: usize) -> Vec<&PlaySession> {
        let mut matches: Vec<&PlaySession> =
            self.sessions.iter().filter(|s| s.slug == slug).collect();
        matches.reverse();
        matches.truncate(limit);
        matches
    }

    /// Segundos totales jugados por una instancia según el historial (no
    /// depende de `Instance::playtime_secs`, útil para verificarlo o si se
    /// pierde el acumulado).
    pub fn total_secs(&self, slug: &str) -> u64 {
        self.sessions
            .iter()
            .filter(|s| s.slug == slug)
            .map(|s| s.secs)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_y_total_secs() {
        let mut history = SessionHistory::default();
        history.record("mi-mundo", "1.21.4", "2026-01-01T10:00:00Z", 1800);
        history.record("mi-mundo", "1.21.4", "2026-01-02T10:00:00Z", 3600);
        assert_eq!(history.total_secs("mi-mundo"), 5400);
        assert_eq!(history.for_instance("mi-mundo", 10).len(), 2);
    }

    #[test]
    fn for_instance_devuelve_las_mas_recientes_primero() {
        let mut history = SessionHistory::default();
        history.record("a", "1.21.4", "2026-01-01T00:00:00Z", 100);
        history.record("a", "1.21.4", "2026-01-02T00:00:00Z", 200);
        let recent = history.for_instance("a", 1);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].secs, 200);
    }

    #[test]
    fn record_recorta_historial_largo() {
        let mut history = SessionHistory::default();
        for i in 0..(MAX_ENTRIES_PER_INSTANCE + 10) {
            history.record("a", "1.21.4", &format!("2026-01-01T00:00:{i:02}Z"), 1);
        }
        assert_eq!(
            history.sessions.iter().filter(|s| s.slug == "a").count(),
            MAX_ENTRIES_PER_INSTANCE
        );
    }

    #[test]
    fn no_mezcla_sesiones_de_otras_instancias() {
        let mut history = SessionHistory::default();
        history.record("a", "1.21.4", "2026-01-01T00:00:00Z", 100);
        history.record("b", "1.21.4", "2026-01-01T00:00:00Z", 999);
        assert_eq!(history.total_secs("a"), 100);
        assert_eq!(history.for_instance("b", 10).len(), 1);
    }
}
