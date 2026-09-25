//! Log de arranque del launcher (`logs/launcher.log`).
//!
//! Distinto del log del juego: esto es lo que hace McLite mientras prepara y lanza
//! (manifiestos, descargas, runtime de Java, errores). Con subsistema "windows" no
//! hay consola, así que sin esto un problema de arranque sería invisible.
//!
//! Un singleton global (`OnceLock`) porque se escribe desde varios hilos y el
//! alternativo (arrastrar el logger por todas las firmas) ensucia media API por
//! una línea de diagnóstico.

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::core::paths::Paths;

/// Rotación: a partir de este tamaño el log actual pasa a `launcher.old.log`.
const ROTATE_BYTES: u64 = 1_000_000;

struct Inner {
    file: Option<File>,
}

static LOG: OnceLock<Mutex<Inner>> = OnceLock::new();

/// Activa el log de arranque. Llamar una vez, nada más conocer la raíz de datos.
/// Fallo silencioso: si no se puede escribir, el launcher funciona igual.
pub fn init(paths: &Paths) {
    let _ = LOG.set(Mutex::new(Inner {
        file: open_log_file(paths),
    }));
}

fn open_log_file(paths: &Paths) -> Option<File> {
    let dir = paths.logs();
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join("launcher.log");

    // Rota si el actual creció demasiado.
    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() > ROTATE_BYTES {
            let old = dir.join("launcher.old.log");
            let _ = std::fs::remove_file(&old);
            let _ = std::fs::rename(&path, &old);
        }
    }

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok()?;
    let _ = writeln!(file, "──────── McLite {} ────────", crate::LAUNCHER_VERSION);
    Some(file)
}

/// Escribe una línea con timestamp. Es tolerante a no estar inicializado.
pub fn write(level: &str, message: &str) {
    let Some(log) = LOG.get() else { return };
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let Ok(mut inner) = log.lock() else { return };
    let Some(file) = inner.file.as_mut() else { return };
    let _ = writeln!(file, "[{seconds}] [{level}] {message}");
}

pub fn info(message: &str) {
    write("INFO", message);
}

pub fn warn(message: &str) {
    write("WARN", message);
}

pub fn error(message: &str) {
    write("ERROR", message);
}

/// Ruta del log de arranque (para el botón de Ajustes). `None` si aún no se inició.
pub fn file_path(paths: &Paths) -> PathBuf {
    paths.logs().join("launcher.log")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escribir_sin_iniciar_no_peta() {
        // No llamó a init: debe ser un no-op, no un panic.
        info("esto no aparece en ningún fichero");
    }
}
