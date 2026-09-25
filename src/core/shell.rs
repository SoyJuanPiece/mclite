//! Apertura de carpetas en el explorador (botones «Abrir carpeta» de la UI).

use std::path::Path;

use crate::core::error::{Error, Result};
use crate::core::process::hidden_command;

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
