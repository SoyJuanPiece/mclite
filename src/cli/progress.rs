//! Progreso para consola: fases, avisos y un contador cada 25 ficheros.

use std::sync::atomic::{AtomicUsize, Ordering};

use mclite::core::progress::{Progress, ProgressEvent, ProgressSink};

struct ConsoleSink {
    done: AtomicUsize,
}

impl ProgressSink for ConsoleSink {
    fn emit(&self, event: ProgressEvent) {
        match event {
            ProgressEvent::Phase(phase) => println!("\n▸ {phase}"),
            ProgressEvent::Total(total) if total > 0 => {
                println!("  {total} ficheros por revisar");
            }
            ProgressEvent::Advance(step) => {
                let done = self.done.fetch_add(step as usize, Ordering::Relaxed) + step as usize;
                if done % 25 == 0 {
                    println!("  … {done}");
                }
            }
            ProgressEvent::Message(message) => println!("  · {message}"),
        }
    }
}

/// `Progress` listo para imprimir en terminal.
pub(crate) fn cli_progress() -> Progress {
    Progress::new(ConsoleSink {
        done: AtomicUsize::new(0),
    })
}
