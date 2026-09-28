//! Captura de crashes del juego (`logs/crash/`).
//!
//! Cada partida deja UN log con nombre legible:
//! `logs/crash/2026-09-28_184512_mi-instancia__CRASH.log` (o `__OK.log`). La hora es
//! local (España peninsular, UTC+2) y es la del ARRANQUE de la sesión: coincide con
//! lo que vio el usuario en el reloj, no con cuándo terminó la partida.
//!
//! A la hora de depurar un crash conviene todo lo que haya caído en disco, así que
//! `GameExit` recoge además (si es de esta partida):
//! * `crash-reports/crash-*.txt` del gameDir — el crash report de Mojang;
//! * `hs_err_pid*.log` del gameDir — lo único que deja la JVM cuando muere sin
//!   reporte de Mojang (típico de shaders/drivers: el juego se mata a sí mismo y no
//!   hay stack de Java que analizable);
//! * `logs/latest.log` del gameDir — el log que escribe el propio Minecraft.
//!
//! Podas: en `logs/crash/` viven las últimas `KEEP_GAME_LOGS` sesiones de cada tipo
//! (OK y CRASH se cuentan por separado), para que la carpeta no crezca sin fin.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::core::paths::Paths;

/// ¿Cuántos logs de sesiones pasadas se conservan? (OK y CRASH se podan por separado.)
const KEEP_GAME_LOGS: usize = 40;

/// Espejo de la salida del juego: recibe las mismas líneas que la UI y las escribe
/// a `logs/crash/<fecha>_<hora>_<instancia>__OK|CRASH.log`. Devuelve la ruta del log escrito.
pub struct GameLogMirror {
    file: Option<std::fs::File>,
    path: PathBuf,
}

impl GameLogMirror {
    /// Crea el espejo. Fallo tolerable: si no se puede escribir, la UI sigue
    /// mostrando el log en vivo.
    pub fn new(paths: &Paths, instance_slug: &str) -> Self {
        let dir = paths.logs().join("crash");
        let path = dir.join(format!(
            "{}_{}.log",
            stamp(SystemTime::now()),
            crate::core::paths::sanitize(instance_slug)
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

    /// Cierra el fichero, lo renombra a su nombre final (`...__OK.log` o
    /// `...__CRASH.log`) y poda los logs antiguos. Devuelve la ruta final.
    pub fn finish(mut self, ok: bool) -> PathBuf {
        self.file = None;
        let final_path = con_sufijo(&self.path, if ok { "__OK.log" } else { "__CRASH.log" });
        // Si el rename falla (¿otro proceso?), queda el provisional: sirve igual.
        let final_path = std::fs::rename(&self.path, &final_path)
            .map(|()| final_path)
            .unwrap_or_else(|_| self.path.clone());
        prune(self.path.parent().unwrap_or(Path::new(".")));
        final_path
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
    /// Log nativo de la JVM (`hs_err_pid*.log`) si lo hubo: es lo que queda cuando
    /// el juego muere SIN reporte de Mojang (shaders, drivers, memoria…).
    pub jvm_log: Option<PathBuf>,
    /// `logs/latest.log` del gameDir si es de esta partida (lo escribe el propio Minecraft).
    pub game_latest_log: Option<PathBuf>,
}

/// Clasifica el final de una partida: código de salida, causa probable (del crash
/// report oficial o de heurísticas sobre la salida) y rutas de logs. `tail` son las
/// últimas líneas del juego (las mismas que pinta la UI); `session_start` es cuándo
/// arrancó (para saber si los ficheros del gameDir son de esta partida).
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

    let mut crash_report = None;
    let mut jvm_log = None;
    let mut game_latest = None;
    if !ok {
        crash_report = latest_crash_report(game_dir);
        jvm_log = latest_jvm_log(game_dir);
        game_latest = game_latest_log(game_dir);
    }
    // Se llama justo al terminar la partida: «fresco» (últimos 10 min) basta para
    // saber que el fichero del gameDir es de esta sesión.

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
        jvm_log,
        game_latest_log: game_latest,
    }
}

/// El crash report más reciente de `gameDir/crash-reports/` (los últimos 10 min).
fn latest_crash_report(game_dir: &Path) -> Option<PathBuf> {
    recent_file_in(&game_dir.join("crash-reports"), |name| {
        name.starts_with("crash-") && name.ends_with(".txt")
    })
}

/// El `hs_err_pid*.log` más reciente del gameDir: lo único que deja la JVM cuando
/// muere sin crash report de Mojang (shaders y drivers gráficos, sobre todo).
/// Solo cuenta si es de esta partida (escrito en los últimos 10 minutos).
fn latest_jvm_log(game_dir: &Path) -> Option<PathBuf> {
    recent_file_in(game_dir, |name| name.starts_with("hs_err_pid"))
}

/// `logs/latest.log` del gameDir: el log que escribe el propio Minecraft. Solo
/// cuenta si es de esta partida (escrito en los últimos 10 minutos).
fn game_latest_log(game_dir: &Path) -> Option<PathBuf> {
    recent_file_in(&game_dir.join("logs"), |name| name == "latest.log")
}

/// El fichero más reciente de `dir` cuyo nombre pasa `matches_name`, siempre que sea
/// fresco (escrito en los últimos 10 minutos: si no, es de otra partida).
fn recent_file_in(dir: &Path, matches_name: impl Fn(&str) -> bool) -> Option<PathBuf> {
    let newest = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| matches_name(name))
        })
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

// ── Nombres de fichero y poda ────────────────────────────────────────────────

/// Marca temporal legible para el nombre de fichero: `2026-09-28_184512`
/// (hora local de España peninsular, UTC+2).
fn stamp(t: SystemTime) -> String {
    let (year, month, day, h, m, s) = timestamp_to_ymdhms(unix_secs(t), SPAIN_UTC_OFFSET_SECS);
    format!("{year:04}-{month:02}-{day:02}_{h:02}{m:02}{s:02}")
}

/// Offset fijo de España peninsular en segundos (UTC+2, horario de verano).
/// No es exacto en invierno (UTC+1), pero el nombre es orientativo: solo importa
/// que la hora sea la que ve el usuario en el reloj la mayor parte del año.
const SPAIN_UTC_OFFSET_SECS: i64 = 2 * 3600;

/// Añade el sufijo final al log (`...__OK.log` / `...__CRASH.log`), sustituyendo
/// la extensión provisional.
fn con_sufijo(path: &Path, sufijo: &str) -> PathBuf {
    let nombre = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("sesion.log");
    let base = nombre.strip_suffix(".log").unwrap_or(nombre);
    path.with_file_name(format!("{base}{sufijo}"))
}

/// Clase de log por nombre: 1 = OK, 2 = CRASH, 0 = provisional sin cerrar
/// (el launcher murió antes de `finish`).
fn kind_of(name: &str) -> u8 {
    if name.ends_with("__CRASH.log") {
        2
    } else if name.ends_with("__OK.log") {
        1
    } else {
        0
    }
}

/// Deja en `dir` solo las últimas `KEEP_GAME_LOGS` sesiones de cada tipo (OK y
/// CRASH se podan por separado) y los provisionales con más de una hora sin
/// escribir (una sesión en vivo escribe constantemente; uno muerto ya no sirve).
/// Devuelve cuántos ficheros borró.
fn prune(dir: &Path) -> usize {
    let mut grupos: [Vec<(SystemTime, PathBuf)>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    if let Ok(read) = std::fs::read_dir(dir) {
        for entry in read.flatten() {
            let path = entry.path();
            if !path.extension().is_some_and(|ext| ext == "log") {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            let Ok(modified) = meta.modified() else { continue };
            let kind = kind_of(&entry.file_name().to_string_lossy());
            grupos[kind as usize].push((modified, path));
        }
    }

    let ahora = SystemTime::now();
    let una_hora = std::time::Duration::from_secs(3600);
    let mut removed = 0;
    for (kind, grupo) in grupos.iter_mut().enumerate() {
        grupo.sort_by(|a, b| b.0.cmp(&a.0));
        if kind == 0 {
            // Provisionales: fuera si llevan una hora sin escribirse.
            for (modified, path) in grupo.iter() {
                if ahora
                    .duration_since(*modified)
                    .is_ok_and(|age| age > una_hora)
                {
                    if std::fs::remove_file(path).is_ok() {
                        removed += 1;
                    }
                }
            }
        } else {
            for (_, path) in grupo.iter().skip(KEEP_GAME_LOGS) {
                if std::fs::remove_file(path).is_ok() {
                    removed += 1;
                }
            }
        }
    }
    removed
}

// ── Tiempo sin cronologia ────────────────────────────────────────────────────

fn unix_secs(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Segundos desde la época a (año, mes, día, hora, minuto, segundo) en el offset
/// dado. Sin `chrono`: es lo único que necesitamos y las dependencias pesan.
fn timestamp_to_ymdhms(secs: u64, offset_secs: i64) -> (i64, u32, u32, u32, u32, u32) {
    let total = secs as i64 + offset_secs;
    let days = total.div_euclid(86_400);
    let rem = total.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    (
        year,
        month,
        day,
        (rem / 3600) as u32,
        ((rem % 3600) / 60) as u32,
        (rem % 60) as u32,
    )
}

/// Días desde la época a (año, mes, día) — algoritmo «civil_from_days» de Hinnant.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
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

    #[test]
    fn la_epoca_es_el_1_de_enero_de_1970() {
        assert_eq!(timestamp_to_ymdhms(0, 0), (1970, 1, 1, 0, 0, 0));
        // 365 días: 1970 no fue bisiesto.
        assert_eq!(timestamp_to_ymdhms(365 * 86_400, 0), (1971, 1, 1, 0, 0, 0));
    }

    #[test]
    fn fecha_conocida_con_hora_y_offset() {
        // 2024-01-01 son 19.723 días desde la época (54 años, 13 bisiestos).
        let base = 19_723 * 86_400;
        assert_eq!(
            timestamp_to_ymdhms(base + 18 * 3600 + 45 * 60 + 12, 0),
            (2024, 1, 1, 18, 45, 12)
        );
        // Con UTC+2 cruza a las 20:45.
        assert_eq!(
            timestamp_to_ymdhms(base + 18 * 3600 + 45 * 60 + 12, SPAIN_UTC_OFFSET_SECS),
            (2024, 1, 1, 20, 45, 12)
        );
    }

    #[test]
    fn los_nombres_de_sesion_llevan_el_sufijo_final() {
        let base = Path::new("/x/2026-09-28_184512_mi-instancia.log");
        assert_eq!(
            con_sufijo(base, "__CRASH.log"),
            PathBuf::from("/x/2026-09-28_184512_mi-instancia__CRASH.log")
        );
        assert_eq!(
            con_sufijo(base, "__OK.log"),
            PathBuf::from("/x/2026-09-28_184512_mi-instancia__OK.log")
        );
        assert_eq!(kind_of("2026-09-28_184512_mi-instancia__CRASH.log"), 2);
        assert_eq!(kind_of("2026-09-28_184512_mi-instancia__OK.log"), 1);
        assert_eq!(kind_of("2026-09-28_184512_mi-instancia.log"), 0);
    }
}
