//! Formateo de salidas compartidas por los comandos de la CLI.

use std::path::{Path, PathBuf};

/// Tamaño legible en MB (el core devuelve bytes).
pub fn human_mb(bytes: u64) -> String {
    format!("{:.0} MB", bytes as f64 / 1_048_576.0)
}

/// ¿Estos argumentos solo piden abrir la GUI? La ventana se abre sin
/// argumentos o con solo banderas de render (`--renderer <modo>`, `--gl`);
/// cualquier comando (`versions`, `launch`, …) va a la CLI.
pub fn wants_gui(args: &[String]) -> bool {
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--gl" => {}
            // `--renderer` consume el valor siguiente; con `=` va junto.
            "--renderer" => {
                rest.next();
            }
            other if other.starts_with("--renderer=") => {}
            _ => return false,
        }
    }
    true
}

/// Junta los `.log`/`.txt` de `dir` y sus subcarpetas (expedientes de crash),
/// sin bajar más de `MAX_DEPTH` niveles.
pub fn collect_logs(dir: &Path) -> Vec<PathBuf> {
    const MAX_DEPTH: usize = 3;
    let mut out = Vec::new();
    collect_logs_inner(dir, &mut out, 0, MAX_DEPTH);
    out.sort_by_key(|path| {
        std::fs::metadata(path)
            .and_then(|meta| meta.modified())
            .ok()
    });
    out.reverse(); // Los más recientes primero.
    out
}

fn collect_logs_inner(dir: &Path, out: &mut Vec<PathBuf>, depth: usize, max_depth: usize) {
    if depth > max_depth {
        return;
    }
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_logs_inner(&path, out, depth + 1, max_depth);
        } else if is_log_file(&path) {
            out.push(path);
        }
    }
}

fn is_log_file(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext == "log" || ext == "txt")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mb_rounding() {
        assert_eq!(human_mb(1_048_576), "1 MB");
        assert_eq!(human_mb(0), "0 MB");
    }

    #[test]
    fn gui_only_flags() {
        assert!(wants_gui(&[]));
        assert!(wants_gui(&["--gl".into()]));
        assert!(wants_gui(&["--renderer".into(), "gl".into()]));
        assert!(wants_gui(&["--renderer=gl".into()]));
        assert!(!wants_gui(&["versions".into()]));
        assert!(!wants_gui(&["--gl".into(), "launch".into()]));
    }
}
