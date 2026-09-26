//! Orquestación: de un id de versión a algo lanzable.
//!
//! El orden importa poco, pero lo que sí importa es que todo lo descargable pase por
//! `Download` (verificación SHA-1 + `.part`) y que las natives se extraigan después de
//! bajarlas. `dry_run` recorre exactamente el mismo camino sin bajar nada pesado: sirve
//! para ver el plan de instalación y la línea de comandos antes de gastar 1 GB.

use std::path::{Path, PathBuf};

use crate::core::assets::{self, AssetIndex};
use crate::core::auth::OfflineAccount;
use crate::core::endpoints;
use crate::core::error::{Error, Result};
use crate::core::http::{self, Download, HttpClient};
use crate::core::java;
use crate::core::launch::{self, LaunchContext, LaunchPlan};
use crate::core::manifest::{VersionFilter, VersionManifest};
use crate::core::natives;
use crate::core::paths::Paths;
use crate::core::progress::Progress;
use crate::core::rules::{Environment, Features};
use crate::core::runtime;
use crate::core::version_json::VersionJson;
use crate::loaders::{self, LoaderKind};

/// Fichero de assets por defecto de las versiones que no declaran `assets`.
const LEGACY_ASSETS: &str = "legacy";

#[derive(Debug, Clone)]
pub struct InstallOptions {
    pub threads: usize,
    /// No descarga client jar, librerías, assets ni log config: solo resuelve el
    /// manifiesto y calcula rutas y línea de comandos.
    pub dry_run: bool,
    /// Omite el índice y los objetos de assets (~1 GB en 1.21). Útil para instalar
    /// la parte "código" sin el contenido, y para tests de humo de las descargas.
    pub skip_assets: bool,
    /// Si no hay un Java del sistema que cubra lo que pide la versión, descargar
    /// el runtime oficial de Mojang (fase 3). Activado por defecto: es lo que
    /// hace el launcher oficial y evita el error clásico de class file version.
    pub mojang_runtime: bool,
}

impl Default for InstallOptions {
    fn default() -> Self {
        Self {
            threads: http::DEFAULT_THREADS,
            dry_run: false,
            skip_assets: false,
            mojang_runtime: true,
        }
    }
}

/// Resultado de dejar una versión lista en disco.
#[derive(Debug, Clone)]
pub struct Installed {
    pub version: VersionJson,
    pub version_id: String,
    pub client_jar: PathBuf,
    pub classpath: Vec<PathBuf>,
    pub natives_dir: PathBuf,
    pub assets_index_name: String,
    pub log_config: Option<PathBuf>,
    /// Tamaño total del índice de assets.
    pub assets_total_bytes: u64,
    pub assets_pending: usize,
    pub libraries_pending: usize,
}

/// Manifiesto de versiones. Si no hay red, tira de la copia local.
pub fn fetch_manifest(http: &HttpClient, paths: &Paths) -> Result<VersionManifest> {
    match http.get_string(endpoints::VERSION_MANIFEST_V2) {
        Ok(raw) => {
            let manifest = VersionManifest::parse(&raw)?;
            if paths.ensure().is_ok() {
                let _ = std::fs::write(manifest_cache(paths), &raw);
            }
            Ok(manifest)
        }
        Err(err) => cached_manifest(paths).ok_or(err),
    }
}

/// Última copia del manifiesto que se descargó, sin tocar la red.
pub fn cached_manifest(paths: &Paths) -> Option<VersionManifest> {
    let raw = std::fs::read_to_string(manifest_cache(paths)).ok()?;
    VersionManifest::parse(&raw).ok()
}

/// Manifiesto con red de respaldo: la copia local o, si no hay, la descarga
/// (y la deja cacheada para la próxima). Para consultas ligeras que no
/// justifican tocar la red cada vez pero que sí la necesitan la primera vez.
pub fn cached_manifest_of(http: &HttpClient, paths: &Paths) -> Option<VersionManifest> {
    cached_manifest(paths).or_else(|| {
        fetch_manifest(http, paths).ok()
    })
}

fn manifest_cache(paths: &Paths) -> PathBuf {
    paths.root().join("version_manifest_v2.json")
}

/// Manifiesto de una versión **vanilla** (sin cargador). Usa la copia local si la hay.
pub fn resolve_vanilla(
    http: &HttpClient,
    paths: &Paths,
    progress: &Progress,
    mc: &str,
    version_id: &str,
) -> Result<VersionJson> {
    let dest = paths.version_json(version_id);
    if let Ok(raw) = std::fs::read_to_string(&dest) {
        if let Ok(version) = VersionJson::from_str(&raw) {
            return Ok(version);
        }
    }

    progress.phase(format!("Manifiesto de Minecraft {mc}"));
    let manifest = fetch_manifest(http, paths)?;
    let entry = manifest.require(mc)?;
    let raw = http.get_string(&entry.url)?;
    let version = VersionJson::from_str(&raw)?;
    write_json(&dest, &raw)?;
    Ok(version)
}

fn write_json(dest: &Path, raw: &str) -> Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    std::fs::write(dest, raw).map_err(|e| Error::io(dest, e))
}

/// Deja lista en disco una versión ya resuelta (vanilla o cargador + vanilla).
pub fn download(
    http: &HttpClient,
    paths: &Paths,
    version: &VersionJson,
    version_id: &str,
    opts: &InstallOptions,
    progress: &Progress,
) -> Result<Installed> {
    paths.ensure()?;

    let env = Environment::current();
    let features = Features::default();

    // ── 1. Client jar ─────────────────────────────────────────────────────────
    progress.phase("Client jar");
    let client = version
        .downloads
        .as_ref()
        .and_then(|downloads| downloads.client.clone())
        .ok_or_else(|| {
            Error::Missing(format!(
                "downloads.client de «{version_id}»: sin él no hay juego que lanzar"
            ))
        })?;
    let client_jar = paths.version_jar(version_id);
    if !opts.dry_run {
        http.download(&file_job(
            &client.url,
            client_jar.clone(),
            client.sha1.clone(),
            client.size,
        ))?;
        progress.advance(1);
    }

    // ── 2. Librerías y natives ────────────────────────────────────────────────
    progress.phase("Librerías");
    let mut classpath: Vec<PathBuf> = Vec::new();
    let mut native_jars: Vec<(PathBuf, Vec<String>)> = Vec::new();
    let mut jobs: Vec<Download> = Vec::new();

    for library in &version.libraries {
        let Some(resolved) = library.resolve(&env, &features) else {
            continue;
        };
        let dest = paths.library_file(&resolved.maven_path);

        // Las natives van al classpath **y** se extraen: es lo que hace el launcher
        // oficial, y así también funcionan los perfiles que las meten ahí.
        classpath.push(dest.clone());
        if resolved.is_native {
            native_jars.push((dest.clone(), resolved.extract_exclude.clone()));
        }
        if let Some(url) = &resolved.url {
            jobs.push(file_job(url, dest, resolved.sha1.clone(), resolved.size));
        } else if !dest.is_file() {
            return Err(Error::Missing(format!(
                "la librería «{}» no tiene URL y no está en disco",
                resolved.name
            )));
        }
    }

    let libraries_pending = jobs.iter().filter(|job| !job.is_complete()).count();
    if !opts.dry_run {
        http::download_all(http, jobs, opts.threads, progress)?;
    }

    // ── 3. Natives ────────────────────────────────────────────────────────────
    let natives_dir = paths.natives_dir(version_id);
    if !native_jars.is_empty() && !opts.dry_run {
        progress.phase("Natives");
        // La carpeta es por versión, así que se puede limpiar sin miedo: si no, un
        // cambio de arquitectura deja DLLs viejas mezcladas.
        if natives_dir.exists() {
            std::fs::remove_dir_all(&natives_dir).map_err(|e| Error::io(&natives_dir, e))?;
        }
        std::fs::create_dir_all(&natives_dir).map_err(|e| Error::io(&natives_dir, e))?;
        for (jar, exclude) in &native_jars {
            if jar.is_file() {
                natives::extract(jar, &natives_dir, exclude)?;
            }
        }
    }

    // ── 4. Assets ─────────────────────────────────────────────────────────────
    let assets_index_name = version
        .assets
        .clone()
        .or_else(|| version.asset_index.as_ref().map(|info| info.id.clone()))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| LEGACY_ASSETS.to_string());

    let mut assets_total_bytes = 0u64;
    let mut assets_pending = 0usize;

    if opts.skip_assets {
        progress.message(format!("assets omitidos (--no-assets): índice {assets_index_name}"));
    } else if let Some(info) = &version.asset_index {
        progress.phase("Assets");
        let index_file = paths.asset_index_file(&assets_index_name);
        if !index_file.is_file() && !opts.dry_run {
            http.download(&file_job(
                &info.url,
                index_file.clone(),
                info.sha1.clone(),
                info.size,
            ))?;
        }
        if index_file.is_file() {
            match std::fs::read_to_string(&index_file)
                .map_err(|e| Error::io(&index_file, e))
                .and_then(|raw| AssetIndex::parse(&raw))
            {
                Ok(index) => {
                    assets_total_bytes = index.total_size();
                    let jobs = assets::plan(&index, paths);
                    assets_pending = assets::missing_count(&jobs);
                    if !opts.dry_run {
                        http::download_all(http, jobs, opts.threads, progress)?;
                    }
                }
                Err(err) => {
                    // Un índice corrupto no debe impedir lanzar: se avisa y se baja de nuevo.
                    progress.message(format!("índice de assets ilegible ({err}); se ignora"));
                    let _ = std::fs::remove_file(&index_file);
                }
            }
        }
        // Los índices antiguos (< 1.7) necesitan los assets copiados al gameDir; eso
        // queda para cuando el soporte de versiones antiguas salga de "experimental".
        if !opts.dry_run && version.assets.is_none() {
            progress.message("índice de assets antiguo: puede requerir copia a gameDir");
        }
    }

    // ── 5. Config de log4j ────────────────────────────────────────────────────
    let mut log_config = None;
    if let Some(logging) = &version.logging {
        // Forge genera `"logging": {}` sin `client`: no hay config que bajar.
        if let Some(logging_client) = &logging.client {
            progress.phase("Config de log");
            let file_name = logging_client
                .file
                .id
                .clone()
                .filter(|id| !id.is_empty())
                .unwrap_or_else(|| format!("client-{version_id}"));
            let file_name = if file_name.ends_with(".xml") {
                file_name
            } else {
                format!("{file_name}.xml")
            };
            let dest = paths.log_configs().join(crate::core::paths::sanitize(&file_name));
            if !opts.dry_run {
                http.download(&file_job(
                    &logging_client.file.url,
                    dest.clone(),
                    logging_client.file.sha1.clone(),
                    logging_client.file.size,
                ))?;
            }
            log_config = Some(dest);
        } else {
            progress.message("config de log4j ausente en el manifiesto: se omite");
        }
    }

    // ── 6. Manifiesto final en disco ──────────────────────────────────────────
    if !opts.dry_run {
        let dest = paths.version_json(version_id);
        let raw = serde_json::to_string_pretty(version)?;
        write_json(&dest, &raw)?;
    }

    Ok(Installed {
        version: version.clone(),
        version_id: version_id.to_string(),
        client_jar,
        classpath,
        natives_dir,
        assets_index_name,
        log_config,
        assets_total_bytes,
        assets_pending,
        libraries_pending,
    })
}

fn file_job(url: &str, dest: PathBuf, sha1: Option<String>, size: Option<u64>) -> Download {
    let mut job = Download::new(url.to_string(), dest);
    if let Some(sha1) = sha1 {
        job = job.with_sha1(sha1);
    }
    if let Some(size) = size {
        job = job.with_size(size);
    }
    job
}

/// Todo lo necesario para lanzar una instancia.
#[derive(Debug, Clone)]
pub struct PlayRequest {
    pub kind: LoaderKind,
    pub mc_version: String,
    pub loader_version: Option<String>,
    pub game_dir: PathBuf,
    pub username: String,
    /// Cuenta Microsoft ya resuelta (token fresco). `None` = offline.
    pub account: Option<AccountCredentials>,
    pub memory_mb: u32,
    pub width: u32,
    pub height: u32,
    /// Java forzado. `None` = autodetección.
    pub java: Option<PathBuf>,
    pub extra_jvm_args: Vec<String>,
    pub filter: VersionFilter,
}

/// El Java que se usará para lanzar. El que pida el manifiesto
/// (`javaVersion.majorVersion`); 8 si no lo declara, que es lo que usan las
/// versiones antiguas.
///
/// Orden de preferencia: 1) el fijado a mano, 2) un Java del sistema que cubra
/// lo pedido (exacto, o el más bajo que lo supere si el pedido es moderno),
/// 3) el runtime de Mojang, que se baja a `runtime/<componente>/` (fase 3).
/// Un Java MÁS VIEJO nunca se acepta: eso revienta en tiempo de ejecución
/// (UnsupportedClassVersionError); y un moderno solo se acepta para pedidos
/// modernos, porque las versiones antiguas se rompen con JVMs nuevos.
pub fn resolve_java(
    http: &HttpClient,
    paths: &Paths,
    required_major: u32,
    forced: Option<&Path>,
    opts: &InstallOptions,
    progress: &Progress,
) -> Result<(PathBuf, Option<runtime::JavaRuntime>)> {
    let Some(path) = forced else {
        let found = java::detect_all();
        if let Some(installation) = java::select_for(required_major, &found) {
            if installation.major != required_major {
                progress.message(format!(
                    "usando Java {} del sistema (la versión pide Java {required_major})",
                    installation.major
                ));
            }
            return Ok((installation.path, None));
        }
        if !opts.mojang_runtime {
            return Err(Error::Unsupported(format!(
                "esta versión pide Java {required_major} y no hay ningún Java instalado \
                 que lo cubra. Instala uno o activa «usar el Java de Mojang» en ajustes."
            )));
        }
        let component = runtime::component_for(required_major);
        let rt = runtime::ensure(http, paths, component, opts.threads, progress)?;
        progress.message(format!("usando {}", rt.label()));
        let java = rt.java.clone();
        return Ok((java, Some(rt)));
    };
    if !path.exists() {
        return Err(Error::Unsupported(format!(
            "el Java configurado no existe: {}",
            path.display()
        )));
    }
    Ok((path.to_path_buf(), None))
}

pub struct Prepared {
    pub version_id: String,
    pub installed: Installed,
    pub java: PathBuf,
    /// Si el Java salió de Mojang en esta preparación, aquí está (para mostrarlo
    /// en ajustes). `None` si se usó uno del sistema o fijado a mano.
    pub mojang_runtime: Option<runtime::JavaRuntime>,
    pub plan: LaunchPlan,
    pub account: OfflineAccount,
}

/// Credenciales de una cuenta Microsoft resuelta (token ya vigente).
#[derive(Debug, Clone)]
pub struct AccountCredentials {
    pub username: String,
    pub uuid: String,
    pub access_token: String,
}

/// Resuelve cargador + vanilla, descarga lo que falte y arma la línea de comandos.
/// Es lo único que tienen que llamar la CLI y la GUI para lanzar.
pub fn prepare(
    http: &HttpClient,
    paths: &Paths,
    req: &PlayRequest,
    opts: &InstallOptions,
    progress: &Progress,
) -> Result<Prepared> {
    // Cuenta Microsoft si el caller trajo credenciales; si no, offline.
    let account = match &req.account {
        Some(creds) => OfflineAccount {
            username: creds.username.clone(),
            uuid: creds.uuid.clone(),
            access_token: creds.access_token.clone(),
        },
        None => OfflineAccount::new(&req.username)?,
    };

    progress.phase("Resolviendo versión");
    let (version_id, version) = loaders::resolve(
        http,
        paths,
        progress,
        req.filter,
        req.kind,
        &req.mc_version,
        req.loader_version.as_deref(),
        opts,
    )?;
    crate::core::logging::info(&format!(
        "versión resuelta: {version_id} ({} {})",
        req.kind.key(),
        req.loader_version.as_deref().unwrap_or("-")
    ));

    let installed = download(http, paths, &version, &version_id, opts, progress)?;

    // Java: el que pida el manifiesto (`javaVersion.majorVersion`); 8 si no lo
    // declara, que es lo que usan las versiones antiguas. Ver `resolve_java`.
    let required = version
        .java_version
        .as_ref()
        .map(|java| java.major_version)
        .unwrap_or(8);
    let (java_path, mojang_runtime) =
        resolve_java(http, paths, required, req.java.as_deref(), opts, progress)?;

    // El juego en Windows va con javaw (misma JVM, sin consola).
    let game_java = java::javaw_path(&java_path);

    progress.phase("Armando la línea de comandos");
    let plan = launch::build(&LaunchContext {
        version: &installed.version,
        version_id: &installed.version_id,
        account: &account,
        paths,
        game_dir: &req.game_dir,
        java: &game_java,
        classpath: &installed.classpath,
        client_jar: installed.client_jar.clone(),
        natives_dir: installed.natives_dir.clone(),
        assets_index_name: installed.assets_index_name.clone(),
        logging_config: installed.log_config.clone(),
        memory_mb: req.memory_mb,
        width: req.width,
        height: req.height,
        extra_jvm_args: req.extra_jvm_args.clone(),
    })?;

    Ok(Prepared {
        version_id,
        installed,
        java: game_java,
        mojang_runtime,
        plan,
        account,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_job_lleva_hash_y_tamano() {
        let job = file_job("http://x/a.jar", PathBuf::from("/tmp/a.jar"), Some("aa".into()), Some(7));
        assert_eq!(job.sha1.as_deref(), Some("aa"));
        assert_eq!(job.size, Some(7));
    }

    #[test]
    fn nombre_del_fichero_de_log() {
        // El `id` real de 1.21.4 es `client-1.21.2` (sin extensión) y el fichero
        // descargado es `.xml`.
        let with_xml = |id: &str| {
            if id.ends_with(".xml") {
                id.to_string()
            } else {
                format!("{id}.xml")
            }
        };
        assert_eq!(with_xml("client-1.21.2"), "client-1.21.2.xml");
        assert_eq!(with_xml("client.xml"), "client.xml");
    }
}
