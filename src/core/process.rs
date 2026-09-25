//! Subprocesos.
//!
//! En Windows hay que pedir explícitamente que no se abra una ventana de consola:
//! si no, cada `java -version` de la detección parpadea un cuadro negro encima del launcher.

use std::path::Path;
use std::process::Command;

/// `CREATE_NO_WINDOW`: el proceso hijo no crea consola.
#[cfg(windows)]
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// `CREATE_NEW_PROCESS_GROUP`: aísla al hijo de Ctrl+C del launcher.
#[cfg(windows)]
pub const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

/// Comando sin ventana de consola (solo relevante en Windows).
#[allow(unused_mut)]
pub fn hidden_command(exe: impl AsRef<Path>) -> Command {
    let mut command = Command::new(exe.as_ref());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// Comando para el juego: sin consola propia pero en su propio grupo de procesos,
/// para que cerrar el launcher no arrastre al juego ni al revés.
#[allow(unused_mut)]
pub fn game_command(exe: impl AsRef<Path>) -> Command {
    let mut command = Command::new(exe.as_ref());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
    }
    command
}
