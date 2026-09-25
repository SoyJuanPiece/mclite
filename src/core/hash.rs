use std::io::Read;
use std::path::Path;

use sha1::{Digest, Sha1};

use crate::core::error::{Error, Result};

/// Hash SHA-1 en hex minúsculas, el formato que usan los manifiestos de Mojang.
pub fn sha1_hex(bytes: impl AsRef<[u8]>) -> String {
    let mut hasher = Sha1::new();
    hasher.update(bytes.as_ref());
    hex(hasher.finalize())
}

/// SHA-1 de un fichero, leyéndolo por bloques para no cargarlo entero en memoria
/// (el client jar pasa de 25 MB).
pub fn sha1_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path).map_err(|e| Error::io(path, e))?;
    let mut hasher = Sha1::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buf).map_err(|e| Error::io(path, e))?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(hex(hasher.finalize()))
}

pub fn hex(bytes: impl AsRef<[u8]>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes = bytes.as_ref();
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_de_vacio() {
        assert_eq!(sha1_hex(b""), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
    }

    #[test]
    fn sha1_de_abc() {
        assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
    }
}
