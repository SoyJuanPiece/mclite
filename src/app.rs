//! Estado de la ventana, mensajes de los hilos de fondo y bucle de la GUI.
//!
//! Toda la lógica de verdad vive en `core` y `loaders`. Aquí solo se orquesta:
//! descargas, red y detección de Java van a hilos bloqueantes, y sus resultados
//! vuelven por un canal de mensajes que la ventana drena cada frame.

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Mutex;
use std::time::Duration;

use crate::core::crash;
use crate::core::logging;
use crate::core::modrinth;

use crate::core::config::LauncherConfig;
use crate::core::http::{Download, HttpClient};
use crate::core::install::{self, InstallOptions, PlayRequest};
use crate::core::instance::{
    now, Instance, InstanceStore, DEFAULT_HEIGHT, DEFAULT_WIDTH,
};
use crate::core::java::{self, JavaInstallation};
use crate::core::manifest::{VersionFilter, VersionManifest};
use crate::core::paths::Paths;
use crate::core::progress::{Progress, ProgressEvent, ProgressSink};
use crate::loaders::{self, LoaderCtx, LoaderKind, LoaderVersion};
use crate::LAUNCHER_VERSION;

use crate::ui::{edit_instance, home, instances, modpacks, new_instance, settings, theme};

/// Pantalla visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Home,
    New,
    Edit,
    Modpacks,
    Settings,
}

/// Lo que los hilos de fondo le cuentan a la ventana.
enum Message {
    Progress(ProgressEvent),
    Manifest(VersionManifest),
    ManifestFailed(String),
    LoaderVersions {
        mc: String,
        kind: LoaderKind,
        result: Result<Vec<LoaderVersion>, String>,
    },
    Javas(Vec<JavaInstallation>),
    /// Versiones de MC que soporta el cargador del formulario (None = sin datos).
    SupportedMcs(Option<Vec<String>>),
    /// Resultados de búsqueda de modpacks en Modrinth.
    PackSearch(Vec<modrinth::PackHit>),
    PackSearchFailed(String),
    /// Versiones de un modpack concreto.
    PackVersions { slug: String, versions: Vec<modrinth::PackVersion> },
    PackVersionsFailed(String),
    /// Detalle completo de un pack (icono, autor, body…).
    PackDetail(modrinth::PackDetail),
    PackDetailFailed(String),
    JobDone(String),
    /// Un modpack terminó de instalarse: recargar el índice de instancias
    /// (se creó en el hilo de fondo) y seleccionar la nueva.
    PackInstalled(String),
    Failed(String),
    /// El juego terminó (bien o mal): la UI muestra la causa y el log guardado.
    GameExit(crash::GameExit),
    Played {
        slug: String,
    },
    Log(String),
}

/// Puente entre el `Progress` del core y la GUI.
///
/// Dos trabajos, los dos clave para el rendimiento:
///
/// 1. **Despertar la GUI por evento.** Cada envío pide un repintado (`request_repaint`),
///    así la app reposa al 0 % de CPU cuando no pasa nada y se redibuja solo cuando
///    llega algo — en vez de repintar 8 veces por segundo "por si acaso".
/// 2. **Coalescer el progreso.** Con miles de descargas pequeñas los `Advance` llegan
///    más rápido de lo que vale un frame; se acumulan y se envían juntos como máximo
///    cada ~33 ms (≈30 fps de barra). Fase y mensajes pasan al instante.
struct GuiSink {
    tx: Mutex<MsgTx>,
    /// Acumulador de Advance pendientes + instante del último envío.
    pending: Mutex<(u64, std::time::Instant)>,
}

/// Ventana de agregación del progreso: un frame a 30 fps.
const PROGRESS_INTERVAL: std::time::Duration = std::time::Duration::from_millis(33);

/// Canal de mensajes que despierta la GUI en cada envío. Todos los hilos lo
/// usan: así ningún mensaje (manifiesto, fin de trabajo, línea del juego…)
/// se queda dormido esperando un repintado que no va a llegar.
#[derive(Clone)]
struct MsgTx(Sender<Message>, egui::Context);

impl MsgTx {
    fn send(&self, message: Message) {
        let _ = self.0.send(message);
        self.1.request_repaint();
    }
}

impl GuiSink {
    fn new(msg_tx: MsgTx) -> Self {
        Self {
            tx: Mutex::new(msg_tx),
            pending: Mutex::new((0, std::time::Instant::now() - PROGRESS_INTERVAL)),
        }
    }

    /// Envía lo acumulado ya (fin de trabajo, fase nueva, etc.).
    fn flush_pending(&self) {
        if let Ok(mut pending) = self.pending.lock() {
            let (accumulated, _) = std::mem::replace(
                &mut *pending,
                (0, std::time::Instant::now()),
            );
            if accumulated > 0 {
                if let Ok(tx) = self.tx.lock() {
                    tx.send(Message::Progress(ProgressEvent::Advance(accumulated)));
                    tx.1.request_repaint();
                }
            }
        }
    }
}

impl ProgressSink for GuiSink {
    fn emit(&self, event: ProgressEvent) {
        match event {
            ProgressEvent::Advance(step) => {
                let due = {
                    let Ok(mut pending) = self.pending.lock() else {
                        return;
                    };
                    pending.0 += step;
                    let due = pending.1.elapsed() >= PROGRESS_INTERVAL;
                    if due {
                        pending.1 = std::time::Instant::now();
                    }
                    due
                };
                if due {
                    self.flush_pending();
                }
            }
            // Fases, mensajes y demás: urgentes, sin coalescer.
            other => {
                if let Ok(tx) = self.tx.lock() {
                    tx.send(Message::Progress(other));
                }
            }
        }
    }
}

/// Trabajo en curso (instalación, reparación o juego en marcha).
pub struct Job {
    pub label: String,
    pub phase: String,
    pub total: u64,
    pub done: u64,
    /// Momento de arranque, para velocidad y ETA en la barra de progreso.
    pub started: std::time::Instant,
}

impl Job {
    /// Velencia media en unidades/s y tiempo restante estimado.
    /// `None` si aún no hay datos suficientes para estimar nada.
    pub fn speed_and_eta(&self) -> Option<(f32, String)> {
        let elapsed = self.started.elapsed().as_secs_f32().max(0.5);
        if self.total == 0 || self.done == 0 || self.done >= self.total {
            return None;
        }
        let speed = self.done as f32 / elapsed;
        if speed < 0.01 {
            return None;
        }
        let seconds_left = ((self.total - self.done) as f32 / speed).ceil() as u64;
        let eta = if seconds_left >= 90 {
            format!("{} min", seconds_left / 60)
        } else {
            format!("{} s", seconds_left)
        };
        Some((speed, eta))
    }
}

/// Formulario de «Nueva instancia».
pub struct Form {
    pub name: String,
    /// El usuario tocó el nombre: ya no lo sobreescribimos con la sugerencia.
    pub name_edited: bool,
    pub mc: String,
    pub search: String,
    pub loader: LoaderKind,
    /// Id del cargador elegido; vacío = «última estable».
    pub loader_version: String,
    pub loader_versions: Vec<LoaderVersion>,
    pub loader_loading: bool,
    /// Para qué (mc, cargador) se está pidiendo la lista, y así ignorar respuestas viejas.
    pub loader_query: Option<(String, LoaderKind)>,
    /// Versiones de MC que soporta el cargador elegido (`None` = sin datos: la
    /// lista no se filtra). Lo llena `begin_supported_fetch` en segundo plano.
    pub supported_mcs: Option<Vec<String>>,
    pub ram_mb: u32,
    pub width: u32,
    pub height: u32,
    /// Instalar Sodium (solo Fabric): lo baja de Modrinth a mods/ tras crear.
    pub with_sodium: bool,
}

impl Form {
    fn new(ram_mb: u32) -> Self {
        Self {
            name: String::new(),
            name_edited: false,
            mc: String::new(),
            search: String::new(),
            loader: LoaderKind::Vanilla,
            loader_version: String::new(),
            loader_versions: Vec::new(),
            loader_loading: false,
            loader_query: None,
            supported_mcs: None,
            ram_mb,
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            with_sodium: false,
        }
    }
}

/// Estado global de la ventana.
pub struct McLiteApp {
    pub paths: Paths,
    pub config: LauncherConfig,
    pub store: InstanceStore,
    pub screen: Screen,
    pub selected: Option<String>,
    pub form: Form,
    pub manifest: Option<VersionManifest>,
    pub manifest_loading: bool,
    pub javas: Vec<JavaInstallation>,
    pub javas_loading: bool,
    pub job: Option<Job>,
    pub log: Vec<String>,
    pub status: String,
    pub error: Option<String>,
    /// Borrado en dos pasos: el primer clic rellena esto.
    pub confirm_delete: Option<String>,
    /// Resultado de la última partida (causa del crash, log guardado…).
    pub last_game_exit: Option<crash::GameExit>,
    // ── Modpacks (Modrinth) ────────────────────────────────────────────
    pub packs: Vec<modrinth::PackHit>,
    pub pack_search: String,
    pub packs_loading: bool,
    pub pack_versions_slug: Option<String>,
    pub pack_versions: Vec<modrinth::PackVersion>,
    /// Instancia que se está editando (Screen::Edit).
    pub editing_slug: Option<String>,
    /// Detalle del pack abierto (pestaña Modpacks).
    pub pack_detail: Option<modrinth::PackDetail>,
    /// Icono del pack que se está instalando (para la instancia que nacerá).
    pending_pack_icon: Option<String>,
    /// Filtro del buscador de la lista lateral.
    pub sidebar_search: String,
    /// Notificaciones flotantes (éxito/error) con auto-cierre.
    pub toasts: Vec<Toast>,
    /// Transición entre pantallas (0.0 = entra, 1.0 = asentada).
    pub screen_fade: f32,
    /// Pantalla desde la que se viene, para animar la entrada.
    screen_from: Option<Screen>,
    /// Contexto de egui, para repintados dirigidos desde la propia app.
    ui_ctx: Option<egui::Context>,
    /// Canal hacia la GUI; cada envío despierta el repintado.
    tx: MsgTx,
    rx: Receiver<Message>,
}

/// Notificación flotante con auto-cierre.
#[derive(Clone)]
pub struct Toast {
    pub text: String,
    pub kind: ToastKind,
    pub born: std::time::Instant,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ToastKind {
    Ok,
    Error,
}

impl McLiteApp {
    /// Guarda el contexto de egui (no hace falta ya que `MsgTx` lo lleva,
    /// pero conserva la puerta para futuros usos de repintado dirigido).
    pub fn set_ui_ctx(&mut self, ctx: egui::Context) {
        self.ui_ctx = Some(ctx);
    }

    /// Muestra una notificación flotante (dura ~4 s).
    pub fn notify(&mut self, text: impl Into<String>, kind: ToastKind) {
        self.toasts.push(Toast {
            text: text.into(),
            kind,
            born: std::time::Instant::now(),
        });
    }

    /// Dibuja las notificaciones flotantes, arriba a la derecha.
    pub fn show_toasts(&mut self, ctx: &egui::Context) {
        self.toasts.retain(|toast| toast.born.elapsed().as_secs_f32() < 4.0);
        let Some(latest) = self.toasts.last().cloned() else {
            return;
        };
        let (color, icon) = match latest.kind {
            ToastKind::Ok => (theme::accent(), "✔"),
            ToastKind::Error => (theme::DANGER, "⚠"),
        };
        egui::Area::new(egui::Id::new("toasts"))
            .anchor(egui::Align2::RIGHT_TOP, [-16.0, 16.0])
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                // Fade de entrada/salida durante los primeros/últimos 0,4 s.
                let age = latest.born.elapsed().as_secs_f32();
                let alpha = (age / 0.4).min(1.0) * ((4.0 - age) / 0.4).min(1.0);
                ui.visuals_mut().override_text_color = Some(egui::Color32::WHITE);
                egui::Frame::new()
                    .fill(theme::CARD_ELEVATED.gamma_multiply(alpha.clamp(0.05, 1.0)))
                    .stroke(egui::Stroke::new(1.0_f32, color.gamma_multiply(alpha.clamp(0.05, 1.0))))
                    .corner_radius(egui::CornerRadius::same(10))
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.set_min_width(260.0);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(icon).color(color).size(15.0));
                            ui.label(egui::RichText::new(&latest.text).color(theme::TEXT));
                        });
                    });
            });
    }
}

impl McLiteApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // El acento se fija antes de aplicar el tema para que todo nazca del color elegido.
        let paths_probe = Paths::discover()
            .unwrap_or_else(|_| Paths::with_root(std::env::temp_dir().join("mclite")));
        let config_probe = LauncherConfig::load(&paths_probe);
        theme::set_accent(theme::Accent::from_key(
            config_probe.accent.as_deref().unwrap_or("green"),
        ));

        theme::apply(&cc.egui_ctx);
        // Iconos de Modrinth en la pestaña Modpacks: carga por URL en segundo plano.
        egui_extras::install_image_loaders(&cc.egui_ctx);

        let paths = paths_probe;
        logging::init(&paths);
        logging::info("arranque del launcher");
        logging::info(&format!("carpeta de datos: {}", paths.root().display()));
        // ¿El exe se ejecutó desde otra carpeta y la config "no está"? Migrar.
        if let Some(from) = crate::core::config::migrate_previous(&paths) {
            logging::info(&format!("config importada de {from}"));
        }
        let config = LauncherConfig::load(&paths);
        let store = InstanceStore::load(&paths);
        // Reabrir la última instancia seleccionada (si sigue existiendo).
        let selected = config
            .last_instance
            .clone()
            .filter(|slug| store.find(slug).is_some())
            .or_else(|| {
                store
                    .instances
                    .first()
                    .map(|instance| instance.slug.clone())
            });
        let (raw_tx, rx) = channel();
        // Todos los hilos envían por aquí: cada send despierta la GUI.
        let tx = MsgTx(raw_tx, cc.egui_ctx.clone());

        let app = Self {
            paths,
            form: Form::new(config.clamped_ram()),
            config,
            store,
            screen: Screen::Home,
            selected,
            manifest: None,
            manifest_loading: true,
            javas: Vec::new(),
            javas_loading: false,
            job: None,
            log: Vec::new(),
            status: "Listo".to_string(),
            error: None,
            confirm_delete: None,
            last_game_exit: None,
            packs: Vec::new(),
            pack_search: String::new(),
            packs_loading: false,
            pack_versions_slug: None,
            pack_versions: Vec::new(),
            editing_slug: None,
            pack_detail: None,
            pending_pack_icon: None,
            sidebar_search: String::new(),
            toasts: Vec::new(),
            screen_fade: 1.0,
            screen_from: None,
            ui_ctx: None,
            tx,
            rx,
        };
        // El manifiesto baja en un hilo: su MsgTx ya despierta la GUI al llegar.
        app.load_manifest();
        app
    }

    // ── Mensajes ─────────────────────────────────────────────────────────────

    fn drain(&mut self) {
        while let Ok(message) = self.rx.try_recv() {
            self.handle(message);
        }
    }

    fn handle(&mut self, message: Message) {
        match message {
            Message::Progress(event) => {
                let mut pending_log: Option<String> = None;
                if let Some(job) = self.job.as_mut() {
                    match event {
                        ProgressEvent::Phase(phase) => {
                            job.phase = phase;
                            job.done = 0;
                            job.total = 0;
                        }
                        ProgressEvent::Total(total) => job.total = total,
                        ProgressEvent::Advance(step) => {
                            job.done = job.done.saturating_add(step);
                        }
                        ProgressEvent::Message(line) => pending_log = Some(line),
                    }
                } else if let ProgressEvent::Message(line) = event {
                    pending_log = Some(line);
                }
                if let Some(line) = pending_log {
                    self.push_log(line);
                }
            }
            Message::Manifest(manifest) => {
                self.manifest_loading = false;
                if self.form.mc.is_empty() {
                    self.form.mc = manifest.latest.release.clone();
                    if !self.form.name_edited {
                        self.form.name =
                            Instance::suggested_name(self.form.loader, &self.form.mc);
                    }
                }
                self.error = None;
                self.status = "Manifiesto de versiones al día".to_string();
                self.manifest = Some(manifest);
            }
            Message::ManifestFailed(message) => {
                self.manifest_loading = false;
                self.error = Some(format!("No pude cargar el manifiesto: {message}"));
            }
            Message::LoaderVersions { mc, kind, result } => {
                let is_current = matches!(
                    &self.form.loader_query,
                    Some((current_mc, current_kind))
                        if *current_mc == mc && *current_kind == kind
                );
                if !is_current {
                    return;
                }
                self.form.loader_loading = false;
                match result {
                    Ok(versions) => self.form.loader_versions = versions,
                    Err(message) => self.error = Some(message),
                }
            }
            Message::Javas(found) => {
                self.javas = found;
                self.javas_loading = false;
                self.status = "Java detectados".to_string();
            }
            Message::PackSearch(hits) => {
                self.packs_loading = false;
                self.packs = hits;
            }
            Message::PackSearchFailed(message) => {
                self.packs_loading = false;
                self.error = Some(format!("búsqueda de modpacks: {message}"));
            }
            Message::PackVersions { slug, versions } => {
                self.packs_loading = false;
                if self.pack_versions_slug.as_deref() == Some(slug.as_str()) {
                    self.pack_versions = versions;
                }
            }
            Message::PackVersionsFailed(message) => {
                self.packs_loading = false;
                self.error = Some(format!("versiones del pack: {message}"));
            }
            Message::PackDetail(mut detail) => {
                self.packs_loading = false;
                // El detalle no trae autor: si la búsqueda sí lo tenía, lo hereda.
                if detail.author.is_none() {
                    if let Some(hit) = self
                        .packs
                        .iter()
                        .find(|pack| pack.slug == detail.slug)
                    {
                        detail.author = hit.author.clone();
                    }
                }
                self.pack_detail = Some(detail);
            }
            Message::PackDetailFailed(message) => {
                self.packs_loading = false;
                self.error = Some(format!("detalle del pack: {message}"));
            }
            Message::SupportedMcs(list) => {
                // Si la versión elegida no tiene soporte en este cargador, salta a
                // la más reciente que sí (por orden del manifiesto, que es el orden
                // real de lanzamiento). Evita el «crear y que reviente después».
                if let Some(supported) = &list {
                    if !supported.iter().any(|id| id == &self.form.mc) {
                        let newest = self
                            .manifest
                            .as_ref()
                            .and_then(|manifest| {
                                manifest
                                    .versions
                                    .iter()
                                    .map(|entry| &entry.id)
                                    .find(|id| supported.contains(id))
                                    .cloned()
                            })
                            .or_else(|| supported.first().cloned());
                        if let Some(newest) = newest {
                            self.form.mc = newest;
                            if !self.form.name_edited {
                                self.form.name = Instance::suggested_name(
                                    self.form.loader,
                                    &self.form.mc,
                                );
                            }
                            self.begin_loader_fetch();
                        }
                    }
                }
                self.form.supported_mcs = list;
            }
            Message::JobDone(status) => {
                self.job = None;
                self.status = status.clone();
                self.notify(status, ToastKind::Ok);
            }
            Message::PackInstalled(slug) => {
                self.job = None;
                // La instancia se creó en el hilo de instalación: recargar el
                // índice para que aparezca YA en la lista, sin reiniciar.
                self.store = InstanceStore::load(&self.paths);
                // El icono del pack, si lo hay, queda como cara de la instancia.
                if let Some(instance) = self
                    .store
                    .instances
                    .iter_mut()
                    .find(|instance| instance.slug == slug)
                {
                    instance.icon = self.pending_pack_icon.take().or(instance.icon.clone());
                    let _ = self.store.save(&self.paths);
                }
                self.selected = Some(slug);
                self.screen = Screen::Home;
                self.status = "Modpack instalado: listo para JUGAR".to_string();
                self.notify("Modpack instalado", ToastKind::Ok);
            }
            Message::GameExit(result) => {
                self.job = None;
                if result.ok {
                    self.status = format!("El juego terminó con código {}", result.code);
                } else {
                    self.status = "El juego se cerró inesperadamente".to_string();
                    self.error = Some(
                        result
                            .cause
                            .clone()
                            .unwrap_or_else(|| format!("código de salida {}", result.code)),
                    );
                }
                self.last_game_exit = Some(result);
            }
            Message::Failed(message) => {
                self.job = None;
                self.error = Some(message.clone());
                self.notify(format!("Error: {message}"), ToastKind::Error);
            }
            Message::Played { slug } => {
                if let Some(instance) = self
                    .store
                    .instances
                    .iter_mut()
                    .find(|instance| instance.slug == slug)
                {
                    instance.last_played = Some(now());
                }
                let _ = self.store.save(&self.paths);
            }
            Message::Log(line) => self.push_log(line),
        }
    }

    /// Añade una línea al log sin dejar crecer la lista sin límite.
    fn push_log(&mut self, line: String) {
        const MAX_LINES: usize = 400;
        // Una palabra larga sin espacios no parte: con mucho más que esto, el
        // ScrollArea central se ensancha y se sale de la ventana.
        const MAX_CHARS: usize = 120;

        let line = if line.chars().count() > MAX_CHARS {
            let cut: String = line.chars().take(MAX_CHARS).collect();
            format!("{cut}…")
        } else {
            line
        };
        self.log.push(line);
        if self.log.len() > MAX_LINES {
            let overflow = self.log.len() - MAX_LINES;
            self.log.drain(..overflow);
        }
    }

    // ── Navegación ───────────────────────────────────────────────────────────

    pub(crate) fn open_new(&mut self) {
        self.screen = Screen::New;
        self.confirm_delete = None;
        self.form.name_edited = false;
        if !self.form.mc.is_empty() {
            self.form.name = Instance::suggested_name(self.form.loader, &self.form.mc);
        }
        if self.form.loader != LoaderKind::Vanilla && self.form.supported_mcs.is_none() {
            self.begin_supported_fetch();
        }
    }

    // ── Manifiesto ───────────────────────────────────────────────────────────

    fn load_manifest(&self) {
        let tx = self.tx.clone();
        let paths = self.paths.clone();
        std::thread::spawn(move || {
            let http = HttpClient::new();
            match install::fetch_manifest(&http, &paths) {
                Ok(manifest) => {
                    tx.send(Message::Manifest(manifest));
                }
                Err(err) => {
                    tx.send(Message::ManifestFailed(err.to_string()));
                }
            }
        });
    }

    pub(crate) fn retry_manifest(&mut self) {
        if self.manifest_loading {
            return;
        }
        self.manifest_loading = true;
        self.error = None;
        self.load_manifest();
    }

    /// Pregunta al cargador qué versiones de MC soporta (hilo de fondo).
    pub(crate) fn begin_supported_fetch(&mut self) {
        let kind = self.form.loader;
        if kind == LoaderKind::Vanilla {
            self.form.supported_mcs = None;
            return;
        }
        let paths = self.paths.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let http = HttpClient::new();
            let progress = Progress::none();
            let ctx = LoaderCtx {
                http: &http,
                paths: &paths,
                progress: &progress,
                filter: VersionFilter::default(),
            };
            let supported = loaders::supported_mcs(&ctx, kind);
            tx.send(Message::SupportedMcs(supported));
        });
    }

    // ── Versiones del cargador ───────────────────────────────────────────────

    pub(crate) fn begin_loader_fetch(&mut self) {
        let mc = self.form.mc.clone();
        let kind = self.form.loader;
        if mc.is_empty() || kind == LoaderKind::Vanilla || !kind.is_implemented() {
            self.form.loader_loading = false;
            return;
        }
        if self.form.loader_query == Some((mc.clone(), kind)) {
            // Ya se está pidiendo (o ya se pidió): no repetir en bucle.
            return;
        }
        self.form.loader_query = Some((mc.clone(), kind));
        self.form.loader_versions.clear();
        self.form.loader_version.clear();
        self.form.loader_loading = true;

        let paths = self.paths.clone();
        let filter = self.config.version_filter();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let http = HttpClient::new();
            let progress = Progress::none();
            let ctx = LoaderCtx {
                http: &http,
                paths: &paths,
                progress: &progress,
                filter,
            };
            let result = loaders::get(kind)
                .list_versions(&ctx, &mc)
                .map_err(|err| err.to_string());
            tx.send(Message::LoaderVersions { mc, kind, result });
        });
    }

    /// Rellena el formulario de edición con la instancia seleccionada.
    pub(crate) fn open_edit(&mut self, slug: &str) {
        let Some(instance) = self.store.find(slug).cloned() else {
            return;
        };
        self.screen = Screen::Edit;
        self.confirm_delete = None;
        self.form.name = instance.name.clone();
        self.form.name_edited = true;
        self.form.mc = instance.mc_version.clone();
        self.form.loader = instance.loader;
        self.form.loader_version = instance.loader_version.clone().unwrap_or_default();
        self.form.loader_versions.clear();
        self.form.loader_query = None;
        self.form.supported_mcs = None;
        self.editing_slug = Some(slug.to_string());
        if instance.loader != LoaderKind::Vanilla {
            self.begin_loader_fetch();
        }
    }

    /// Guarda los cambios del formulario de edición sobre la instancia.
    /// Si cambió versión/cargador, relanza la instalación (mods y mundos se quedan).
    pub(crate) fn save_edit(&mut self) {
        let Some(slug) = self.editing_slug.clone() else {
            return;
        };
        let name = self.form.name.trim().to_string();
        if name.is_empty() || self.form.mc.is_empty() {
            self.error = Some("Nombre y versión de Minecraft no pueden quedar vacíos.".into());
            return;
        }
        let loader_version = match self.chosen_loader_version() {
            Ok(value) => value,
            Err(message) => {
                self.error = Some(message);
                return;
            }
        };

        let Some(instance) = self.store.instances.iter_mut().find(|i| i.slug == slug) else {
            return;
        };
        instance.name = name.clone();
        instance.mc_version = self.form.mc.clone();
        instance.loader = self.form.loader;
        instance.loader_version = loader_version.clone();
        instance.ram_mb = self.form.ram_mb;
        instance.width = self.form.width;
        instance.height = self.form.height;
        if let Err(err) = self.store.save(&self.paths) {
            self.error = Some(err.to_string());
            return;
        }
        self.screen = Screen::Home;
        self.status = format!("«{name}» actualizada");

        // Reinstalar si cambió la versión o el cargador (repone lo que falte).
        self.start_install(
            format!("Actualizando {name}"),
            self.form.loader,
            self.form.mc.clone(),
            loader_version,
            slug,
            false,
        );
    }

    /// Instala Sodium en una instancia Fabric que ya existe.
    pub(crate) fn add_sodium(&mut self, slug: &str) {
        if self.job.is_some() {
            return;
        }
        let Some(instance) = self.store.find(slug).cloned() else {
            return;
        };
        if instance.loader != LoaderKind::Fabric {
            self.error = Some("Sodium solo aplica a instancias de Fabric.".into());
            return;
        }
        let paths = self.paths.clone();
        let tx = self.tx.clone();
        let game_dir = instance.game_dir(&paths);
        let mc = instance.mc_version.clone();

        self.job = Some(Job {
            label: "Sodium".into(),
            phase: "Buscando la versión".into(),
            total: 0,
            done: 0,
            started: std::time::Instant::now(),
        });
        self.error = None;

        std::thread::spawn(move || {
            let http = HttpClient::new();
            let progress = Progress::new(GuiSink::new(tx.clone()));
            let result = (|| -> Result<String, crate::Error> {
                let Some(sodium) = crate::core::sodium::latest_for(&http, &mc)? else {
                    return Ok(format!("Sodium aún no soporta Minecraft {mc}"));
                };
                let jar = crate::core::sodium::install(&http, &sodium, &game_dir, &progress)?;
                Ok(format!(
                    "Sodium {} instalado: {}",
                    sodium.version_number,
                    jar.file_name().unwrap_or_default().to_string_lossy()
                ))
            })();
            match result {
                Ok(status) => {
                    tx.send(Message::JobDone(status));
                }
                Err(err) => {
                    tx.send(Message::Failed(err.to_string()));
                }
            }
        });
    }

    /// Versión del cargador elegida (o la última estable de la lista).
    fn chosen_loader_version(&self) -> Result<Option<String>, String> {
        if self.form.loader == LoaderKind::Vanilla {
            return Ok(None);
        }
        if !self.form.loader.is_implemented() {
            return Err(format!(
                "{} todavía no está implementado (fases 4 y 5).",
                self.form.loader.label()
            ));
        }
        if !self.form.loader_version.is_empty() {
            return Ok(Some(self.form.loader_version.clone()));
        }
        if self.form.loader_loading {
            return Err("Espera a que carguen las versiones del cargador.".to_string());
        }
        let chosen = self
            .form
            .loader_versions
            .iter()
            .find(|version| version.stable)
            .or_else(|| self.form.loader_versions.first())
            .ok_or_else(|| {
                format!(
                    "{} no publica versiones para Minecraft {}.",
                    self.form.loader.label(),
                    self.form.mc
                )
            })?;
        Ok(Some(chosen.id.clone()))
    }

    // ── Modpacks (Modrinth) ──────────────────────────────────────────────────

    pub(crate) fn search_packs(&mut self) {
        if self.packs_loading {
            return;
        }
        self.packs_loading = true;
        let query = self.pack_search.trim().to_string();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let http = HttpClient::new();
            match modrinth::search(&http, &query, 20) {
                Ok(hits) => {
                    tx.send(Message::PackSearch(hits));
                }
                Err(err) => {
                    tx.send(Message::PackSearchFailed(err.to_string()));
                }
            }
        });
    }

    pub(crate) fn fetch_pack_versions(&mut self, slug: String) {
        if self.packs_loading {
            return;
        }
        self.packs_loading = true;
        self.pack_versions_slug = Some(slug.clone());
        self.pack_versions.clear();
        // El detalle (icono, autor, body) va por su lado: si falla, la lista
        // de versiones sigue funcionando.
        {
            let tx = self.tx.clone();
            let slug_detail = slug.clone();
            std::thread::spawn(move || {
                let http = HttpClient::new();
                match modrinth::detail(&http, &slug_detail) {
                    Ok(detail) => {
                        tx.send(Message::PackDetail(detail));
                    }
                    Err(err) => {
                        tx.send(Message::PackDetailFailed(err.to_string()));
                    }
                }
            });
        }
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let http = HttpClient::new();
            match modrinth::versions(&http, &slug) {
                Ok(versions) => {
                    tx.send(Message::PackVersions { slug, versions });
                }
                Err(err) => {
                    tx.send(Message::PackVersionsFailed(err.to_string()));
                }
            }
        });
    }

    /// Crea la instancia del pack y lanza la instalación completa: juego base
    /// (loader + MC del índice) → descarga del .mrpack → mods + overrides.
    pub(crate) fn start_pack_install(&mut self, hit: &modrinth::PackHit, version: &modrinth::PackVersion) {
        if self.job.is_some() {
            return;
        }
        let slug = hit.slug.clone();
        let version_id = version.id.clone();
        let url = match version.files.iter().find(|file| file.primary).or_else(|| version.files.first()) {
            Some(file) => file.url.clone(),
            None => {
                self.error = Some(format!("el pack «{}» no tiene fichero descargable", hit.title));
                return;
            }
        };
        let name = hit.title.clone();
        let tx = self.tx.clone();
        let paths = self.paths.clone();
        let config = self.config.clone();
        // Para vestir la instancia nueva con el icono del pack al terminar.
        self.pending_pack_icon = hit.icon_url.clone();

        self.job = Some(Job {
            label: format!("Pack {name}"),
            phase: "Preparando".to_string(),
            total: 0,
            done: 0,
            started: std::time::Instant::now(),
        });
        self.error = None;

        std::thread::spawn(move || {
            let http = HttpClient::new();
            let progress = Progress::new(GuiSink::new(tx.clone()));
            let result = (|| -> Result<String, crate::Error> {
                progress.phase("Leyendo el pack");
                let dest = paths
                    .packs_dir()
                    .join(crate::core::paths::sanitize(&format!("{slug}-{version_id}")) + ".mrpack");
                http.download(&Download::new(&url, &dest))?;
                let index = modrinth::read_index(&dest)?;

                // 1) Juego base con el loader y la versión del índice.
                let slug_instancia = {
                    let mut store = InstanceStore::load(&paths);
                    let mut instance = Instance::new(&name, index.mc_version().as_deref().unwrap_or("release"), index.loader_kind());
                    instance.loader_version = index.loader_version();
                    instance.from_pack = Some(name.clone());
                    let slug_instancia = store.add(instance, &paths)?;
                    store.save(&paths)?;
                    slug_instancia
                };
                let game_dir = paths.instance_dir(&slug_instancia);
                std::fs::create_dir_all(&game_dir).map_err(|e| crate::Error::io(&game_dir, e))?;

                let request = PlayRequest {
                    kind: index.loader_kind(),
                    mc_version: index.mc_version().unwrap_or_else(|| "release".into()),
                    loader_version: index.loader_version(),
                    game_dir: game_dir.clone(),
                    username: config.username_or_default(),
                    memory_mb: config.clamped_ram(),
                    width: 854,
                    height: 480,
                    java: config.java_path.clone(),
                    extra_jvm_args: Vec::new(),
                    filter: config.version_filter(),
                };
                let opts = InstallOptions::default();
                // prepare() baja el juego base entero (client, libs, natives, assets).
                install::prepare(&http, &paths, &request, &opts, &progress)?;

                // 2) Mods del índice + overrides del pack.
                modrinth::install(&http, &paths, &dest, &game_dir, opts.threads, &progress)?;
                Ok(slug_instancia)
            })();
            match result {
                Ok(slug) => {
                    logging::info(&format!("modpack instalado en {slug}"));
                    tx.send(Message::PackInstalled(slug));
                }
                Err(err) => {
                    logging::error(&format!("modpack fallido: {err}"));
                    tx.send(Message::Failed(err.to_string()));
                }
            }
        });
    }

    // ── Trabajos ─────────────────────────────────────────────────────────────

    fn install_options(&self) -> InstallOptions {
        InstallOptions {
            threads: self.config.threads.clamp(1, 16),
            dry_run: false,
            skip_assets: false,
            mojang_runtime: self.config.use_mojang_runtime,
        }
    }

    pub(crate) fn start_create(&mut self) {
        if self.job.is_some() {
            return;
        }
        let name = self.form.name.trim().to_string();
        if name.is_empty() {
            self.error = Some("Ponle nombre a la instancia.".to_string());
            return;
        }
        if self.form.mc.is_empty() {
            self.error = Some("Elige una versión de Minecraft.".to_string());
            return;
        }
        let loader_version = match self.chosen_loader_version() {
            Ok(value) => value,
            Err(message) => {
                self.error = Some(message);
                return;
            }
        };

        let kind = self.form.loader;
        let with_sodium = self.form.with_sodium && kind == LoaderKind::Fabric;
        let mc = self.form.mc.clone();
        let mut instance = Instance::new(&name, &mc, kind);
        instance.loader_version = loader_version.clone();
        instance.ram_mb = self.form.ram_mb;
        instance.width = self.form.width;
        instance.height = self.form.height;

        let slug = match self.store.add(instance, &self.paths) {
            Ok(slug) => slug,
            Err(err) => {
                self.error = Some(err.to_string());
                return;
            }
        };
        if let Err(err) = self.store.save(&self.paths) {
            self.error = Some(err.to_string());
            return;
        }

        self.selected = Some(slug.clone());
        self.screen = Screen::Home;
        self.status = format!("«{name}» creada");
        self.start_install(
            format!("Instalando {name}"),
            kind,
            mc,
            loader_version,
            slug,
            with_sodium,
        );
    }

    pub(crate) fn start_repair(&mut self, slug: &str) {
        let Some(instance) = self.store.find(slug).cloned() else {
            return;
        };
        self.start_install(
            format!("Reparando {}", instance.name),
            instance.loader,
            instance.mc_version.clone(),
            instance.loader_version.clone(),
            slug.to_string(),
            false,
        );
    }

    /// Resuelve el manifiesto (vanilla + cargador) y baja lo que falte.
    fn start_install(
        &mut self,
        label: String,
        kind: LoaderKind,
        mc: String,
        loader_version: Option<String>,
        instance_slug: String,
        with_sodium: bool,
    ) {
        if self.job.is_some() {
            return;
        }
        let paths = self.paths.clone();
        let filter = self.config.version_filter();
        let opts = self.install_options();
        let tx = self.tx.clone();

        self.job = Some(Job {
            label,
            phase: "Preparando".to_string(),
            total: 0,
            done: 0,
            started: std::time::Instant::now(),
        });
        self.error = None;

        std::thread::spawn(move || {
            let http = HttpClient::new();
            let progress = Progress::new(GuiSink::new(tx.clone()));
            let result = loaders::resolve(
                &http,
                &paths,
                &progress,
                filter,
                kind,
                &mc,
                loader_version.as_deref(),
                &opts,
            )
            .and_then(|(version_id, version)| {
                install::download(&http, &paths, &version, &version_id, &opts, &progress)?;

                // Sodium (Fabric): un mod más en mods/, con hash SHA-1.
                if with_sodium && kind == LoaderKind::Fabric {
                    if let Some(sodium) = crate::core::sodium::latest_for(&http, &mc)? {
                        let game_dir = paths.instance_dir(&instance_slug);
                        let jar = crate::core::sodium::install(
                            &http,
                            &sodium,
                            &game_dir,
                            &progress,
                        )?;
                        progress.message(format!(
                            "Sodium {}: {}",
                            sodium.version_number,
                            jar.file_name().unwrap_or_default().to_string_lossy()
                        ));
                    } else {
                        progress.message(format!(
                            "Sodium aún no soporta Minecraft {mc}: se omite"
                        ));
                    }
                }
                Ok(version_id)
            });
            match result {
                Ok(version_id) => {
                    logging::info(&format!("instalación completada: {version_id}"));
                    tx.send(Message::JobDone(format!("«{version_id}» listo")));
                }
                Err(err) => {
                    logging::error(&format!("instalación fallida: {err}"));
                    tx.send(Message::Failed(err.to_string()));
                }
            }
        });
    }

    pub(crate) fn start_play(&mut self, slug: &str) {
        if self.job.is_some() {
            return;
        }
        let Some(instance) = self.store.find(slug).cloned() else {
            self.error = Some("Esa instancia ya no existe.".to_string());
            return;
        };

        let paths = self.paths.clone();
        let game_dir = instance.game_dir(&paths);
        if let Err(err) = std::fs::create_dir_all(&game_dir) {
            self.error = Some(crate::Error::io(&game_dir, err).to_string());
            return;
        }

        let request = PlayRequest {
            kind: instance.loader,
            mc_version: instance.mc_version.clone(),
            loader_version: instance.loader_version.clone(),
            game_dir,
            username: self.config.username_or_default(),
            memory_mb: instance.ram_clamped(),
            width: instance.width,
            height: instance.height,
            java: instance
                .java_path
                .clone()
                .or_else(|| self.config.java_path.clone()),
            extra_jvm_args: Vec::new(),
            filter: self.config.version_filter(),
        };
        let opts = self.install_options();
        let tx = self.tx.clone();
        let label = format!("Lanzando {}", instance.name);

        self.job = Some(Job {
            label,
            phase: "Preparando".to_string(),
            total: 0,
            done: 0,
            started: std::time::Instant::now(),
        });
        self.error = None;

        std::thread::spawn(move || {
            let http = HttpClient::new();
            let progress = Progress::new(GuiSink::new(tx.clone()));

            // El log de la sesión de juego empieza ANTES de preparar: así un fallo
            // de preparación (red, Java, instaladores…) también queda registrado y
            // en logs/crash/ no quedan ficheros vacíos ni «fantasma».
            let mut mirror = crash::GameLogMirror::new(&paths, &request_slug(&request));
            mirror.write_line(&format!(
                "── McLite {} · sesión de juego · instancia {} ──",
                LAUNCHER_VERSION,
                request_slug(&request)
            ));

            let prepared = match install::prepare(&http, &paths, &request, &opts, &progress) {
                Ok(prepared) => prepared,
                Err(err) => {
                    mirror.write_line(&format!("error al preparar: {err}"));
                    logging::error(&format!("fallo al preparar el lanzamiento: {err}"));
                    tx.send(Message::Failed(err.to_string()));
                    return;
                }
            };

            tx.send(Message::Progress(ProgressEvent::Phase(
                "Lanzando el juego".to_string(),
            )));
            let command_line = prepared.plan.display_string();
            tx.send(Message::Log(format!("$ {command_line}")));
            mirror.write_line(&format!("$ {command_line}"));
            logging::info(&format!(
                "lanzando {} con {}",
                prepared.version_id,
                prepared.java.display()
            ));

            let (log_tx, log_rx) = channel::<String>();
            let mut child = match prepared.plan.spawn(Some(log_tx)) {
                Ok(child) => child,
                Err(err) => {
                    mirror.write_line(&format!("error al arrancar: {err}"));
                    logging::error(&format!("no pudo arrancar el juego: {err}"));
                    tx.send(Message::Failed(err.to_string()));
                    return;
                }
            };

            // Las líneas del juego van a la UI **y** al espejo en
            // logs/crash/<instancia>-<timestamp>.log que sobrevive al cierre.
            let game_dir = request.game_dir.clone();
            let slug = request_slug(&request);
            let mut tail: Vec<String> = Vec::new();
            for line in log_rx {
                mirror.write_line(&line);
                tail.push(line.clone());
                if tail.len() > 400 {
                    tail.remove(0);
                }
                tx.send(Message::Log(line));
            }

            let status = child.wait();
            let result = crash::classify(status, &game_dir, std::path::PathBuf::new(), &tail);
            mirror.write_line(&match &result.cause {
                Some(cause) => format!("── fin: {cause} ──"),
                None => format!("── fin: ok, código {} ──", result.code),
            });
            let log_path = mirror.finish();
            let result = crash::GameExit { log_path, ..result };
            logging::info(&format!(
                "el juego terminó: ok={} código={}",
                result.ok, result.code
            ));
            tx.send(Message::Played { slug });
            tx.send(Message::GameExit(result));
        });
    }

    pub(crate) fn detect_javas(&mut self) {
        if self.javas_loading {
            return;
        }
        self.javas_loading = true;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let found = java::detect_all();
            tx.send(Message::Javas(found));
        });
    }

    // ── Pintado ──────────────────────────────────────────────────────────────

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        // Todo se copia antes del closure para no atascar préstamos.
        let job = self
            .job
            .as_ref()
            .map(|job| (job.label.clone(), job.phase.clone(), job.done, job.total));
        let error = self.error.clone();
        let status = self.status.clone();
        let mut clear_error = false;

        egui::Frame::new()
            .fill(theme::SIDEBAR)
            .inner_margin(egui::Margin::same(6))
            .show(ui, |ui| {
                if let Some((label, phase, done, total)) = job {
                    if total > 0 {
                        let fraction = (done as f32 / total as f32).clamp(0.0, 1.0);
                        // Velocidad + ETA (usa el Job con su instante de arranque).
                        let suffix = self
                            .job
                            .as_ref()
                            .and_then(|job| job.speed_and_eta())
                            .map(|(speed, eta)| format!(" · {:.0}/s · queda {eta}", speed))
                            .unwrap_or_default();
                        ui.add(
                            egui::ProgressBar::new(fraction)
                                .show_percentage()
                                .text(format!("{label} · {phase}: {done}/{total}{suffix}")),
                        );
                    } else {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(format!("{label} · {phase}…"));
                        });
                    }
                } else if let Some(error) = error {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("error: {error}")).color(theme::DANGER));
                        if ui.small_button("×").clicked() {
                            clear_error = true;
                        }
                    });
                } else {
                    ui.horizontal(|ui| {
                        ui.label(theme::muted(&status));
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.label(theme::muted(format!("McLite {LAUNCHER_VERSION}")));
                            },
                        );
                    });
                }
            });

        if clear_error {
            self.error = None;
        }
    }
}

/// El slug de la instancia que se pidió lanzar (para marcar «última partida»).
fn request_slug(request: &PlayRequest) -> String {
    request
        .game_dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

impl eframe::App for McLiteApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain();

        // Recordar la última instancia seleccionada (barato: solo al cambiar).
        if self.config.last_instance.as_deref() != self.selected.as_deref() {
            self.config.last_instance = self.selected.clone();
            let _ = self.config.save(&self.paths);
        }

        // Y el tamaño de la ventana (para restaurarlo al arrancar).
        let size = ctx.input(|input| input.screen_rect.size());
        if size.x > 100.0 && size.y > 100.0 {
            let changed = self
                .config
                .window
                .map(|window| {
                    (window.width - size.x).abs() > 4.0 || (window.height - size.y).abs() > 4.0
                })
                .unwrap_or(true);
            if changed {
                self.config.window = Some(crate::core::config::WindowSize {
                    width: size.x,
                    height: size.y,
                });
                let _ = self.config.save(&self.paths);
            }
        }

        // Transición de pantalla: se anima la entrada (caída leve que se asienta).
        let previous = self.screen_from.unwrap_or(self.screen);
        if self.screen != previous {
            self.screen_fade = 0.0;
        }
        self.screen_from = Some(self.screen);
        if self.screen_fade < 1.0 {
            let delta = ctx.input(|input| input.stable_dt).min(0.06);
            self.screen_fade = (self.screen_fade + delta / 0.18).min(1.0);
        }

        // Barra de estado degradada, esquinas superiores redondeadas.
        egui::TopBottomPanel::bottom("status_bar")
            .frame(
                egui::Frame::new()
                    .fill(theme::SIDEBAR)
                    .inner_margin(egui::Margin::same(6))
                    .corner_radius(egui::CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }),
            )
            .show(ctx, |ui| self.status_bar(ui));

        egui::SidePanel::left("instances")
            .frame(egui::Frame::new().fill(theme::SIDEBAR))
            .default_width(244.0)
            .min_width(208.0)
            .max_width(320.0)
            .resizable(false)
            .show(ctx, |ui| instances::show(self, ui));

        let screen = self.screen;
        let fade = self.screen_fade;
        egui::CentralPanel::default().show(ctx, |ui| {
            // Entrada de pantalla: pequeño desplazamiento vertical que se asienta.
            let offset = (1.0 - fade) * 10.0;
            egui::Frame::new()
                .outer_margin(egui::Margin { top: (offset as i8), ..Default::default() })
                .show(ui, |ui| match screen {
                    Screen::Home => home::show(self, ui),
                    Screen::New => new_instance::show(self, ui),
                    Screen::Edit => edit_instance::show(self, ui),
                    Screen::Modpacks => modpacks::show(self, ui),
                    Screen::Settings => settings::show(self, ui),
                });
            // Repintar hasta terminar la transición.
            if fade < 1.0 {
                ctx.request_repaint();
            }
        });

        self.show_toasts(ctx);

        // Repintado por evento: los hilos despiertan la GUI al enviar (MsgTx).
        // Solo mientras hay trabajo (ETA/velocidad cambian con el tiempo) o una
        // transición en curso se necesita un pulso periódico; en reposo, 0 % CPU.
        if self.job.is_some() || self.screen_fade < 1.0 || !self.toasts.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
    }
}

/// Registra fallos de arranque en un log junto al exe: con subsistema "windows"
/// no hay consola, así que un pánico o un error de wgpu moriría en silencio.
fn crash_log(message: &str) {
    let path = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("mclite-crash.log")))
        .unwrap_or_else(|| std::env::temp_dir().join("mclite-crash.log"));
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        use std::io::Write as _;
        let _ = writeln!(file, "[{seconds}] {message}");
    }
}

/// Los pánicos (p. ej. al crear la app o inicializar la GPU) quedan en el log.
fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        crash_log(&format!("pánico: {info}"));
        default_hook(info);
    }));
}

/// Abre la ventana. Sin argumentos en la CLI, la GUI es la cara del launcher.
pub fn run() -> std::process::ExitCode {
    install_panic_hook();

    // Restaurar el tamaño de la sesión anterior si lo tenemos guardado.
    let saved_size = std::fs::read_to_string(
        Paths::discover()
            .map(|paths| paths.config_file())
            .unwrap_or_else(|_| std::path::PathBuf::from("mclite.json")),
    )
    .ok()
    .and_then(|raw| serde_json::from_str::<LauncherConfig>(&raw).ok())
    .and_then(|config| config.window)
    .map(|window| [window.width, window.height]);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(format!("McLite {LAUNCHER_VERSION}"))
            .with_inner_size(saved_size.unwrap_or([980.0_f32, 620.0]))
            .with_min_inner_size([820.0_f32, 520.0]),
        ..Default::default()
    };

    let result = eframe::run_native(
        "McLite",
        options,
        Box::new(|cc| Ok(Box::new(McLiteApp::new(cc)))),
    );

    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            crash_log(&format!("no pude abrir la ventana: {err}"));
            eprintln!("no pude abrir la ventana: {err}");
            std::process::ExitCode::FAILURE
        }
    }
}
