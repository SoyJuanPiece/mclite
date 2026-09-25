//! Progreso y log. El core no sabe nada de egui: emite eventos a un `ProgressSink`
//! y la GUI decide cómo pintarlos.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProgressEvent {
    /// Cambio de fase ("Resolviendo manifiesto", "Descargando librerías", ...).
    Phase(String),
    /// Total de unidades de la fase actual (ficheros o bytes).
    Total(u64),
    /// Unidades completadas.
    Advance(u64),
    /// Línea de log.
    Message(String),
}

pub trait ProgressSink: Send + Sync {
    fn emit(&self, event: ProgressEvent);
}

/// No reporta nada (tests, CLI silenciosa).
pub struct NullSink;

impl ProgressSink for NullSink {
    fn emit(&self, _event: ProgressEvent) {}
}

struct FnSink<F>(F);

impl<F: Fn(ProgressEvent) + Send + Sync> ProgressSink for FnSink<F> {
    fn emit(&self, event: ProgressEvent) {
        (self.0)(event)
    }
}

/// Handle clonable que se pasa por todo el core.
#[derive(Clone)]
pub struct Progress {
    sink: Arc<dyn ProgressSink>,
    /// Copia local del total, para que las descargas paralelas puedan repartir
    /// el avance sin bloquearse entre ellas.
    total: Arc<AtomicU64>,
}

impl Default for Progress {
    fn default() -> Self {
        Self::none()
    }
}

impl Progress {
    pub fn none() -> Self {
        Self {
            sink: Arc::new(NullSink),
            total: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn new(sink: impl ProgressSink + 'static) -> Self {
        Self {
            sink: Arc::new(sink),
            total: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn from_fn(f: impl Fn(ProgressEvent) + Send + Sync + 'static) -> Self {
        Self::new(FnSink(f))
    }

    pub fn phase(&self, phase: impl Into<String>) {
        self.total.store(0, Ordering::Relaxed);
        self.sink.emit(ProgressEvent::Phase(phase.into()));
    }

    pub fn set_total(&self, total: u64) {
        self.total.store(total, Ordering::Relaxed);
        self.sink.emit(ProgressEvent::Total(total));
    }

    pub fn advance(&self, n: u64) {
        self.sink.emit(ProgressEvent::Advance(n));
    }

    pub fn message(&self, msg: impl Into<String>) {
        self.sink.emit(ProgressEvent::Message(msg.into()));
    }
}
