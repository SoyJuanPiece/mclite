//! Reglas del manifiesto: deciden qué librerías y qué argumentos aplican a esta máquina.
//!
//! Semántica oficial: si no hay reglas, aplica. Si hay, se recorren en orden y **la
//! última que hace match gana**. Arrancar con `allow = false` es intencional: un
//! manifiesto que solo dice "allow osx" excluye todo lo demás.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub action: RuleAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<OsRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub features: Option<std::collections::BTreeMap<String, bool>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleAction {
    Allow,
    Disallow,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsRule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
}

/// Arquitecturas tal y como las escriben los manifiestos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    X86,
    X64,
    Arm32,
    Arm64,
}

impl Arch {
    pub fn current() -> Self {
        if cfg!(target_arch = "x86_64") {
            Arch::X64
        } else if cfg!(target_arch = "x86") {
            Arch::X86
        } else if cfg!(target_arch = "aarch64") {
            Arch::Arm64
        } else if cfg!(target_arch = "arm") {
            Arch::Arm32
        } else {
            Arch::X64
        }
    }

    /// Normaliza lo que aparece en el JSON: Mojang escribe `x86`/`x86_64`,
    /// Java reporta `amd64`, los clasificadores usan `arm64`/`aarch_64`...
    pub fn parse(raw: &str) -> Option<Self> {
        let lower = raw.to_ascii_lowercase();
        // El orden importa: "x86_64" antes que "x86".
        if lower.contains("x86_64") || lower.contains("amd64") {
            Some(Arch::X64)
        } else if lower.contains("aarch64") || lower.contains("aarch_64") || lower.contains("arm64") {
            Some(Arch::Arm64)
        } else if lower.contains("arm32") || lower == "arm" || lower.contains("armv7") {
            Some(Arch::Arm32)
        } else if lower.contains("i386") || lower.contains("i686") || lower.contains("ia32") || lower == "x86" {
            Some(Arch::X86)
        } else {
            None
        }
    }
}

/// Entorno de ejecución que se compara contra las reglas.
#[derive(Debug, Clone)]
pub struct Environment {
    /// `windows` | `osx` | `linux`
    pub os_name: String,
    pub os_arch: Arch,
    /// Versión del SO. Vacío = desconocida; en ese caso las reglas que exigen
    /// una versión concreta no hacen match (nos quedamos del lado conservador).
    pub os_version: String,
}

impl Environment {
    pub fn current() -> Self {
        Self {
            os_name: current_os_name().to_string(),
            os_arch: Arch::current(),
            os_version: String::new(),
        }
    }

    /// Igual que `current` pero con la versión del SO, que solo se usa para
    /// reglas `os.version` (históricamente, todas de macOS).
    pub fn current_with_version(version: impl Into<String>) -> Self {
        Self {
            os_version: version.into(),
            ..Self::current()
        }
    }
}

pub fn current_os_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    }
}

/// Features que el launcher puede activar. Todas en `false` por defecto: es lo que
/// quiere un launcher normal (sin demo, sin quick-play).
#[derive(Debug, Clone, Default)]
pub struct Features {
    pub is_demo_user: bool,
    pub has_custom_resolution: bool,
    pub has_quick_plays_support: bool,
    pub is_quick_play_singleplayer: bool,
    pub is_quick_play_multiplayer: bool,
    pub is_quick_play_realms: bool,
}

impl Features {
    pub fn get(&self, key: &str) -> bool {
        match key {
            "is_demo_user" => self.is_demo_user,
            "has_custom_resolution" => self.has_custom_resolution,
            "has_quick_plays_support" => self.has_quick_plays_support,
            "is_quick_play_singleplayer" => self.is_quick_play_singleplayer,
            "is_quick_play_multiplayer" => self.is_quick_play_multiplayer,
            "is_quick_play_realms" => self.is_quick_play_realms,
            _ => false,
        }
    }
}

impl Rule {
    pub fn matches(&self, env: &Environment, features: &Features) -> bool {
        if let Some(os) = &self.os {
            if let Some(name) = &os.name {
                if name != &env.os_name {
                    return false;
                }
            }
            if let Some(arch) = &os.arch {
                match Arch::parse(arch) {
                    Some(arch) if arch == env.os_arch => {}
                    _ => return false,
                }
            }
            if let Some(pattern) = &os.version {
                if env.os_version.is_empty() {
                    return false;
                }
                let Ok(re) = regex_lite::Regex::new(pattern) else {
                    // Un patrón raro no debe romper el launch: lo tratamos como no-match.
                    return false;
                };
                if !re.is_match(&env.os_version) {
                    return false;
                }
            }
        }
        if let Some(wanted) = &self.features {
            for (key, value) in wanted {
                if features.get(key) != *value {
                    return false;
                }
            }
        }
        true
    }
}

/// ¿Las reglas permiten este elemento en este entorno?
pub fn allowed(rules: Option<&[Rule]>, env: &Environment, features: &Features) -> bool {
    let Some(rules) = rules else {
        return true;
    };
    if rules.is_empty() {
        return true;
    }
    let mut allow = false;
    for rule in rules {
        if rule.matches(env, features) {
            allow = rule.action == RuleAction::Allow;
        }
    }
    allow
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn env_windows_x64() -> Environment {
        Environment {
            os_name: "windows".into(),
            os_arch: Arch::X64,
            os_version: String::new(),
        }
    }

    fn rule(action: RuleAction, os: Option<OsRule>) -> Rule {
        Rule {
            action,
            os,
            features: None,
        }
    }

    #[test]
    fn sin_reglas_aplica() {
        let f = Features::default();
        assert!(allowed(None, &env_windows_x64(), &f));
        assert!(allowed(Some(&[]), &env_windows_x64(), &f));
    }

    #[test]
    fn allow_osx_excluye_windows() {
        let f = Features::default();
        let rules = vec![rule(
            RuleAction::Allow,
            Some(OsRule {
                name: Some("osx".into()),
                version: None,
                arch: None,
            }),
        )];
        assert!(!allowed(Some(&rules), &env_windows_x64(), &f));
    }

    #[test]
    fn allow_windows_incluye_windows() {
        let f = Features::default();
        let rules = vec![rule(
            RuleAction::Allow,
            Some(OsRule {
                name: Some("windows".into()),
                version: None,
                arch: None,
            }),
        )];
        assert!(allowed(Some(&rules), &env_windows_x64(), &f));
    }

    #[test]
    fn la_ultima_regla_que_hace_match_gana() {
        let f = Features::default();
        // allow windows, luego disallow windows => excluido
        let rules = vec![
            rule(
                RuleAction::Allow,
                Some(OsRule {
                    name: Some("windows".into()),
                    version: None,
                    arch: None,
                }),
            ),
            rule(
                RuleAction::Disallow,
                Some(OsRule {
                    name: Some("windows".into()),
                    version: None,
                    arch: None,
                }),
            ),
        ];
        assert!(!allowed(Some(&rules), &env_windows_x64(), &f));
    }

    #[test]
    fn regla_de_arquitectura_x86_no_aplica_a_x64() {
        let f = Features::default();
        let rules = vec![rule(
            RuleAction::Allow,
            Some(OsRule {
                name: None,
                version: None,
                arch: Some("x86".into()),
            }),
        )];
        assert!(!allowed(Some(&rules), &env_windows_x64(), &f));
    }

    #[test]
    fn features_filtran() {
        let mut feats = Features::default();
        let mut wanted = BTreeMap::new();
        wanted.insert("is_demo_user".to_string(), false);

        let rules = vec![Rule {
            action: RuleAction::Allow,
            os: None,
            features: Some(wanted),
        }];
        assert!(allowed(Some(&rules), &env_windows_x64(), &feats));

        feats.is_demo_user = true;
        assert!(!allowed(Some(&rules), &env_windows_x64(), &feats));
    }

    #[test]
    fn normaliza_arquitecturas() {
        assert_eq!(Arch::parse("x86_64"), Some(Arch::X64));
        assert_eq!(Arch::parse("amd64"), Some(Arch::X64));
        assert_eq!(Arch::parse("x86"), Some(Arch::X86));
        assert_eq!(Arch::parse("aarch_64"), Some(Arch::Arm64));
        assert_eq!(Arch::parse("arm64"), Some(Arch::Arm64));
        assert_eq!(Arch::parse("natives-windows"), None);
    }
}
