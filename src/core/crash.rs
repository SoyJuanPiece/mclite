//! Captura de crashes del juego (`logs/crash/`).
//!
//! El log del juego vuela con la ventana: aquí se guarda un espejo de su salida en
//! `logs/crash/<version>-<timestamp>.log` y, si acabó mal, se extrae la causa
//! probable para mostrarla en la UI sin pedirle al usuario que navegue ficheros.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::core::paths::Paths;

/// Espejo de la salida del juego: recibe las mismas líneas que la UI y las escribe
/// a `logs/crash/<version>-<timestamp>.log`. Devuelve la ruta del log escrito.
pub struct GameLogMirror {
    file: Option<std::fs::File>,
    path: PathBuf,
}

impl GameLogMirror {
    /// Crea el espejo. Fallo tolerable: si no se puede escribir, la UI sigue
    /// mostrando el log en vivo.
    pub fn new(paths: &Paths, version_id: &str) -> Self {
        let dir = paths.logs().join("crash");
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let path = dir.join(format!(
            "{}-{}.log",
            crate::core::paths::sanitize(version_id),
            seconds
        ));
        let file = std::fs::create_dir_all(&dir)
            .ok()
            .and_then(|()| {
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .ok()
            });
        Self { file, path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Escribe una línea del juego al espejo. Pública: el llamador decide qué
    /// líneas van (las mismas que pinta la UI).
    pub fn write_line(&mut self, line: &str) {
        if let Some(file) = self.file.as_mut() {
            let _ = writeln!(file, "{line}");
        }
    }

    /// Cierra el fichero y devuelve la ruta del log.
    pub fn finish(mut self) -> PathBuf {
        self.file = None;
        self.path
    }
}

/// Resultado de una partida terminada, listo para la UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameExit {
    pub ok: bool,
    pub code: i32,
    /// Causa probable si `!ok` (del crash report o de la salida del juego).
    pub cause: Option<String>,
    /// Log completo espejado, para el botón «Abrir log».
    pub log_path: PathBuf,
    /// Crash report oficial de Mojang si lo hubo (`crash-reports/` del gameDir).
    pub crash_report: Option<PathBuf>,
}

/// Clasifica el final de una partida: código de salida, causa probable (del crash
/// report oficial o de heurísticas sobre la salida) y rutas de logs. `tail` son las
/// últimas líneas del juego (las mismas que pinta la UI).
pub fn classify(
    status: std::io::Result<std::process::ExitStatus>,
    game_dir: &Path,
    log_path: PathBuf,
    tail: &[String],
) -> GameExit {
    let (ok, code) = match status {
        Ok(status) => (status.success(), status.code().unwrap_or(-1)),
        Err(_) => (false, -1),
    };

    let crash_report = if ok { None } else { latest_crash_report(game_dir) };
    let cause = if ok {
        None
    } else {
        crash_report
            .as_deref()
            .and_then(read_crash_cause)
            .or_else(|| guess_cause(tail))
            .or_else(|| crash_report.as_ref().map(|report| {
                format!("revisa el crash report completo: {}", report.display())
            }))
    };

    GameExit {
        ok,
        code,
        cause,
        log_path,
        crash_report,
    }
}

/// El crash report más reciente de `gameDir/crash-reports/` (los últimos 10 min).
fn latest_crash_report(game_dir: &Path) -> Option<PathBuf> {
    let dir = game_dir.join("crash-reports");
    let newest = std::fs::read_dir(&dir)
        .ok()?
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "txt"))
        .max_by_key(|entry| {
            entry
                .metadata()
                .and_then(|meta| meta.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
        })?
        .path();

    // Solo cuenta si es de esta partida (recién creado).
    let fresh = std::fs::metadata(&newest)
        .and_then(|meta| meta.modified())
        .ok()?
        .elapsed()
        .map(|age| age.as_secs() < 600)
        .unwrap_or(false);
    fresh.then_some(newest)
}

/// La primera línea `Description:` del crash report de Mojang resume la causa.
fn read_crash_cause(report: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(report).ok()?;
    for line in raw.lines() {
        let line = line.trim();
        if let Some(description) = line.strip_prefix("Description: ") {
            return Some(description.trim().to_string());
        }
    }
    None
}

/// Heurísticas sobre las últimas líneas del juego cuando no hay crash report:
/// cubren los fallos que más se ven en la práctica.
fn guess_cause(tail: &[String]) -> Option<String> {
    let joined: String = tail.join("\n");
    let patterns: [(&str, &str); 8] = [
        (
            "UnsupportedClassVersionError",
            "Java de la versión incorrecta: esta versión de Minecraft necesita otro Java",
        ),
        (
            "NoClassDefFoundError",
            "falta una clase (suele ser una librería o mod incompatible)",
        ),
        (
            "UnsatisfiedLinkError",
            "falta una librería nativa (natives no extraídas o arquitectura incorrecta)",
        ),
        (
            "CreateFailedException",
            "no se pudo crear el mundo (¿disco lleno o permisos?)",
        ),
        (
            "OutOfMemoryError",
            "se quedó sin memoria: sube la RAM de la instancia",
        ),
        (
            "OpenGL error",
            "error gráfico: actualiza los drivers de la GPU",
        ),
        (
            "Failed to check session lock",
            "otro Minecraft está usando esta carpeta, ciérralo",
        ),
        (
            "access denied",
            "permiso denegado: revisa que la carpeta no sea de solo lectura",
        ),
    ];
    for (needle, cause) in patterns {
        if joined.contains(needle) {
            return Some(cause.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lee_la_description_del_crash_report() {
        let dir = std::env::temp_dir().join("mclite-crash-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let report = dir.join("crash-2026-09-24.txt");
        std::fs::write(
            &report,
            "---- Minecraft Crash Report ----\n// Quite honestly, I hardly bother...\n\nDescription: Rendering overlay\n\njava.lang.IllegalStateException: ...\n",
        )
        .unwrap();

        assert_eq!(read_crash_cause(&report), Some("Rendering overlay".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reconoce_los_fallos_tipicos() {
        let tail = vec![
            "Exception in thread \"main\"".to_string(),
            "java.lang.UnsupportedClassVersionError: net/minecraft/client/main/Main has been compiled by a more recent version".to_string(),
        ];
        let cause = guess_cause(&tail).unwrap();
        assert!(cause.contains("Java"));
    }

    #[test]
    fn sin_señales_no_inventa_causa() {
        assert!(guess_cause(&["[render thread/INFO]: hola".into()]).is_none());
    }
}
