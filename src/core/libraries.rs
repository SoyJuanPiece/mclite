//! Librerías: coordenadas maven, URLs, y — lo más delicado — la separación entre
//! librerías de classpath y librerías *natives*.
//!
//! Trampa verificada en el manifiesto real de 1.21.4: las natives vienen como
//! entradas de librería con clasificador (`...:natives-windows`, `...:natives-windows-x86`,
//! `...:natives-windows-arm64`) y sus reglas **solo filtran por `os.name`**, nunca por
//! `os.arch`. Evaluar las reglas y nada más mete DLLs de arm64 y x86 en una máquina x64.
//! Por eso aquí hay un filtro de arquitectura por token del clasificador.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::core::endpoints;
use crate::core::rules::{self, Arch, Environment, Features, Rule};

pub const OS_NAMES: [&str; 3] = ["windows", "osx", "linux"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    /// `grupo:artefacto:versión[:clasificador][@ext]`
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloads: Option<LibraryDownloads>,
    /// Formato antiguo: `{"windows": "natives-windows"}` → clave en `downloads.classifiers`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub natives: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules: Option<Vec<Rule>>,
    /// Maven del que bajar la librería (así las publica Fabric, sin `downloads`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extract: Option<Extract>,
    /// La generó el propio launcher (OptiFine parcheado, launchwrapper extraído):
    /// vive en `libraries/` pero no hay nada que descargar. Se persiste porque el
    /// manifiesto guardado se relee entre sesiones.
    #[serde(default)]
    pub local_only: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LibraryDownloads {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<Artifact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classifiers: Option<BTreeMap<String, Artifact>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha1: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default)]
    pub url: String,
}

/// Qué excluir al extraer (los manifiestos mandan `META-INF/` fuera).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Extract {
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MavenCoord {
    pub group: String,
    pub artifact: String,
    pub version: String,
    pub classifier: Option<String>,
    pub extension: String,
}

impl MavenCoord {
    pub fn parse(name: &str) -> Option<Self> {
        let (head, extension) = match name.split_once('@') {
            Some((head, ext)) if !ext.is_empty() => (head, ext.to_string()),
            _ => (name, "jar".to_string()),
        };
        let mut parts = head.split(':');
        let group = parts.next()?.trim();
        let artifact = parts.next()?.trim();
        let version = parts.next()?.trim();
        if group.is_empty() || artifact.is_empty() || version.is_empty() {
            return None;
        }
        let classifier = parts
            .next()
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .map(str::to_string);
        Some(Self {
            group: group.to_string(),
            artifact: artifact.to_string(),
            version: version.to_string(),
            classifier,
            extension,
        })
    }

    /// Ruta relativa dentro de `libraries/`, en formato maven.
    pub fn path(&self) -> String {
        let group = self.group.replace('.', "/");
        let file = match &self.classifier {
            Some(classifier) => format!(
                "{}-{}-{}.{}",
                self.artifact, self.version, classifier, self.extension
            ),
            None => format!("{}-{}.{}", self.artifact, self.version, self.extension),
        };
        format!("{}/{}/{}/{}", group, self.artifact, self.version, file)
    }

    pub fn classifier(&self) -> Option<&str> {
        self.classifier.as_deref()
    }
}

/// Librería ya resuelta para esta máquina.
#[derive(Debug, Clone)]
pub struct ResolvedLibrary {
    pub name: String,
    pub maven_path: String,
    /// `None` = no hay de dónde bajarla: tiene que existir ya en disco
    /// (p. ej. la librería de OptiFine, que la genera el propio launcher).
    pub url: Option<String>,
    pub sha1: Option<String>,
    pub size: Option<u64>,
    pub is_native: bool,
    pub extract_exclude: Vec<String>,
}

impl Library {
    pub fn coord(&self) -> Option<MavenCoord> {
        MavenCoord::parse(&self.name)
    }

    /// Clasificador que corresponde a este entorno, si la librería lo usa.
    fn native_classifier(&self, env: &Environment) -> Option<String> {
        // Formato antiguo: mapa explícito por sistema operativo.
        if let Some(natives) = &self.natives {
            if let Some(raw) = natives.get(&env.os_name) {
                return Some(substitute_arch(raw, env));
            }
            return None;
        }
        // Formato moderno: el clasificador va en el propio nombre.
        let classifier = self.coord()?.classifier;
        classifier.filter(|c| c.starts_with("natives-"))
    }

    pub fn is_native_library(&self, env: &Environment) -> bool {
        self.native_classifier(env).is_some()
    }

    /// Resuelve la librería para este entorno (o `None` si no aplica aquí).
    pub fn resolve(&self, env: &Environment, features: &Features) -> Option<ResolvedLibrary> {
        if !rules::allowed(self.rules.as_deref(), env, features) {
            return None;
        }

        let name = self.name.clone();
        let coord = self.coord()?;

        // 1) Formato antiguo: el artefacto concreto sale de `classifiers`.
        if self.natives.is_some() {
            let classifier = self.native_classifier(env)?;
            let downloads = self.downloads.as_ref()?;
            let artifact = downloads.classifiers.as_ref()?.get(&classifier)?;
            // El path del manifiesto manda si está.
            let maven_path = artifact
                .path
                .clone()
                .unwrap_or_else(|| coord.path());
            return Some(ResolvedLibrary {
                name,
                maven_path,
                url: Some(artifact.url.clone()),
                sha1: artifact.sha1.clone(),
                size: artifact.size,
                is_native: true,
                extract_exclude: self
                    .extract
                    .as_ref()
                    .map(|e| e.exclude.clone())
                    .unwrap_or_default(),
            });
        }

        // 2) Filtro de arquitectura por token del clasificador.
        if let Some(classifier) = coord.classifier() {
            if !classifier_arch_ok(classifier, env) {
                return None;
            }
        }

        let is_native = self.is_native_library(env);

        // 3) URL: `downloads.artifact` > `url` + ruta maven > maven de Mojang.
        //    Las librerías locales (generadas por el launcher) no tienen ninguna:
        //    sin la marca, caerían al maven de Mojang por defecto y darían 404.
        let (url, sha1, size, path) = if self.local_only {
            (None, None, None, None)
        } else {
            match self.downloads.as_ref().and_then(|d| d.artifact.as_ref()) {
                Some(artifact) => (
                    Some(artifact.url.clone()),
                    artifact.sha1.clone(),
                    artifact.size,
                    artifact.path.clone(),
                ),
                None => {
                    let base = self.url.as_deref().unwrap_or(endpoints::LIBRARIES_MAVEN);
                    let base = base.trim_end_matches('/');
                    (
                        Some(format!("{}/{}", base, coord.path())),
                        None,
                        None,
                        None,
                    )
                }
            }
        };

        Some(ResolvedLibrary {
            name,
            maven_path: path.unwrap_or_else(|| coord.path()),
            url,
            sha1,
            size,
            is_native,
            extract_exclude: self
                .extract
                .as_ref()
                .map(|e| e.exclude.clone())
                .unwrap_or_default(),
        })
    }
}

/// `natives-windows-${arch}` → `natives-windows-x86_64` etc.
fn substitute_arch(raw: &str, env: &Environment) -> String {
    if !raw.contains("${arch}") {
        return raw.to_string();
    }
    let arch = match env.os_arch {
        Arch::X86 => "x86",
        Arch::X64 => "x86_64",
        Arch::Arm64 => "arm64",
        Arch::Arm32 => "arm",
    };
    raw.replace("${arch}", arch)
}

/// ¿El clasificador encaja con esta arquitectura?
///
/// Un clasificador sin token de arquitectura (`natives-windows`) se acepta: ese es
/// el artefacto por defecto de la plataforma, que para nuestros targets es x64.
/// Uno con token (`natives-windows-x86`) solo vale si coincide.
pub fn classifier_arch_ok(classifier: &str, env: &Environment) -> bool {
    match arch_token(classifier) {
        Some(arch) => arch == env.os_arch,
        None => true,
    }
}

/// Extrae el token de arquitectura del final de un clasificador, si lo tiene.
fn arch_token(classifier: &str) -> Option<Arch> {
    let lower = classifier.to_ascii_lowercase();
    // `aarch_64` y `x86_64` llevan guion bajo, así que se comprueba por sufijo.
    for (suffix, arch) in [
        ("aarch_64", Arch::Arm64),
        ("aarch64", Arch::Arm64),
        ("x86_64", Arch::X64),
        ("arm64", Arch::Arm64),
        ("amd64", Arch::X64),
        ("arm32", Arch::Arm32),
        ("i386", Arch::X86),
        ("i686", Arch::X86),
        ("ia32", Arch::X86),
        ("x86", Arch::X86),
    ] {
        if lower == suffix || lower.ends_with(&format!("-{suffix}")) || lower.ends_with(&format!("_{suffix}")) {
            return Some(arch);
        }
    }
    None
}

/// Classpath ya listo para pasar a `-cp`. El separador es `;` en Windows y `:` en el resto.
pub fn classpath_string(entries: &[std::path::PathBuf]) -> String {
    let sep = if cfg!(windows) { ';' } else { ':' };
    entries
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(&sep.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::rules::Features;

    fn env(arch: Arch) -> Environment {
        Environment {
            os_name: "windows".into(),
            os_arch: arch,
            os_version: String::new(),
        }
    }

    fn lib(name: &str) -> Library {
        Library {
            name: name.into(),
            downloads: None,
            natives: None,
            rules: None,
            url: None,
            extract: None,
            local_only: false,
        }
    }

    #[test]
    fn parsea_coordenadas() {
        let c = MavenCoord::parse("org.lwjgl:lwjgl:3.3.3").unwrap();
        assert_eq!(c.group, "org.lwjgl");
        assert_eq!(c.classifier, None);
        assert_eq!(
            c.path(),
            "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar"
        );

        let c = MavenCoord::parse("org.lwjgl:lwjgl:3.3.3:natives-windows").unwrap();
        assert_eq!(c.classifier.as_deref(), Some("natives-windows"));
        assert_eq!(
            c.path(),
            "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar"
        );

        let c = MavenCoord::parse("net.minecraftforge:forge:1.20.1-47.2.0:universal@jar").unwrap();
        assert_eq!(c.classifier.as_deref(), Some("universal"));
        assert_eq!(c.extension, "jar");
    }

    #[test]
    fn filtra_natives_de_otras_arquitecturas() {
        // Este es el bug que evitamos: las reglas de 1.21.4 solo dicen "os: windows",
        // así que sin este filtro entrarían las natives de x86 y arm64.
        assert!(classifier_arch_ok("natives-windows", &env(Arch::X64)));
        assert!(!classifier_arch_ok("natives-windows-x86", &env(Arch::X64)));
        assert!(!classifier_arch_ok("natives-windows-arm64", &env(Arch::X64)));

        // Y al revés: en arm64 sí vale la de arm64.
        assert!(classifier_arch_ok("natives-windows-arm64", &env(Arch::Arm64)));
        // Clasificadores de netty: `linux-aarch_64` vs `linux-x86_64`.
        assert!(classifier_arch_ok("linux-aarch_64", &env(Arch::Arm64)));
        assert!(!classifier_arch_ok("linux-aarch_64", &env(Arch::X64)));
        assert!(classifier_arch_ok("linux-x86_64", &env(Arch::X64)));
        // Sin token: pasa.
        assert!(classifier_arch_ok("sources", &env(Arch::X64)));
    }

    #[test]
    fn resuelve_url_de_maven_propio() {
        // Así publica Fabric: `url` + ruta maven, sin `downloads`.
        let mut l = lib("net.fabricmc:intermediary:1.21.4");
        l.url = Some("https://maven.fabricmc.net/".into());
        let r = l.resolve(&env(Arch::X64), &Features::default()).unwrap();
        assert_eq!(
            r.url.as_deref(),
            Some("https://maven.fabricmc.net/net/fabricmc/intermediary/1.21.4/intermediary-1.21.4.jar")
        );
        assert!(!r.is_native);
    }

    #[test]
    fn resuelve_natives_del_formato_antiguo() {
        let mut l = lib("org.lwjgl.lwjgl:lwjgl-platform:2.9.4:natives-windows");
        let mut classifiers = BTreeMap::new();
        classifiers.insert(
            "natives-windows".to_string(),
            Artifact {
                path: None,
                sha1: Some("abc".into()),
                size: Some(10),
                url: "https://libraries.minecraft.net/x.jar".into(),
            },
        );
        l.downloads = Some(LibraryDownloads {
            artifact: None,
            classifiers: Some(classifiers),
        });
        l.natives = Some(BTreeMap::from([(
            "windows".to_string(),
            "natives-windows".to_string(),
        )]));

        let r = l.resolve(&env(Arch::X64), &Features::default()).unwrap();
        assert!(r.is_native);
        assert_eq!(r.sha1.as_deref(), Some("abc"));
    }

    #[test]
    fn sustitituye_arch_en_natives() {
        let env = env(Arch::Arm64);
        assert_eq!(substitute_arch("natives-windows-${arch}", &env), "natives-windows-arm64");
        assert_eq!(substitute_arch("natives-windows", &env), "natives-windows");
    }
}
