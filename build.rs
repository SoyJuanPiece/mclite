//! EmbeBe recursos Win32 (icono, VERSIONINFO y manifest) cuando el *target* es Windows.
//!
//! Ojo: los build scripts se compilan para el **host**, no para el target. Por eso la
//! detección se hace con `CARGO_CFG_TARGET_OS` y no con `cfg!(windows)`.

use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=assets/mclite.manifest");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=build.rs");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        return;
    }

    let mut res = winresource::WindowsResource::new();
    res.set("FileDescription", "McLite — launcher lite de Minecraft");
    res.set("ProductName", "McLite");
    res.set("ProductVersion", env!("CARGO_PKG_VERSION"));
    res.set("FileVersion", env!("CARGO_PKG_VERSION"));
    res.set("LegalCopyright", "MIT");
    res.set("OriginalFilename", "mclite.exe");

    if Path::new("assets/mclite.manifest").exists() {
        res.set_manifest_file("assets/mclite.manifest");
    }
    // El icono es opcional: si no está, se compila igual sin icono.
    if Path::new("assets/icon.ico").exists() {
        res.set_icon("assets/icon.ico");
    }

    if let Err(err) = res.compile() {
        // No rompemos el build por no poder embeber recursos (falta windres/llvm-rc en el host).
        println!(
            "cargo:warning=no se pudieron embeber los recursos Win32 ({err}). \
             Falta una herramienta de recursos (llvm-rc o windres). El binario se genera sin icono."
        );
    }
}
