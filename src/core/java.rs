//! Detección de Java.
//!
//! Fase 1 solo detecta lo que ya está instalado. Descargar el runtime de Mojang
//! (`java-runtime-*`) es la Fase 3 — ver PLAN.md.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::core::process::hidden_command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaInstallation {
    pub path: PathBuf,
    pub major: u32,
    /// Cadena de versión tal cual la reporta el JVM (`21.0.4`, `1.8.0_402`...).
    pub version: String,
    /// De dónde salió, para poder mostrarlo en Ajustes.
    pub source: String,
}

impl JavaInstallation {
    pub fn label(&self) -> String {
        format!("Java {} ({})", self.major, self.path.display())
    }
}

pub fn java_binary_name() -> &'static str {
    if cfg!(windows) {
        "java.exe"
    } else {
        "java"
    }
}

/// En Windows devuelve el `javaw.exe` de la misma carpeta: es la misma JVM pero
/// no abre una ventana de consola junto al juego. Si no existe (JRE raros), se
/// queda con `java.exe`. En otros sistemas es un no-op.
pub fn javaw_path(java: &Path) -> PathBuf {
    if !cfg!(windows) {
        return java.to_path_buf();
    }
    let is_java_exe = java
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("java.exe"));
    if !is_java_exe {
        return java.to_path_buf();
    }
    let javaw = java.with_file_name("javaw.exe");
    if javaw.is_file() {
        javaw
    } else {
        java.to_path_buf()
    }
}

/// Busca todas las instalaciones que pueda encontrar y las devuelve ordenadas
/// de mayor a menor versión.
pub fn detect_all() -> Vec<JavaInstallation> {
    let mut found = Vec::new();
    let mut seen = HashSet::new();

    for (candidate, source) in candidate_paths() {
        let Ok(canonical) = candidate.canonicalize() else {
            continue;
        };
        if !seen.insert(canonical) {
            continue;
        }
        if let Some((major, version)) = probe(&candidate) {
            found.push(JavaInstallation {
                path: candidate,
                major,
                version,
                source,
            });
        }
    }

    found.sort_by(|a, b| b.major.cmp(&a.major).then_with(|| a.path.cmp(&b.path)));
    found
}

/// Ejecuta `java -version` y saca la versión. `None` si no es un JVM usable.
pub fn probe(java: &Path) -> Option<(u32, String)> {
    let output = hidden_command(java).arg("-version").output().ok()?;
    let mut text = String::from_utf8_lossy(&output.stderr).to_string();
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    parse_version_output(&text)
}

/// Extrae la versión de la salida de `java -version`.
pub fn parse_version_output(output: &str) -> Option<(u32, String)> {
    let start = output.find('"')?;
    let rest = &output[start + 1..];
    let end = rest.find('"')?;
    parse_version_string(&rest[..end])
}

/// `21.0.4` → `(21, "21.0.4")`; `1.8.0_402` → `(8, "1.8.0_402")`.
pub fn parse_version_string(version: &str) -> Option<(u32, String)> {
    let mut parts = version.split(['.', '_', '-']);
    let first = parts.next()?;
    let major = if first == "1" {
        // Esquema antiguo: el número real es el segundo componente.
        parts.next()?.parse().ok()?
    } else {
        first.parse().ok()?
    };
    Some((major, version.to_string()))
}    /// Elige el Java para una versión de Minecraft: primero el que coincide exacto;
    /// si no, el más bajo que lo supere, pero SOLO cuando el pedido es moderno
    /// (17+): ejecutar versiones antiguas (Java 8) con un JVM moderno se rompe con
    /// frecuencia, y para eso está el `jre-legacy` de Mojang (`core::runtime`).
    /// Devuelve `None` si nada cubre el requisito con garantías.
    pub fn select_for(required_major: u32, found: &[JavaInstallation]) -> Option<JavaInstallation> {
        if let Some(exact) = found.iter().find(|j| j.major == required_major) {
            return Some(exact.clone());
        }
        if required_major >= 17 {
            if let Some(next) = found
                .iter()
                .filter(|j| j.major > required_major)
                .min_by_key(|j| j.major)
            {
                return Some(next.clone());
            }
        }
        None
    }

fn candidate_paths() -> Vec<(PathBuf, String)> {
    let mut out: Vec<(PathBuf, String)> = Vec::new();
    let binary = java_binary_name();

    if let Some(home) = std::env::var_os("JAVA_HOME") {
        out.push((PathBuf::from(&home).join("bin").join(binary), "JAVA_HOME".into()));
    }

    // Raíces del sistema donde suelen vivir los JVM, escaneadas hasta 3 niveles.
    let mut roots: Vec<(PathBuf, String)> = Vec::new();
    if let Some(dir) = std::env::var_os("ProgramFiles") {
        roots.push((PathBuf::from(dir), "%ProgramFiles%".into()));
    }
    if let Some(dir) = std::env::var_os("ProgramFiles(x86)") {
        roots.push((PathBuf::from(dir), "%ProgramFiles(x86)%".into()));
    }
    if let Some(dir) = std::env::var_os("LOCALAPPDATA") {
        roots.push((
            PathBuf::from(dir).join("Programs"),
            "%LOCALAPPDATA%\\Programs".into(),
        ));
    }
    for dir in ["/usr/lib/jvm", "/opt/java", "/opt"] {
        roots.push((PathBuf::from(dir), dir.to_string()));
    }
    roots.push((
        PathBuf::from("/Library/Java/JavaVirtualMachines"),
        "macOS JVMs".into(),
    ));

    for (root, source) in roots {
        for java in scan_for_java(&root, 3) {
            out.push((java, source.clone()));
        }
    }

    // Y lo que haya en el PATH.
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join(binary);
            if candidate.is_file() {
                out.push((candidate, "PATH".into()));
            }
        }
    }

    out
}

/// Busca `<dir>/**/bin/java[.exe]` hasta `depth` niveles.
fn scan_for_java(dir: &Path, depth: usize) -> Vec<PathBuf> {
    let mut found = Vec::new();
    if depth == 0 {
        return found;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let candidate = path.join("bin").join(java_binary_name());
        if candidate.is_file() {
            found.push(candidate);
        } else {
            found.extend(scan_for_java(&path, depth - 1));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsea_esquema_moderno() {
        assert_eq!(parse_version_string("21.0.4"), Some((21, "21.0.4".into())));
        assert_eq!(parse_version_string("17.0.15"), Some((17, "17.0.15".into())));
    }

    #[test]
    fn parsea_esquema_antiguo_1_x() {
        assert_eq!(parse_version_string("1.8.0_402"), Some((8, "1.8.0_402".into())));
        assert_eq!(parse_version_string("1.7.0_80"), Some((7, "1.7.0_80".into())));
    }

    #[test]
    fn parsea_la_salida_real_de_java_version() {
        let openjdk = "openjdk version \"21.0.4\" 2024-07-16 LTS\nOpenJDK Runtime Environment (build 21.0.4+7)\n";
        assert_eq!(parse_version_output(openjdk), Some((21, "21.0.4".into())));

        let legacy = "java version \"1.8.0_402\"\nJava(TM) SE Runtime Environment\n";
        assert_eq!(parse_version_output(legacy), Some((8, "1.8.0_402".into())));

        assert_eq!(parse_version_output("command not found"), None);
    }

    fn fake(major: u32, path: &str) -> JavaInstallation {
        JavaInstallation {
            path: PathBuf::from(path),
            major,
            version: major.to_string(),
            source: "test".into(),
        }
    }

    #[test]
    fn elige_el_java_que_toca() {
        let found = vec![fake(8, "/j8"), fake(17, "/j17"), fake(21, "/j21")];

        // Coincidencia exacta.
        assert_eq!(select_for(17, &found).unwrap().path, PathBuf::from("/j17"));
        assert_eq!(select_for(21, &found).unwrap().path, PathBuf::from("/j21"));

        // Moderno sin exacto: el más bajo que lo supere.
        assert_eq!(select_for(18, &found).unwrap().path, PathBuf::from("/j21"));

        // Antiguo sin exacto (p. ej. 1.8.9 con solo Java 17/21): None → el
        // llamador baja el jre-legacy de Mojang, que es lo fiable.
        assert!(select_for(16, &found).is_none());
        let modernos = vec![fake(17, "/j17"), fake(21, "/j21")];
        assert!(select_for(8, &modernos).is_none());

        // Moderno más nuevo que todo lo instalado: None → runtime de Mojang.
        assert!(select_for(25, &found).is_none());

        assert!(select_for(21, &[]).is_none());
    }
}
