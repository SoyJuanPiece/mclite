//! HTTP con reintentos y descargas paralelas verificadas por SHA-1.
//!
//! Es `ureq` bloqueante a propósito: para un launcher, un pool de hilos es más
//! simple (y más chico) que arrastrar un runtime async.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::de::DeserializeOwned;

use crate::core::error::{Error, Result};
use crate::core::progress::Progress;

/// Conexiones simultáneas por defecto. Más de 16 suele empeorar las cosas
/// (los servidores de Mojang empiezan a cortar) y satura discos lentos.
pub const DEFAULT_THREADS: usize = 8;
const MAX_THREADS: usize = 16;
const ATTEMPTS: u32 = 3;

pub struct HttpClient {
    agent: ureq::Agent,
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpClient {
    pub fn new() -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_recv_body(Some(Duration::from_secs(300)))
            .user_agent(concat!("mclite/", env!("CARGO_PKG_VERSION")))
            .build();
        Self {
            agent: config.into(),
        }
    }

    fn call(&self, url: &str) -> Result<ureq::http::Response<ureq::Body>> {
        // Un 5xx o un corte de red se reintenta con backoff; un 4xx no
        // (insistir con un 404 solo hace perder tiempo).
        let mut last: Option<Error> = None;
        for attempt in 1..=ATTEMPTS {
            match self.agent.get(url).call() {
                Ok(response) => return Ok(response),
                Err(ureq::Error::StatusCode(code)) if code < 500 && code != 408 && code != 429 => {
                    return Err(Error::Http(format!("HTTP {code} en {url}")));
                }
                Err(err) => {
                    last = Some(Error::Http(format!("{err} en {url}")));
                }
            }
            std::thread::sleep(Duration::from_millis(200 * 2u64.pow(attempt - 1)));
        }
        Err(last.unwrap_or(Error::Download {
            url: url.to_string(),
            attempts: ATTEMPTS,
        }))
    }

    pub fn get_string(&self, url: &str) -> Result<String> {
        let mut response = self.call(url)?;
        let mut body = String::new();
        response
            .body_mut()
            .as_reader()
            .read_to_string(&mut body)
            .map_err(|e| Error::Http(format!("leyendo {url}: {e}")))?;
        Ok(body)
    }

    pub fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        let raw = self.get_string(url)?;
        serde_json::from_str(&raw).map_err(Error::from)
    }

    /// JSON con User-Agent propio. La API de Modrinth pide identificarse con
    /// `repo/version` (el genérico que traen otros endpoints del launcher no
    /// basta para sus reglas de uso).
    pub fn get_json_ua<T: DeserializeOwned>(&self, url: &str, user_agent: &str) -> Result<T> {
        let raw = self.get_string_ua(url, user_agent)?;
        serde_json::from_str(&raw).map_err(Error::from)
    }

    /// GET a texto con User-Agent concreto (una petición, sin reintentos: el
    /// caller decide si reintenta).
    pub fn get_string_ua(&self, url: &str, user_agent: &str) -> Result<String> {
        let mut response = self
            .agent
            .get(url)
            .header("User-Agent", user_agent)
            .call()
            .map_err(|err| match err {
                ureq::Error::StatusCode(code) => Error::Http(format!("HTTP {code} en {url}")),
                other => Error::Http(format!("{other} en {url}")),
            })?;
        let mut body = String::new();
        response
            .body_mut()
            .as_reader()
            .read_to_string(&mut body)
            .map_err(|e| Error::Http(format!("leyendo {url}: {e}")))?;
        Ok(body)
    }

    /// Baja `job.url` a `job.dest` verificando el SHA-1.
    ///
    /// Escribe a un `.part` y renombra al final: así una descarga cortada nunca
    /// deja un fichero a medias que parezca bueno.
    pub fn download(&self, job: &Download) -> Result<u64> {
        if let Some(size) = job.size {
            if job.is_complete() {
                return Ok(size);
            }
        } else if job.dest.is_file() {
            return Ok(0);
        }

        let parent = job.dest.parent().ok_or_else(|| {
            Error::Unsupported(format!("destino sin carpeta padre: {}", job.dest.display()))
        })?;
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;

        let tmp = tmp_path(&job.dest);
        let mut last_hash_error = None;

        for attempt in 1..=ATTEMPTS {
            let result = (|| -> Result<u64> {
                let mut response = self.call(&job.url)?;
                let mut reader = response.body_mut().as_reader();
                let mut file = std::fs::File::create(&tmp).map_err(|e| Error::io(&tmp, e))?;
                let written = std::io::copy(&mut reader, &mut file).map_err(|e| {
                    Error::Http(format!("descargando {}: {e}", job.url))
                })?;
                file.flush().map_err(|e| Error::io(&tmp, e))?;
                drop(file);

                if let Some(expected) = &job.sha1 {
                    let actual = crate::core::hash::sha1_file(&tmp)?;
                    if !actual.eq_ignore_ascii_case(expected) {
                        return Err(Error::HashMismatch {
                            path: job.dest.clone(),
                            expected: expected.clone(),
                            actual,
                        });
                    }
                }
                std::fs::rename(&tmp, &job.dest).map_err(|e| Error::io(&job.dest, e))?;
                Ok(written)
            })();

            match result {
                Ok(written) => return Ok(written),
                Err(err) => {
                    let _ = std::fs::remove_file(&tmp);
                    last_hash_error = Some(err);
                    if attempt < ATTEMPTS {
                        std::thread::sleep(Duration::from_millis(200 * 2u64.pow(attempt - 1)));
                    }
                }
            }
        }

        Err(last_hash_error.unwrap_or(Error::Download {
            url: job.url.clone(),
            attempts: ATTEMPTS,
        }))
    }
}

fn tmp_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    dest.with_file_name(name)
}

/// Una descarga pendiente.
#[derive(Clone, Debug)]
pub struct Download {
    pub url: String,
    pub dest: PathBuf,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

impl Download {
    pub fn new(url: impl Into<String>, dest: impl Into<PathBuf>) -> Self {
        Self {
            url: url.into(),
            dest: dest.into(),
            sha1: None,
            size: None,
        }
    }

    pub fn with_sha1(mut self, sha1: impl Into<String>) -> Self {
        self.sha1 = Some(sha1.into());
        self
    }

    pub fn with_size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }

    /// ¿Ya está en disco y con el hash correcto?
    pub fn is_complete(&self) -> bool {
        if !self.dest.is_file() {
            return false;
        }
        match &self.sha1 {
            Some(expected) => crate::core::hash::sha1_file(&self.dest)
                .map(|actual| actual.eq_ignore_ascii_case(expected))
                .unwrap_or(false),
            None => true,
        }
    }
}

/// Descarga una lista de ficheros en paralelo, saltando lo que ya está bien.
///
/// Devuelve cuántos ficheros había realmente pendientes (útil para decidir si
/// hay algo que instalar).
pub fn download_all(
    http: &HttpClient,
    jobs: Vec<Download>,
    threads: usize,
    progress: &Progress,
) -> Result<usize> {
    let pending: Vec<Download> = jobs.into_iter().filter(|job| !job.is_complete()).collect();

    progress.set_total(pending.len() as u64);
    if pending.is_empty() {
        return Ok(0);
    }

    let threads = threads.clamp(1, MAX_THREADS).min(pending.len().max(1));
    let next = AtomicUsize::new(0);
    let failures: Mutex<Vec<Error>> = Mutex::new(Vec::new());
    let abort = AtomicBool::new(false);

    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| loop {
                if abort.load(Ordering::Relaxed) {
                    break;
                }
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(job) = pending.get(index) else {
                    break;
                };
                match http.download(job) {
                    Ok(_) => {
                        progress.advance(1);
                    }
                    Err(err) => {
                        failures.lock().unwrap().push(err);
                        abort.store(true, Ordering::Relaxed);
                        break;
                    }
                }
            });
        }
    });

    match failures.into_inner().unwrap().into_iter().next() {
        Some(err) => Err(err),
        None => Ok(pending.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::hash::sha1_hex;

    #[test]
    fn detecta_fichero_ya_completo() {
        let dir = std::env::temp_dir().join("mclite-http-test");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("hola.txt");
        std::fs::write(&file, b"abc").unwrap();

        // Con el hash correcto: ya está.
        let ok = Download::new("http://x", &file).with_sha1(sha1_hex(b"abc"));
        assert!(ok.is_complete());

        // Con otro hash: hay que rebajar.
        let bad = Download::new("http://x", &file).with_sha1(sha1_hex(b"xyz"));
        assert!(!bad.is_complete());

        // Sin hash esperado: basta con que exista.
        assert!(Download::new("http://x", &file).is_complete());

        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn el_part_no_se_confunde_con_el_destino() {
        assert_eq!(
            tmp_path(Path::new("/a/b/client.jar")),
            PathBuf::from("/a/b/client.jar.part")
        );
    }
}
