use std::path::PathBuf;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("error de red: {0}")]
    Http(String),

    #[error("JSON inválido: {0}")]
    Json(#[from] serde_json::Error),

    #[error("E/S en {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("E/S: {0}")]
    PlainIo(#[from] std::io::Error),

    #[error("no pude leer el zip {path}: {reason}")]
    Zip { path: PathBuf, reason: String },

    #[error("falta el campo {0} en los datos recibidos")]
    Missing(String),

    #[error("no soportado: {0}")]
    Unsupported(String),

    #[error("hash SHA-1 no coincide en {path}: esperado {expected}, obtenido {actual}")]
    HashMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },

    #[error("la descarga falló tras {attempts} intentos: {url}")]
    Download { url: String, attempts: u32 },

    #[error("error al lanzar el juego: {0}")]
    Launch(String),

    #[error("nick inválido: {0}")]
    InvalidUsername(String),
}

impl Error {
    /// Envuelve un error de E/S añadiendo la ruta, que casi siempre es lo único
    /// que hace falta para diagnosticar.
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Error::Io {
            path: path.into(),
            source,
        }
    }
}
