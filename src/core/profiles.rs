//! Perfiles de rendimiento: RAM y argumentos de JVM preconfigurados.
//!
//! En vez de que el usuario tenga que investigar flags de G1GC o cuánta RAM
//! darle al juego, elige uno de 3 perfiles y McLite calcula lo demás. Son solo
//! una fuente de valores por defecto: la instancia sigue guardando `ram_mb` y
//! el usuario puede editarlo a mano después (el perfil no se "fija").

use serde::{Deserialize, Serialize};

use crate::core::config::{DEFAULT_RAM_MB, MAX_RAM_MB, MIN_RAM_MB};

/// Perfil de rendimiento seleccionable al crear o editar una instancia.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PerformanceProfile {
    /// RAM moderada y G1GC estándar: sirve para la mayoría de PCs.
    Balanced,
    /// Más RAM y flags de G1GC afinados para modpacks pesados / shaders.
    Performance,
    /// RAM mínima viable, pensado para PCs con 8 GB o menos.
    LowRam,
}

impl Default for PerformanceProfile {
    fn default() -> Self {
        PerformanceProfile::Balanced
    }
}

pub const PROFILES: [PerformanceProfile; 3] = [
    PerformanceProfile::Balanced,
    PerformanceProfile::Performance,
    PerformanceProfile::LowRam,
];

impl PerformanceProfile {
    pub fn label(self) -> &'static str {
        match self {
            PerformanceProfile::Balanced => "Equilibrado",
            PerformanceProfile::Performance => "Rendimiento",
            PerformanceProfile::LowRam => "Ahorro de RAM",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            PerformanceProfile::Balanced => "RAM moderada, bueno para Vanilla y Fabric ligero.",
            PerformanceProfile::Performance => "Más RAM y G1GC afinado: modpacks grandes, shaders.",
            PerformanceProfile::LowRam => "El mínimo viable: PCs con 8 GB de RAM o menos.",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            PerformanceProfile::Balanced => "balanced",
            PerformanceProfile::Performance => "performance",
            PerformanceProfile::LowRam => "low_ram",
        }
    }

    pub fn from_key(key: &str) -> Self {
        match key {
            "performance" => PerformanceProfile::Performance,
            "low_ram" => PerformanceProfile::LowRam,
            _ => PerformanceProfile::Balanced,
        }
    }

    /// RAM sugerida para este perfil, respetando los límites globales.
    pub fn ram_mb(self) -> u32 {
        let suggested = match self {
            PerformanceProfile::Balanced => DEFAULT_RAM_MB,
            PerformanceProfile::Performance => 6144,
            PerformanceProfile::LowRam => 2048,
        };
        suggested.clamp(MIN_RAM_MB, MAX_RAM_MB)
    }

    /// Argumentos extra de JVM que complementan (no sustituyen) el G1GC base
    /// que ya aplica `optimize_jvm` en `core::launch`.
    pub fn extra_jvm_args(self) -> Vec<String> {
        match self {
            PerformanceProfile::Balanced => Vec::new(),
            PerformanceProfile::Performance => vec![
                "-XX:G1NewSizePercent=40".to_string(),
                "-XX:G1MaxNewSizePercent=60".to_string(),
                "-XX:G1HeapRegionSize=16M".to_string(),
                "-XX:MaxGCPauseMillis=25".to_string(),
            ],
            PerformanceProfile::LowRam => vec![
                "-XX:MaxGCPauseMillis=50".to_string(),
                "-XX:G1HeapRegionSize=4M".to_string(),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cada_perfil_tiene_ram_dentro_de_los_limites_globales() {
        for profile in PROFILES {
            let ram = profile.ram_mb();
            assert!(ram >= MIN_RAM_MB && ram <= MAX_RAM_MB);
        }
    }

    #[test]
    fn key_y_from_key_son_inversas() {
        for profile in PROFILES {
            assert_eq!(PerformanceProfile::from_key(profile.key()), profile);
        }
    }

    #[test]
    fn balanced_no_agrega_flags_extra() {
        assert!(PerformanceProfile::Balanced.extra_jvm_args().is_empty());
    }

    #[test]
    fn performance_pide_mas_ram_que_low_ram() {
        assert!(PerformanceProfile::Performance.ram_mb() > PerformanceProfile::LowRam.ram_mb());
    }
}
