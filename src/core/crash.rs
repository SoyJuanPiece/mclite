//! Logs de sesión del juego, crashes y mods.
//!
//! El launcher separa a propósito tres cosas que se depuran de forma distinta,
//! cada una en su carpeta dentro de `logs/`:
//!
//! ```text
//! logs/
//! ├── launcher.log                          lo que hace McLite (core/logging.rs)
//! ├── game/<instancia>/<fecha>__OK.log      la sesión completa, salga bien o mal
//! ├── game/<instancia>/<fecha>__CRASH.log
//! ├── mods/<instancia>/<fecha>.log          solo lo de mods (lista + líneas)
//! └── crash/<instancia>/<fecha>/            expediente autocontenido del fallo
//!     ├── resumen.txt                      cabecera legible: causa, versión, rutas
//!     ├── sesion.log                       copia de la sesión completa
//!     ├── mods.log                         copia del log de mods
//!     ├── minecraft-crash-report.txt       el reporte oficial de Mojang
//!     ├── minecraft-latest.log             logs/latest.log que escribe el juego
//!     └── jvm-hs-err.log                   el `hs_err_pid*.log` de la JVM
//! ```
//!
//! La idea del expediente es que se pueda enviar TAL CUAL (comprimido) a quien
//! te vaya a ayudar: no hay que ir a buscar ficheros sueltos por el gameDir.
//!
//! Podas: `game/` y `mods/` guardan las últimas `KEEP_GAME_LOGS` sesiones de
//! cada tipo (OK y CRASH por separado) y `crash/` los últimos
//! `KEEP_CRASH_BUNDLES` expedientes, para que la carpeta no crezca sin fin.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::core::paths::{sanitize, Paths};

/// ¿Cuántos logs de sesiones pasadas se conservan? (OK y CRASH se podan por separado.)
const KEEP_GAME_LOGS: usize = 40;
/// ¿Cuántos expedientes de crash se conservan por instancia?
const KEEP_CRASH_BUNDLES: usize = 20;

/// Espejo de la salida del juego: recibe las mismas líneas que la UI y las escribe
/// a `logs/game/<instancia>/<fecha>.log`. Al cerrar, `finish` lo renombra con el
/// sufijo `__OK` o `__CRASH`.
pub struct GameLogMirror {
    file: Option<std::fs::File>,
    path: PathBuf,
    /// Marca temporal (`2026-09-28_184512`) que comparte con el expediente de crash.
    stamp: String,
    /// Instancia a la que pertenece la sesión.
    slug: String,
}

impl GameLogMirror {
    /// Crea el espejo. Fallo tolerable: si no se puede escribir, la UI sigue
    /// mostrando el log en vivo.
    pub fn new(paths: &Paths, instance_slug: &str) -> Self {
        let slug = sanitize(instance_slug);
        let dir = paths.game_logs(&slug);
        let stamp = stamp(SystemTime::now());
        let path = dir.join(format!("{stamp}_{slug}.log"));
        let file = std::fs::create_dir_all(&dir)
            .ok()
            .and_then(|()| {
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .ok()
            });
        Self {
            file,
            path,
            stamp,
            slug,
        }
    }

    /// Ruta provisional del log en curso.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Marca temporal de la sesión (la comparten el log y el expediente de crash).
    pub fn stamp(&self) -> &str {
        &self.stamp
    }

    /// Instancia de la sesión (ya saneada).
    pub fn slug(&self) -> &str {
        &self.slug
    }

    /// Escribe una línea del juego al espejo. Pública: el llamador decide qué
    /// líneas van (las mismas que pinta la UI).
    pub fn write_line(&mut self, line: &str) {
        if let Some(file) = self.file.as_mut() {
            let _ = writeln!(file, "{line}");
        }
    }

    /// Cierra el fichero, lo renombra a su nombre final (`...__OK.log` o
    /// `...__CRASH.log`) y poda las sesiones antiguas de esta instancia.
    /// Devuelve la ruta final.
    pub fn finish(mut self, ok: bool) -> PathBuf {
        self.file = None;
        let final_path = con_sufijo(&self.path, if ok { "__OK.log" } else { "__CRASH.log" });
        // Si el rename falla (¿otro proceso?), queda el provisional: sirve igual.
        let final_path = std::fs::rename(&self.path, &final_path)
            .map(|()| final_path)
            .unwrap_or_else(|_| self.path.clone());
        prune_session_dir(final_path.parent().unwrap_or(Path::new(".")));
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
    /// Sesión completa: `logs/game/<instancia>/<fecha>__OK|__CRASH.log`.
    pub session_log: PathBuf,
    /// Expediente del crash (`logs/crash/<instancia>/<fecha>/`) si falló.
    pub crash_dir: Option<PathBuf>,
    /// `resumen.txt` dentro del expediente (lo que se lee primero).
    pub summary: Option<PathBuf>,
    /// Log de mods de la sesión (`logs/mods/<instancia>/<fecha>.log`).
    pub mod_log: Option<PathBuf>,
    /// Crash report oficial de Mojang si lo hubo (`crash-reports/` del gameDir).
    pub crash_report: Option<PathBuf>,
    /// Log nativo de la JVM (`hs_err_pid*.log`) si lo hubo: es lo que queda cuando
    /// el juego muere SIN reporte de Mojang (shaders, drivers, memoria…).
    pub jvm_log: Option<PathBuf>,
    /// `logs/latest.log` del gameDir si es de esta partida (lo escribe el propio Minecraft).
    pub game_latest_log: Option<PathBuf>,
}

impl GameExit {
    /// Carpeta donde vive todo lo de esta sesión (para el botón «Abrir carpeta»).
    pub fn folder(&self) -> PathBuf {
        self.crash_dir
            .clone()
            .or_else(|| self.session_log.parent().map(Path::to_path_buf))
            .unwrap_or_default()
    }

    /// Todos los ficheros que merece la pena mirar, de lo más resumido a lo más
    /// crudo. La UI los usa para pestañas y botones.
    pub fn reports(&self) -> Vec<(&'static str, PathBuf)> {
        let mut out = Vec::new();
        if let Some(summary) = &self.summary {
            out.push(("Resumen", summary.clone()));
        }
        if !self.session_log.as_os_str().is_empty() {
            out.push(("Sesión", self.session_log.clone()));
        }
        if let Some(mods) = &self.mod_log {
            out.push(("Mods", mods.clone()));
        }
        if let Some(report) = &self.crash_report {
            out.push(("Crash report", report.clone()));
        }
        if let Some(jvm) = &self.jvm_log {
            out.push(("JVM (hs_err)", jvm.clone()));
        }
        if let Some(latest) = &self.game_latest_log {
            out.push(("latest.log", latest.clone()));
        }
        out
    }
}

/// Clasifica el final de una partida: código de salida, causa probable (del crash
/// report oficial o de heurísticas sobre la salida) y rutas de los logs del
/// gameDir. `tail` son las últimas líneas del juego (las mismas que pinta la UI).
pub fn classify(
    status: std::io::Result<std::process::ExitStatus>,
    game_dir: &Path,
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
        session_log: PathBuf::new(),
        crash_dir: None,
        summary: None,
        mod_log: None,
        crash_report,
        jvm_log,
        game_latest_log: game_latest,
    }
}

/// Lo que `organize` dejó escrito en disco para una sesión.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Artifacts {
    pub mod_log: Option<PathBuf>,
    pub crash_dir: Option<PathBuf>,
    pub summary: Option<PathBuf>,
}

/// Reúne, para una sesión ya terminada, el log de mods y (si falló) el expediente
/// de crash autocontenido. Se llama justo después de `GameLogMirror::finish`, así
/// que el gameDir aún está fresco y los ficheros de Mojang/JVM son de esta partida.
pub fn organize(
    paths: &Paths,
    slug: &str,
    stamp: &str,
    exit: &GameExit,
    session_log: &Path,
) -> Artifacts {
    let slug = sanitize(slug);
    let mod_log = write_mod_log(paths, &slug, stamp, exit, session_log);
    let (crash_dir, summary) = if exit.ok {
        (None, None)
    } else {
        let dir = write_crash_bundle(paths, &slug, stamp, exit, session_log, mod_log.as_deref());
        (Some(dir.clone()), Some(dir.join("resumen.txt")))
    };
    Artifacts {
        mod_log,
        crash_dir,
        summary,
    }
}

// ── Log de mods ──────────────────────────────────────────────────────────────

/// Marcadores de una línea «de mods». Se busca en minúsculas: los cargadores
/// escriben con su propia capitalización y no queremos perder líneas por eso.
const MOD_MARKERS: [&str; 15] = [
    "mixin",
    "modid",
    "mod id",
    "mod file",
    "modlauncher",
    "fabricloader",
    "fabric loader",
    "quilt",
    "neoforge",
    "forge",
    "fml",
    "optifine",
    // «mods» en cualquier forma: «Loading 42 mods:», «mods/sodium.jar»,
    // «duplicate mods»… y no aparece en palabras normales («models» no lo tiene).
    "mods",
    ".jar",
    "duplicate mod",
];

/// ¿Es una línea relacionada con mods? Pública para poder testearla.
pub fn is_mod_line(line: &str) -> bool {
    let lower = line.to_lowercase();
    MOD_MARKERS.iter().any(|marker| lower.contains(marker))
}

/// Escribe `logs/mods/<slug>/<fecha>.log`: la lista de mods de la carpeta más las
/// líneas de la sesión que hablan de mods. Devuelve la ruta, o `None` si no había
/// ni mods ni líneas que los mencionen.
fn write_mod_log(
    paths: &Paths,
    slug: &str,
    stamp: &str,
    exit: &GameExit,
    session_log: &Path,
) -> Option<PathBuf> {
    let mods = list_mods(&paths.instance_dir(slug));
    let session = std::fs::read(session_log).ok()?;
    let text = String::from_utf8_lossy(&session);
    let matched: Vec<&str> = text
        .lines()
        .filter(|line| is_mod_line(line))
        .collect();
    if mods.is_empty() && matched.is_empty() {
        return None;
    }

    let dir = paths.mod_logs(slug);
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join(format!("{stamp}.log"));
    let mut file = std::fs::File::create(&path).ok()?;

    let _ = writeln!(
        file,
        "── Mods de «{slug}» · {stamp} · fin {} ──",
        if exit.ok { "correcto" } else { "con error" }
    );
    let _ = writeln!(file, "Mods en la carpeta ({}):", mods.len());
    for (name, size) in &mods {
        let _ = writeln!(file, "  {name}   ({})", human_size(*size));
    }
    let _ = writeln!(file, "\n── Líneas de la sesión relacionadas con mods ──");
    if matched.is_empty() {
        let _ = writeln!(file, "(el juego no mencionó ningún mod)");
    }
    for line in matched {
        let _ = writeln!(file, "{line}");
    }
    prune_mod_dir(&dir);
    Some(path)
}

/// Mods de `mods/`: nombre y tamaño, ordenados por nombre. Los `.disabled`
/// cuentan igual: saber qué hay apagado también importa al depurar.
fn list_mods(game_dir: &Path) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    if let Ok(read) = std::fs::read_dir(game_dir.join("mods")) {
        for entry in read.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if !(name.ends_with(".jar") || name.ends_with(".disabled")) {
                continue;
            }
            let size = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
            out.push((name, size));
        }
    }
    out.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    out
}

fn human_size(bytes: u64) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else {
        format!("{} KB", bytes / 1024)
    }
}

// ── Expediente de crash ──────────────────────────────────────────────────────

/// Escribe `logs/crash/<slug>/<stamp>/` con TODO lo que hace falta para depurar
/// el fallo y devuelve la carpeta. Cada fichero se copia con un nombre que dice
/// de dónde salió: quien lo lea no tiene que adivinar.
fn write_crash_bundle(
    paths: &Paths,
    slug: &str,
    stamp: &str,
    exit: &GameExit,
    session_log: &Path,
    mod_log: Option<&Path>,
) -> PathBuf {
    let dir = paths.crash_logs(slug).join(stamp);
    if std::fs::create_dir_all(&dir).is_err() {
        return dir;
    }

    // Ficheros copiados, del más específico al más general.
    let copies: [(&str, Option<&Path>); 5] = [
        ("sesion.log", Some(session_log)),
        ("mods.log", mod_log),
        ("minecraft-crash-report.txt", exit.crash_report.as_deref()),
        ("minecraft-latest.log", exit.game_latest_log.as_deref()),
        ("jvm-hs-err.log", exit.jvm_log.as_deref()),
    ];
    let mut written: Vec<String> = Vec::new();
    for (name, source) in copies {
        let Some(source) = source else { continue };
        if source.as_os_str().is_empty() || !source.is_file() {
            continue;
        }
        if std::fs::copy(source, dir.join(name)).is_ok() {
            written.push(name.to_string());
        }
    }

    let summary = dir.join("resumen.txt");
    if let Ok(mut file) = std::fs::File::create(&summary) {
        let _ = writeln!(file, "── McLite · resumen de la sesión que falló ──");
        let _ = writeln!(file, "Instancia : {slug}");
        let _ = writeln!(
            file,
            "Causa     : {}",
            exit.cause.as_deref().unwrap_or("desconocida")
        );
        let _ = writeln!(file, "Código    : {}", exit.code);
        let _ = writeln!(file, "Fecha     : {stamp}");
        let _ = writeln!(file, "Launcher  : McLite {}", crate::LAUNCHER_VERSION);
        let _ = writeln!(file, "\nFicheros en este expediente:");
        for name in &written {
            let _ = writeln!(file, "  {name}");
        }
        let _ = writeln!(file, "\nSesión completa : {}", session_log.display());
        if let Some(report) = &exit.crash_report {
            let _ = writeln!(file, "Crash report    : {}", report.display());
        }
        if let Some(latest) = &exit.game_latest_log {
            let _ = writeln!(file, "latest.log      : {}", latest.display());
        }
        if let Some(jvm) = &exit.jvm_log {
            let _ = writeln!(file, "hs_err de JVM   : {}", jvm.display());
        }
        let _ = writeln!(
            file,
            "\nConsejo: comprime esta carpeta y mándala entera; ya lleva todo lo necesario."
        );
    }

    prune_crash_dir(&paths.crash_logs(slug));
    dir
}

// ── Búsqueda de ficheros del gameDir ─────────────────────────────────────────

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
                .unwrap_or(SystemTime::UNIX_EPOCH)
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

/// Deja en la carpeta de una instancia solo las últimas `KEEP_GAME_LOGS` sesiones
/// de cada tipo (OK y CRASH se podan por separado) y los provisionales con más de
/// una hora sin escribir (una sesión en vivo escribe constantemente; uno muerto
/// ya no sirve). Devuelve cuántos ficheros borró.
fn prune_session_dir(dir: &Path) -> usize {
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

/// Poda `logs/mods/<instancia>/`: se queda con las últimas `KEEP_GAME_LOGS`.
fn prune_mod_dir(dir: &Path) -> usize {
    let mut files: Vec<(SystemTime, PathBuf)> = Vec::new();
    if let Ok(read) = std::fs::read_dir(dir) {
        for entry in read.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            let Ok(modified) = meta.modified() else { continue };
            files.push((modified, entry.path()));
        }
    }
    files.sort_by(|a, b| b.0.cmp(&a.0));
    let mut removed = 0;
    for (_, path) in files.iter().skip(KEEP_GAME_LOGS) {
        if std::fs::remove_file(path).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Poda `logs/crash/<instancia>/`: se queda con los últimos `KEEP_CRASH_BUNDLES`
/// expedientes (carpetas) y borra los ficheros sueltos antiguos (formato previo).
fn prune_crash_dir(dir: &Path) -> usize {
    let mut dirs: Vec<(SystemTime, PathBuf)> = Vec::new();
    let mut removed = 0;
    if let Ok(read) = std::fs::read_dir(dir) {
        for entry in read.flatten() {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else { continue };
            let Ok(modified) = meta.modified() else { continue };
            // Solo los expedientes (carpetas) cuentan para el límite. Los
            // ficheros sueltos del formato antiguo (logs/crash/<fecha>.log) se
            // conservan tal cual: no se tocan datos del usuario por estética.
            if meta.is_dir() {
                dirs.push((modified, path));
            }
        }
    }
    dirs.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, path) in dirs.iter().skip(KEEP_CRASH_BUNDLES) {
        if std::fs::remove_dir_all(path).is_ok() {
            removed += 1;
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

    fn temp_paths(name: &str) -> Paths {
        let root = std::env::temp_dir().join(format!("mclite-crash-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        Paths::with_root(root)
    }

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

    #[test]
    fn reconoce_las_lineas_de_mods() {
        assert!(is_mod_line("[18:45:12] [main/INFO]: Loading 42 mods:"));
        assert!(is_mod_line("[main/WARN]: Mixin config sodium.mixins.json not found"));
        assert!(is_mod_line("[worker/INFO]: Mod file mods/jei-1.21.jar requires fabric"));
        assert!(is_mod_line("loading modid=sodium version=0.6"));
        // Una línea normal no entra.
        assert!(!is_mod_line("[Render thread/INFO]: OpenGL 4.6 ready"));
    }

    #[test]
    fn la_sesion_vive_en_su_carpeta_y_se_cierra_con_sufijo() {
        let paths = temp_paths("sesion");
        let mut mirror = GameLogMirror::new(&paths, "Mi Pack");
        mirror.write_line("hola");
        let stamp = mirror.stamp().to_string();
        let provisional = mirror.path().to_path_buf();
        assert_eq!(provisional.parent(), Some(paths.game_logs("Mi Pack").as_path()));
        assert!(provisional.ends_with(format!("{stamp}_Mi_Pack.log")));

        let final_path = mirror.finish(false);
        assert!(final_path.ends_with(format!("{stamp}_Mi_Pack__CRASH.log")));
        assert!(final_path.is_file());
        assert!(!provisional.exists());

        let _ = std::fs::remove_dir_all(paths.root());
    }

    #[test]
    fn el_crash_deja_expediente_y_log_de_mods() {
        let paths = temp_paths("bundle");
        paths.ensure().unwrap();
        // Una instancia con un mod.
        let game_dir = paths.instance_dir("Mi_Pack");
        std::fs::create_dir_all(game_dir.join("mods")).unwrap();
        std::fs::write(game_dir.join("mods").join("sodium.jar"), vec![0u8; 2048]).unwrap();

        let mut mirror = GameLogMirror::new(&paths, "Mi_Pack");
        let stamp = mirror.stamp().to_string();
        mirror.write_line("[main/INFO]: Loading 1 mods:");
        mirror.write_line("[main/INFO]: Mixin config sodium.mixins.json");
        mirror.write_line("[Render thread/INFO]: OpenGL 4.6 ready");
        let session_log = mirror.finish(false);

        let exit = GameExit {
            ok: false,
            code: 1,
            cause: Some("Rendering overlay".into()),
            session_log: session_log.clone(),
            crash_dir: None,
            summary: None,
            mod_log: None,
            crash_report: None,
            jvm_log: None,
            game_latest_log: None,
        };
        let artifacts = organize(&paths, "Mi_Pack", &stamp, &exit, &session_log);

        // Log de mods: lista el jar y las líneas de mods, no las de OpenGL.
        let mod_log = artifacts.mod_log.clone().expect("debería haber log de mods");
        let mods_text = std::fs::read_to_string(&mod_log).unwrap();
        assert!(mods_text.contains("sodium.jar"), "{mods_text}");
        assert!(mods_text.contains("Mixin config"), "{mods_text}");
        assert!(!mods_text.contains("OpenGL 4.6"), "{mods_text}");

        // Expediente: autocontenido, con copia de la sesión y resumen.
        let crash_dir = artifacts.crash_dir.clone().expect("debería haber expediente");
        assert_eq!(crash_dir, paths.crash_logs("Mi_Pack").join(&stamp));
        assert!(crash_dir.join("sesion.log").is_file());
        assert!(crash_dir.join("mods.log").is_file());
        let resumen = std::fs::read_to_string(crash_dir.join("resumen.txt")).unwrap();
        assert!(resumen.contains("Rendering overlay"), "{resumen}");
        assert!(resumen.contains("Mi_Pack"), "{resumen}");

        // Y todos los ficheros se ofrecen a la UI.
        let listed = GameExit {
            summary: artifacts.summary.clone(),
            crash_dir: artifacts.crash_dir.clone(),
            mod_log: artifacts.mod_log.clone(),
            ..exit.clone()
        }
        .reports();
        assert_eq!(listed.len(), 3, "{listed:?}");

        let _ = std::fs::remove_dir_all(paths.root());
    }

    #[test]
    fn una_sesion_correcta_no_genera_expediente() {
        let paths = temp_paths("ok");
        paths.ensure().unwrap();
        let mirror = GameLogMirror::new(&paths, "sin_mods");
        let stamp = mirror.stamp().to_string();
        let session_log = mirror.finish(true);
        let exit = GameExit {
            ok: true,
            code: 0,
            cause: None,
            session_log: session_log.clone(),
            crash_dir: None,
            summary: None,
            mod_log: None,
            crash_report: None,
            jvm_log: None,
            game_latest_log: None,
        };
        let artifacts = organize(&paths, "sin_mods", &stamp, &exit, &session_log);
        assert!(artifacts.crash_dir.is_none());
        // Sin mods ni líneas de mods, no se escribe un fichero vacío.
        assert!(artifacts.mod_log.is_none());
        let _ = std::fs::remove_dir_all(paths.root());
    }
}
