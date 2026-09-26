//! Apertura de carpetas en el explorador y URLs en el navegador.

use std::path::Path;

use crate::core::error::{Error, Result};
use crate::core::process::hidden_command;

/// Abre una URL en el navegador por defecto (login device-code de Microsoft,
/// enlaces del launcher). En Windows usa `start` sin consola; si no, `xdg-open`.
pub fn open_url(url: &str) -> Result<()> {
    if cfg!(windows) {
        // `cmd /C start "" <url>`: las comillas vacías evitan que un título
        // con `&` rompa el comando.
        let status = hidden_command("cmd")
            .args(["/C", "start", "", url])
            .status()
            .map_err(|e| Error::Launch(format!("no pude abrir el navegador: {e}")))?;
        if status.success() {
            Ok(())
        } else {
            Err(Error::Launch(format!("start salió con {status}")))
        }
    } else if cfg!(target_os = "macos") {
        let status = hidden_command("open")
            .arg(url)
            .status()
            .map_err(|e| Error::Launch(format!("no pude abrir el navegador: {e}")))?;
        if status.success() {
            Ok(())
        } else {
            Err(Error::Launch(format!("open salió con {status}")))
        }
    } else {
        let status = hidden_command("xdg-open")
            .arg(url)
            .status()
            .map_err(|e| Error::Launch(format!("no pude abrir el navegador: {e}")))?;
        if status.success() {
            Ok(())
        } else {
            Err(Error::Launch(format!("xdg-open salió con {status}")))
        }
    }
}

/// Abre `path` en el explorador de ficheros del sistema.
/// En Windows usa `explorer`; en macOS `open`; en Linux `xdg-open`.
pub fn open_in_explorer(path: &Path) -> Result<()> {
    if !path.exists() {
        std::fs::create_dir_all(path).map_err(|e| Error::io(path, e))?;
    }

    let (program, arg): (&'static str, &Path) = if cfg!(windows) {
        ("explorer", path)
    } else if cfg!(target_os = "macos") {
        ("open", path)
    } else {
        ("xdg-open", path)
    };

    let status = hidden_command(program)
        .arg(arg)
        .status()
        .map_err(|e| Error::Launch(format!("no pude abrir {program}: {e}")))?;

    if status.success() {
        Ok(())
    } else {
        Err(Error::Launch(format!(
            "{program} salió con {status} al abrir {}",
            path.display()
        )))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "abre ventanas: solo manual"]
    fn abre_el_temp_dir() {
        super::open_in_explorer(&std::env::temp_dir()).unwrap();
    }
}
