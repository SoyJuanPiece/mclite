//! Renderizado: qué backend pinta la ventana y cómo elegirlo sin dejar a nadie fuera.
//!
//! La UI es egui, y egui puede pintarse con **wgpu** (DirectX 12 / Vulkan / GLES) o
//! con **OpenGL** (glow). El defecto es wgpu porque es lo más rápido, pero hay GPUs
//! —integradas viejas, sobre todo— donde wgpu pinta las *formas* y pierde el
//! *texto*: la UI aparece como cajas y botones sin una sola letra.
//!
//! La causa es el atlas de fuentes. Todo lo que pinta egui lleva coordenadas de
//! textura, incluso un rectángulo de color plano (va al píxel blanco del atlas).
//! Los glifos llegan después, en **subidas parciales** (`Queue::write_texture`
//! con un `origin` distinto de cero). Si el driver ignora o corrompe esas subidas
//! parciales, el píxel blanco sigue ahí (las formas se ven) pero las zonas de los
//! glifos quedan vacías → rectángulos y botones, cero letras. Es exactamente lo
//! que se ve en esas capturas.
//!
//! Por eso aquí hay tres cosas:
//!
//! 1. [`Mode`] — el backend elegido, persistido en `config.json`.
//! 2. [`probe`] — una **prueba real** del camino que falla: crea una textura,
//!    espejo, mira si llega el píxel, y hace una subida parcial en `origin`
//!    distinto de cero, igual que el atlas de fuentes. Si el driver no lo honra,
//!    la prueba lo detecta antes de abrir la ventana.
//! 3. [`run`] — la cadena de intentos: lo elegido, y si falla o no pasa la
//!    prueba, OpenGL; y si tampoco, un mensaje legible en vez de una ventana
//!    invisible.
//!
//! Nada de esto cambia el aspecto de la UI: es solo *con qué* se pinta.

use crate::core::logging;

/// Backend de render de la ventana.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// wgpu, eligiendo el mejor adaptador (DX12 → Vulkan → GLES en Windows).
    Auto,
    /// OpenGL (glow). Es el camino que mejor aguanta drivers viejos.
    Gl,
    /// wgpu forzando GLES (OpenGL por detrás de wgpu). Útil si el driver tiene
    /// buen OpenGL pero el backend nativo de wgpu falla.
    WgpuGl,
}

impl Mode {
    /// Clave tal como se guarda en `config.json`.
    pub fn key(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Gl => "gl",
            Self::WgpuGl => "wgpu-gl",
        }
    }

    /// Nombre para la UI.
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Automático (recomendado)",
            Self::Gl => "OpenGL (compatibilidad)",
            Self::WgpuGl => "wgpu sobre OpenGL",
        }
    }

    /// Descripción para la UI.
    pub fn hint(self) -> &'static str {
        match self {
            Self::Auto => "Prueba DirectX 12/Vulkan y, si algo falla, pasa solo a OpenGL.",
            Self::Gl => "Úsalo si la ventana se ve sin letras o con colores raros.",
            Self::WgpuGl => "Término medio: la ruta de wgpu, pero sobre OpenGL.",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "auto" | "" => Some(Self::Auto),
            "gl" | "glow" | "opengl" | "gles" => Some(Self::Gl),
            "wgpu-gl" | "wgpu_gl" | "wgpuopengl" | "wgpu-opengl" => Some(Self::WgpuGl),
            _ => None,
        }
    }

    /// `None` = no hay variable de entorno puesta.
    pub fn from_env() -> Option<Self> {
        std::env::var("MCLITE_RENDERER")
            .ok()
            .and_then(|raw| Self::parse(&raw))
    }
}

impl Default for Mode {
    fn default() -> Self {
        Self::Auto
    }
}

/// Un intento concreto: qué renderer usar y con qué backends de wgpu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Attempt {
    /// Nombre corto para el log.
    name: &'static str,
    /// `true` = pintar con OpenGL (glow); `false` = con wgpu.
    glow: bool,
    /// Backends que se le permiten a wgpu (`None` = los suyos por defecto).
    backends: Option<wgpu::Backends>,
}

/// Cadena de intentos para un modo, en orden.
fn chain(mode: Mode) -> Vec<Attempt> {
    let wgpu_default = Attempt {
        name: "wgpu (DX12/Vulkan)",
        glow: false,
        backends: None,
    };
    let wgpu_gl = Attempt {
        name: "wgpu sobre OpenGL",
        glow: false,
        backends: Some(wgpu::Backends::GL),
    };
    let opengl = Attempt {
        name: "OpenGL (glow)",
        glow: true,
        backends: None,
    };

    match mode {
        // Lo más compatible primero, pero sin castigar a quien no lo necesita:
        // si wgpu pasa la prueba se queda.
        Mode::Auto => vec![wgpu_default, opengl, wgpu_gl],
        // Forzado a OpenGL: sin experimentos.
        Mode::Gl => vec![opengl],
        Mode::WgpuGl => vec![wgpu_gl, opengl],
    }
}

/// Resultado de la prueba de GPU.
pub enum Probe {
    /// El backend pinta y las subidas parciales de textura funcionan.
    Ok { adapter: String, partial_upload: bool },
    /// wgpu pinta, pero pierde las subidas parciales: es el fallo del texto.
    PartialUploadsBroken { adapter: String },
    /// Ni siquiera se pudo abrir el dispositivo.
    Unavailable(String),
}

impl Probe {
    pub fn adapter(&self) -> &str {
        match self {
            Self::Ok { adapter, .. } | Self::PartialUploadsBroken { adapter } => adapter,
            Self::Unavailable(_) => "—",
        }
    }
}

/// Cuánto se espera a que la GPU responda a la prueba antes de darla por perdida.
/// Generoso a propósito: en una PC modesta, arrancar un dispositivo puede tardar.
const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(6);

/// Elige el backend y abre la ventana. No vuelve salvo que se cierre.
///
/// La cadena es: se prueba cada intento (crear dispositivo + subir una textura
/// parcial) y se abre la ventana con el primero que pasa. Si el que arranca
/// falla igualmente, se pasa al siguiente. El resultado queda en el log.
pub fn run(mode: Mode, title: String, options: eframe::NativeOptions) -> std::process::ExitCode {
    let attempts = chain(mode);
    let mut last_error: Option<String> = None;

    for (index, attempt) in attempts.iter().enumerate() {
        logging::info(&format!(
            "render: probando «{}» ({}/{})",
            attempt.name,
            index + 1,
            attempts.len()
        ));

        if !attempt.glow {
            match probe(attempt.backends) {
                Probe::Ok {
                    adapter,
                    partial_upload,
                } => logging::info(&format!(
                    "render: «{}» OK · GPU: {adapter} · subidas parciales de textura: {}",
                    attempt.name,
                    if partial_upload { "sí" } else { "no" }
                )),
                Probe::PartialUploadsBroken { adapter } => {
                    // Este es EL fallo: la GPU dibuja pero pierde los glifos.
                    logging::warn(&format!(
                        "render: «{}» descartado · GPU: {adapter} · pierde las subidas \
                         parciales de textura (es el fallo de «UI sin letras»). Se pasa al \
                         siguiente backend.",
                        attempt.name
                    ));
                    last_error = Some(format!(
                        "«{}» no sube bien las texturas de fuentes ({adapter})",
                        attempt.name
                    ));
                    continue;
                }
                Probe::Unavailable(why) => {
                    logging::warn(&format!(
                        "render: «{}» no disponible ({why}); se pasa al siguiente backend.",
                        attempt.name
                    ));
                    last_error = Some(format!("«{}» no disponible: {why}", attempt.name));
                    continue;
                }
            }
        }

        let mut options = options.clone();
        options.renderer = if attempt.glow {
            eframe::Renderer::Glow
        } else {
            eframe::Renderer::Wgpu
        };
        if !attempt.glow {
            let backends = attempt.backends;
            let mut wgpu_options = options.wgpu_options.clone();
            if let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut wgpu_options.wgpu_setup {
                if let Some(backends) = backends {
                    setup.instance_descriptor.backends = backends;
                }
            }
            options.wgpu_options = wgpu_options;
        }

        match eframe::run_native(
            &title,
            options,
            Box::new(|cc| Ok(Box::new(crate::app::McLiteApp::new(cc)))),
        ) {
            Ok(()) => {
                logging::info("render: ventana cerrada por el usuario");
                return std::process::ExitCode::SUCCESS;
            }
            Err(err) => {
                logging::error(&format!(
                    "render: «{}» falló al abrir la ventana: {err}",
                    attempt.name
                ));
                last_error = Some(format!("«{}»: {err}", attempt.name));
            }
        }
    }

    // Ningún backend pudo: que quede claro en el log y en el crash log de al lado
    // del exe, y avisa al usuario en vez de morir en silencio.
    let detail = last_error.unwrap_or_else(|| "ningún backend de render disponible".to_string());
    logging::error(&format!("render: sin backend utilizable. Último error: {detail}"));
    crate::app::report_fatal(&format!(
        "McLite no pudo dibujar la ventana.\n\n\
         Último error: {detail}\n\n\
         Prueba a abrir el launcher con la opción «OpenGL (compatibilidad)» en \
         Ajustes → Renderizado, o ejecuta `mclite --renderer gl`.\n\n\
         Detalle completo en logs/launcher.log."
    ));
    std::process::ExitCode::FAILURE
}

/// Prueba real del camino que falla, sin abrir ninguna ventana.
///
/// 1. Crea una instancia/dispositivo de wgpu (con `backends` si se limita).
/// 2. Sube una textura de 4×4 y la vuelve a leer: comprueba que la GPU pinta.
/// 3. Hace una **subida parcial en `origin` (1,1)** — el mismo camino que usa el
///    atlas de fuentes para los glifos — y comprueba que ese píxel llegó.
///
/// Si el paso 3 falla, la UI se vería sin texto: es justo lo que hay que detectar.
fn probe(backends: Option<wgpu::Backends>) -> Probe {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(probe_blocking(backends));
    });
    match receiver.recv_timeout(PROBE_TIMEOUT) {
        Ok(probe) => probe,
        Err(_) => Probe::Unavailable("la GPU no respondió a tiempo".into()),
    }
}

/// Las constantes del experimento: textura 4×4, un color puesto de golpe y otro
/// puesto por subida parcial en una esquina.
const PROBE_SIZE: u32 = 4;
const PROBE_FILL: [u8; 4] = [10, 20, 30, 255];
const PROBE_PATCH: [u8; 4] = [200, 100, 50, 255];
const PROBE_PATCH_AT: u32 = 1;

fn probe_blocking(backends: Option<wgpu::Backends>) -> Probe {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: backends.unwrap_or(wgpu::Backends::all()),
        ..Default::default()
    });

    let adapter = match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    })) {
        Ok(adapter) => adapter,
        Err(err) => return Probe::Unavailable(format!("sin adaptador de GPU ({err})")),
    };

    let info = adapter.get_info();
    let adapter_name = format!("{} ({:?}, {:?})", info.name, info.backend, info.device_type);

    let (device, queue) = match pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("mclite render probe"),
        required_features: wgpu::Features::empty(),
        // Límites conservadores: la prueba no necesita nada grande y así también
        // pasa en GPUs modestas (que es donde interesa que pase).
        required_limits: wgpu::Limits::downlevel_defaults(),
        ..Default::default()
    })) {
        Ok(pair) => pair,
        Err(err) => {
            return Probe::Unavailable(format!("no pude abrir el dispositivo ({err})"));
        }
    };

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("mclite render probe texture"),
        size: wgpu::Extent3d {
            width: PROBE_SIZE,
            height: PROBE_SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });

    let full: Vec<u8> = PROBE_FILL
        .iter()
        .cycle()
        .take((PROBE_SIZE * PROBE_SIZE * 4) as usize)
        .copied()
        .collect();

    // Paso 2: subida completa (así llega el píxel blanco del atlas y, con él,
    // todas las formas).
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &full,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * PROBE_SIZE),
            rows_per_image: Some(PROBE_SIZE),
        },
        wgpu::Extent3d {
            width: PROBE_SIZE,
            height: PROBE_SIZE,
            depth_or_array_layers: 1,
        },
    );

    // Paso 3: subida PARCIAL en (1,1) — el camino de los glifos.
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d {
                x: PROBE_PATCH_AT,
                y: PROBE_PATCH_AT,
                z: 0,
            },
            aspect: wgpu::TextureAspect::All,
        },
        &PROBE_PATCH,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );

    // Leer de vuelta. `bytes_per_row` debe ir alineado a 256 (COPY_BYTES_PER_ROW_ALIGNMENT).
    const ROW_PITCH: u32 = 256;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("mclite render probe readback"),
        size: (ROW_PITCH * PROBE_SIZE) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("mclite render probe encoder"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ROW_PITCH),
                rows_per_image: Some(PROBE_SIZE),
            },
        },
        wgpu::Extent3d {
            width: PROBE_SIZE,
            height: PROBE_SIZE,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));

    let (map_sender, map_receiver) = std::sync::mpsc::channel();
    readback.slice(..).map_async(wgpu::MapMode::Read, move |result| {
        let _ = map_sender.send(result);
    });
    if device.poll(wgpu::PollType::wait()).is_err() || map_receiver.recv().is_err() {
        return Probe::Unavailable("la GPU no devolvió la textura de prueba".into());
    }

    let pixel_at = |x: u32, y: u32| -> [u8; 4] {
        let data = readback.slice(..).get_mapped_range();
        let offset = (y * ROW_PITCH + x * 4) as usize;
        let mut pixel = [0u8; 4];
        if let Some(bytes) = data.get(offset..offset + 4) {
            pixel.copy_from_slice(bytes);
        }
        pixel
    };

    let painted = pixel_at(0, 0) == PROBE_FILL;
    let patched = pixel_at(PROBE_PATCH_AT, PROBE_PATCH_AT) == PROBE_PATCH;
    readback.unmap();

    if !painted {
        return Probe::Unavailable(format!("la GPU no pintó la textura de prueba ({adapter_name})"));
    }
    if !patched {
        return Probe::PartialUploadsBroken {
            adapter: adapter_name,
        };
    }
    Probe::Ok {
        adapter: adapter_name,
        partial_upload: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_modo_se_guarda_y_se_relee() {
        assert_eq!(Mode::parse("auto"), Some(Mode::Auto));
        assert_eq!(Mode::parse("GL"), Some(Mode::Gl));
        assert_eq!(Mode::parse(" opengl "), Some(Mode::Gl));
        assert_eq!(Mode::parse("wgpu-gl"), Some(Mode::WgpuGl));
        assert_eq!(Mode::parse("vulkan"), None);
        // Sin valor = automático (una config vieja sigue cargando).
        assert_eq!(Mode::parse(""), Some(Mode::Auto));
    }

    #[test]
    fn automatico_prefiere_wgpu_y_siempre_tiene_plan_b() {
        let attempts = chain(Mode::Auto);
        assert_eq!(attempts.len(), 3);
        assert_eq!(attempts[0].name, "wgpu (DX12/Vulkan)");
        // Si wgpu no pasa la prueba, la ventana se abre igual: con OpenGL.
        assert!(attempts.iter().any(|attempt| attempt.glow));
        // Y el último recurso es wgpu limitado a GLES.
        assert_eq!(attempts[2].backends, Some(wgpu::Backends::GL));
    }

    #[test]
    fn forzar_opengl_no_prueba_wgpu() {
        let attempts = chain(Mode::Gl);
        assert_eq!(attempts.len(), 1);
        assert!(attempts[0].glow);
    }

    #[test]
    fn wgpu_gl_cae_a_opengl() {
        let attempts = chain(Mode::WgpuGl);
        assert_eq!(attempts.len(), 2);
        assert!(!attempts[0].glow);
        assert!(attempts[1].glow);
    }
}