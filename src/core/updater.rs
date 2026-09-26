//! Auto-update del launcher (A del plan): GitHub releases + verificación SHA-256
//! + swap del exe en ejecución.
//!
//! En Windows un exe en ejecución no se puede borrar/sobrescribir, pero SÍ
//! renombrar. Estrategia (la de Chromium/VS Code): bajar el nuevo a
//! `update/mclite.exe.new`, verificarlo, renombrar el actual a
//! `mclite.exe.old`, copiar el nuevo a `mclite.exe` y — si el rename falló
//! (antivirus) — dejar un helper `.bat` que completa el cambio tras la salida.
//! El arranque limpia `.old` residuales.

use serde::Deserialize;

use crate::core::error::{Error, Result};
use crate::core::http::{Download, HttpClient};
use crate::core::paths::Paths;

const RELEASES_API: &str = "https://api.github.com/repos/SoyJuanPiece/mclite/releases/latest";

/// Datos del último release publicadado en GitHub.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseInfo {
    /// Versión sin la "v" inicial ("0.4.0").
    pub version: String,
    pub exe_url: String,
    pub sha256_url: String,
    pub html_url: String,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    html_url: String,
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

/// Consulta el último release. `None` si no hay red, GitHub cae, o el release
/// no trae el par `mclite.exe` + `mclite.exe.sha256` (releases antiguos).
pub fn latest_release(http: &HttpClient) -> Option<ReleaseInfo> {
    let body = http.get_string_ua(RELEASES_API, crate::core::modrinth::USER_AGENT).ok()?;
    let release: GhRelease = serde_json::from_str(&body).ok()?;
    let version = release.tag_name.strip_prefix('v')?.to_string();
    let find = |suffix: &str| {
        release
            .assets
            .iter()
            .find(|asset| asset.name == format!("mclite.exe{suffix}"))
            .map(|asset| asset.browser_download_url.clone())
    };
    Some(ReleaseInfo {
        version,
        exe_url: find("")?,
        sha256_url: find(".sha256")?,
        html_url: release.html_url,
    })
}

/// ¿Es `candidate` una versión más nueva que `current`? ("0.4.0" vs "0.3.0").
pub fn version_is_newer(candidate: &str, current: &str) -> bool {
    let parse = |text: &str| -> Vec<u64> {
        text.split('.')
            .map(|part| {
                part.chars()
                    .filter(|c| c.is_ascii_digit())
                    .collect::<String>()
            })
            .filter_map(|part| part.parse().ok())
            .collect()
    };
    let (candidate, current) = (parse(candidate), parse(current));
    for index in 0..3 {
        let a = candidate.get(index).copied().unwrap_or(0);
        let b = current.get(index).copied().unwrap_or(0);
        if a != b {
            return a > b;
        }
    }
    false
}

/// Lee "abc123…  mclite.exe" y devuelve solo el hash en minúsculas.
pub fn parse_sha256_file(content: &str) -> Option<String> {
    content
        .split_whitespace()
        .next()
        .filter(|hash| hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()))
        .map(|hash| hash.to_ascii_lowercase())
}

/// Hash SHA-256 de un fichero, en hex minúsculas.
pub fn sha256_of(path: &std::path::Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).map_err(|err| Error::io(path, err))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// Carpeta de trabajo del updater.
fn update_dir(paths: &Paths) -> std::path::PathBuf {
    paths.root().join("update")
}

/// Descarga el exe nuevo a `update/mclite.exe.new` y verifica su hash.
/// Devuelve la ruta del fichero verificado. Nada toca el exe actual.
pub fn download_update(
    http: &HttpClient,
    paths: &Paths,
    release: &ReleaseInfo,
) -> Result<std::path::PathBuf> {
    let dir = update_dir(paths);
    std::fs::create_dir_all(&dir).map_err(|err| Error::io(&dir, err))?;
    let new_exe = dir.join("mclite.exe.new");
    let hash_file = dir.join("release.sha256");

    // Limpiar restos de un intento anterior: el downloader se salta los
    // destinos que ya existen, y un .sha256 viejo haría fallar la verificación
    // del exe nuevo con un hash de otra versión (falso "hash mismatch").
    let _ = std::fs::remove_file(&new_exe);
    let _ = std::fs::remove_file(&hash_file);

    http.download(&Download::new(&release.sha256_url, &hash_file))?;
    let expected = parse_sha256_file(
        &std::fs::read_to_string(&hash_file).map_err(|err| Error::io(&hash_file, err))?,
    )
    .ok_or_else(|| crate::Error::Unsupported("el .sha256 del release no es válido".into()))?;

    http.download(&Download::new(&release.exe_url, &new_exe))?;
    let actual = sha256_of(&new_exe)?;
    if actual != expected {
        let _ = std::fs::remove_file(&new_exe);
        return Err(crate::Error::Unsupported(format!(
            "el exe descargado no coincide con su hash ({actual})"
        )));
    }
    Ok(new_exe)
}

/// Añade una línea al `mclite-update.log` junto al exe (best-effort).
fn update_log(exe_dir: &std::path::Path, line: &str) {
    use std::io::Write;
    let path = exe_dir.join("mclite-update.log");
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(file, "{line}");
    }
}

/// Intercambia el exe en ejecución por el nuevo. SIN bucles ni
/// `tasklist | find` en batch (ese patrón se colgaba: el .bat quedaba vivo
/// sin copiar nada). Todo el trabajo pesado ocurre aquí, en Rust:
///
/// 1. Renombra el exe vivo → `mclite.exe.old` (permitido en ejecución en
///    Windows; queda como rollback). El proceso sigue corriendo desde la
///    imagen renombrada.
/// 2. Copia el exe nuevo al hueco y VERIFICA el tamaño (3 intentos).
/// 3. Escribe un helper mínimo de 3 líneas —espera 2 s, `start`, se borra—
///    SOLO para reabrir tras el cierre; sin PID, sin delayed expansion.
///
/// Si el rename fallara (antivirus agresivo), el helper de respaldo copia
/// tras 3 s de gracia (proceso ya muerto) y luego abre.
pub fn apply_swap(new_exe: &std::path::Path) -> Result<()> {
    use std::process::Command;

    let current = std::env::current_exe().map_err(|err| {
        crate::Error::Unsupported(format!("no pude localizar el exe en ejecución: {err}"))
    })?;
    let Some(exe_dir) = current.parent() else {
        return Err(crate::Error::Unsupported(
            "el exe está en la raíz del sistema de archivos".into(),
        ));
    };
    let _ = std::fs::write(
        exe_dir.join("mclite-update.log"),
        "── actualización McLite ──\r\n",
    );
    update_log(exe_dir, &format!("exe actual: {}", current.display()));

    // 1) Rollback: renombrar el exe vivo. Si funciona, el hueco queda libre.
    let old = exe_dir.join("mclite.exe.old");
    let _ = std::fs::remove_file(&old);
    let renamed = std::fs::rename(&current, &old);
    match &renamed {
        Ok(()) => update_log(exe_dir, "paso 1: exe viejo renombrado a .old (rollback listo)"),
        Err(err) => update_log(
            exe_dir,
            &format!("paso 1: rename falló ({err}); el helper copiará tras el cierre"),
        ),
    }

    let helper = exe_dir.join("mclite-update.bat");
    let script = if renamed.is_ok() {
        // 2) Copiar y verificar AQUÍ, en Rust: tamaño exacto, 3 intentos.
        let expected = std::fs::metadata(new_exe).map(|meta| meta.len()).unwrap_or(0);
        let mut installed = false;
        for attempt in 1..=3 {
            match std::fs::copy(new_exe, &current) {
                Ok(len) if len == expected => {
                    installed = true;
                    update_log(
                        exe_dir,
                        &format!(
                            "paso 2: exe nuevo instalado y verificado (intento {attempt}, {len} bytes)"
                        ),
                    );
                    break;
                }
                other => {
                    update_log(
                        exe_dir,
                        &format!("paso 2: intento {attempt} falló ({other:?})"),
                    );
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
            }
        }
        if !installed {
            update_log(exe_dir, "ERROR: no pude colocar el exe nuevo; se conserva el .new");
            return Err(crate::Error::Unsupported(
                "no pude colocar el exe nuevo (ver mclite-update.log)".into(),
            ));
        }
        let _ = std::fs::remove_file(new_exe);
        // 3) Solo reabrir: 2 s de gracia, start, autodestrucción. Nada más.
        format!(
            "@echo off\r\nping -n 3 127.0.0.1 >nul\r\nstart \"\" \"{current}\"\r\ndel /F /Q \"%~f0\"\r\n",
            current = current.display(),
        )
    } else {
        // Respaldo: el helper copia tras 3 s (proceso ya muerto) y abre.
        format!(
            "@echo off\r\nping -n 4 127.0.0.1 >nul\r\ncopy /Y \"{new_exe}\" \"{current}\" >nul\r\nstart \"\" \"{current}\"\r\ndel /F /Q \"%~f0\"\r\n",
            new_exe = new_exe.display(),
            current = current.display(),
        )
    };

    std::fs::write(&helper, script).map_err(|err| Error::io(&helper, err))?;
    update_log(exe_dir, "paso 3: helper armado para reabrir tras el cierre");
    let mut command = Command::new("cmd");
    command.args(["/C", &helper.display().to_string()]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0000_0008); // DETACHED_PROCESS
    }
    command
        .spawn()
        .map_err(|err| Error::Launch(format!("no pude armar el helper de actualización: {err}")))?;
    Ok(())
}

/// Limpieza al arrancar: si quedó un `.old` de una actualización previa y ya
/// no está bloqueado, fuera. Nunca es fatal.
/// ¿Toca borrar la copia vieja? Sin fecha legible → sí (limpieza garantizada);
/// con fecha, solo cuando supera la ventana de rollback configurada
/// (`keep_secs` = 0 → borrar en el siguiente arranque).
fn should_delete_old(age_secs: Option<u64>, keep_secs: u64) -> bool {
    match age_secs {
        None => true,
        Some(age) => age >= keep_secs,
    }
}

/// Limpia restos de actualizaciones en la carpeta del exe. Solo se conserva
/// UNA copia (`mclite.exe.old`) y se borra cuando su antigüedad supera la
/// ventana de rollback configurada (`keep_secs`; 0 = al siguiente arranque).
pub fn cleanup_old(keep_secs: u64) {
    if let Ok(current) = std::env::current_exe() {
        if let Some(exe_dir) = current.parent() {
            let old = exe_dir.join("mclite.exe.old");
            if old.exists() {
                let age = std::fs::metadata(&old)
                    .and_then(|meta| meta.modified())
                    .ok()
                    .and_then(|mtime| mtime.elapsed().ok())
                    .map(|elapsed| elapsed.as_secs());
                if should_delete_old(age, keep_secs) {
                    let _ = std::fs::remove_file(&old);
                }
            }
            // El helper del .bat sí siempre se puede borrar: ya no sirve.
            let _ = std::fs::remove_file(exe_dir.join("mclite-update.bat"));
            // Y el log del update anterior también.
            let _ = std::fs::remove_file(exe_dir.join("mclite-update.log"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test EN VIVO contra el release real de GitHub: valida el fix del falso
    /// "hash mismatch". Envenena update/release.sha256 con un hash viejo y
    /// comprueba que download_update lo refresca (antes el descargador se
    /// saltaba el fichero existente y comparaba contra el hash de otra versión).
    /// Ignorado por defecto (toca red); correr con: cargo test -- --ignored
    #[test]
    #[ignore = "toca red y baja ~10 MB: verificación manual del updater"]
    fn download_update_refresca_el_hash_viejo() {
        use crate::core::paths::Paths;
        let http = HttpClient::new();
        let paths = Paths::discover().expect("paths descubribles");
        let release = latest_release(&http).expect("debe leer el último release");

        // 1) Primera pasada: deja release.sha256 y mclite.exe.new reales.
        let first = download_update(&http, &paths, &release).expect("primera descarga");
        assert!(first.is_file());

        // 2) Envenenar el hash guardado (simula el resto de una versión anterior).
        let hash_file = update_dir(&paths).join("release.sha256");
        std::fs::write(&hash_file, format!("{}  mclite.exe\n", "0".repeat(64)))
            .expect("escribir hash envenenado");

        // 3) Segunda pasada: SIN el fix fallaría aquí con "no coincide con su hash".
        let second = download_update(&http, &paths, &release).expect("segunda descarga (hash refrescado)");
        assert_eq!(first, second);
        let expected = parse_sha256_file(
            &std::fs::read_to_string(&hash_file).expect("leer el hash refrescado"),
        )
        .expect("hash refrescado válido");
        assert_eq!(sha256_of(&second).expect("hash del exe"), expected);

        // Limpieza del directorio de prueba.
        let _ = std::fs::remove_file(&second);
        let _ = std::fs::remove_file(&hash_file);
    }

    #[test]
    fn la_ventana_de_rollback_decide_el_borrado() {
        // Sin fecha legible: siempre se limpia.
        assert!(should_delete_old(None, 3600));
        // Ventana 0: borrar en cuanto se detecte.
        assert!(should_delete_old(Some(0), 0));
        assert!(should_delete_old(Some(3600), 3600));
        // Dentro de la ventana: se conserva para rollback.
        assert!(!should_delete_old(Some(100), 3600));
    }

    #[test]
    fn compara_versiones() {
        assert!(version_is_newer("0.4.0", "0.3.0"));
        assert!(version_is_newer("0.3.1", "0.3.0"));
        assert!(version_is_newer("1.0.0", "0.9.9"));
        assert!(!version_is_newer("0.3.0", "0.3.0"));
        assert!(!version_is_newer("0.2.9", "0.3.0"));
        // Con "v" delante o letras raras, se filtran los dígitos por posición.
        assert!(version_is_newer("0.4.0-beta", "0.3.0"));
    }

    #[test]
    fn parsea_el_fichero_sha256() {
        let content = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  mclite.exe\n";
        let hash = parse_sha256_file(content).unwrap();
        assert_eq!(hash.len(), 64);
        assert_eq!(hash, hash.to_lowercase());
        assert!(parse_sha256_file("corto").is_none());
        assert!(parse_sha256_file("").is_none());
    }

    #[test]
    fn calcula_sha256_conocido() {
        let dir = std::env::temp_dir().join("mclite-updater-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("vacio.txt");
        std::fs::write(&file, b"").unwrap();
        // SHA-256 del fichero vacío, constante conocida.
        assert_eq!(
            sha256_of(&file).unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
