//! URLs externas. Todas viven aquí para poder cambiarlas en un solo sitio
//! (y para tener a mano qué está verificado y qué no).

/// Manifiesto oficial de versiones de Java Edition.
pub const VERSION_MANIFEST_V2: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

/// Objetos de assets (`{root}/{2 primeros del hash}/{hash}`).
pub const ASSET_OBJECTS_ROOT: &str = "https://resources.download.minecraft.net";

/// Maven de las librerías de Mojang.
pub const LIBRARIES_MAVEN: &str = "https://libraries.minecraft.net";

/// Manifest de los runtimes de Java que publica Mojang (lo usa el launcher oficial).
pub const JAVA_RUNTIME_ALL: &str =
    "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";

// ── Cargadores ────────────────────────────────────────────────────────────────
// Verificados el 2026-09-24. Ver §2 de PLAN.md.

/// Fabric: lista de loaders por versión de MC.
pub const FABRIC_META: &str = "https://meta.fabricmc.net";
pub const QUILT_META: &str = "https://meta.quiltmc.org";

/// Forge. Se usa `maven-metadata.xml`: `promotions_slim.json` responde 403 a
/// clientes automatizados desde ~2024, así que no se usa.
pub const FORGE_MAVEN: &str = "https://maven.minecraftforge.net";
pub const NEOFORGE_MAVEN: &str = "https://maven.neoforged.net/releases";

/// BMCLAPI: mirror de terceros que sí permite automatizar OptiFine
/// (`GET /optifine/{mc}` y `GET /optifine/{mc}/{type}/{patch}`).
pub const BMCLAPI: &str = "https://bmclapi2.bangbang93.com";

/// Página oficial de OptiFine (fallback manual).
pub const OPTIFINE_DOWNLOADS_PAGE: &str = "https://optifine.net/downloads";

/// Clave de plataforma del runtime de Java de Mojang.
pub fn java_runtime_platform() -> &'static str {
    if cfg!(target_os = "windows") {
        if cfg!(target_arch = "x86") {
            "windows-x86"
        } else if cfg!(target_arch = "aarch64") {
            "windows-arm64"
        } else {
            "windows-x64"
        }
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "mac-os-arm64"
        } else {
            "mac-os"
        }
    } else if cfg!(target_arch = "aarch64") {
        "linux-arm64"
    } else if cfg!(target_arch = "x86") {
        "linux-i386"
    } else {
        "linux"
    }
}
