//! Paleta, tipografía y detalles visuales: tema oscuro con acento configurable.
//!
//! El acento es dinámico (`accent()`): vive en un atomic y se puede cambiar en
//! Ajustes al vuelo. La tipografía es Inter (regular + semibold, OFL), y el
//! logo es un bloque de hierba pintado a mano con el painter.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use egui::{
    pos2, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Rect,
    RichText, Sense, Shape, Stroke, TextStyle, Visuals,
};

pub const ACCENT: Color32 = Color32::from_rgb(0x3C, 0x85, 0x27);
/// Fondo general (panel central): casi negro con un leve tinte frío, estilo
/// "glass UI" (CMClient-like) en vez de un verde plano.
pub const BG: Color32 = Color32::from_rgb(0x0A, 0x0C, 0x0B);
/// Panel lateral y barra de estado: un escalón más claro que el fondo.
pub const SIDEBAR: Color32 = Color32::from_rgb(0x12, 0x15, 0x13);
/// Tarjetas.
pub const CARD: Color32 = Color32::from_rgb(0x17, 0x1B, 0x18);
/// Tarjeta elevada (hover / cabecera de detalle): más contraste para dar
/// sensación de "vidrio" iluminado por el acento.
pub const CARD_ELEVATED: Color32 = Color32::from_rgb(0x20, 0x27, 0x22);
/// Fondo de los campos de texto.
pub const INPUT: Color32 = Color32::from_rgb(0x08, 0x0A, 0x09);
pub const TEXT: Color32 = Color32::from_rgb(0xEE, 0xF3, 0xEE);
pub const MUTED: Color32 = Color32::from_rgb(0x9A, 0xA7, 0x9C);
pub const DANGER: Color32 = Color32::from_rgb(0xE5, 0x62, 0x5B);
/// Borde sutil de tarjetas y separadores.
pub const BORDER: Color32 = Color32::from_rgb(0x27, 0x2E, 0x29);

/// Radio de esquina estándar (tarjetas, botones, inputs). Más grande que antes
/// para un look más suave/moderno (estilo CMClient).
const RADIUS: f32 = 14.0;

/// Sombra suave y reutilizable: da profundidad a tarjetas y paneles flotantes sin
/// manchar el fondo. `egui` no trae sombras por defecto en los `Frame`.
pub fn shadow() -> egui::epaint::Shadow {
    egui::epaint::Shadow {
        offset: [0, 6],
        blur: 22,
        spread: 0,
        color: Color32::from_black_alpha(90),
    }
}

/// Sombra con resplandor del acento activo: para tarjetas destacadas (botón
/// "Jugar", instancia seleccionada) — el efecto "glow" característico de
/// launchers modernos tipo CMClient.
pub fn glow_shadow() -> egui::epaint::Shadow {
    let [r, g, b, _] = accent().to_array();
    egui::epaint::Shadow {
        offset: [0, 0],
        blur: 28,
        spread: 1,
        color: Color32::from_rgba_unmultiplied(r, g, b, 60),
    }
}

/// Separador horizontal de una línea, con el borde del tema.
pub fn separator(ui: &mut egui::Ui) {
    ui.add_space(6.0);
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0, BORDER);
    ui.add_space(6.0);
}

// ── Acento dinámico ──────────────────────────────────────────────────────────

/// Paletas de acento seleccionables en Ajustes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accent {
    Green,
    Blue,
    Violet,
    Rose,
    Amber,
}

pub const ACCENTS: [Accent; 5] = [
    Accent::Green,
    Accent::Blue,
    Accent::Violet,
    Accent::Rose,
    Accent::Amber,
];

impl Accent {
    pub fn name(self) -> &'static str {
        match self {
            Accent::Green => "Verde",
            Accent::Blue => "Azul",
            Accent::Violet => "Morado",
            Accent::Rose => "Rosa",
            Accent::Amber => "Ámbar",
        }
    }

    /// Clave que se guarda en config.json.
    pub fn key(self) -> &'static str {
        match self {
            Accent::Green => "green",
            Accent::Blue => "blue",
            Accent::Violet => "violet",
            Accent::Rose => "rose",
            Accent::Amber => "amber",
        }
    }

    pub fn from_key(key: &str) -> Self {
        match key {
            "blue" => Accent::Blue,
            "violet" => Accent::Violet,
            "rose" => Accent::Rose,
            "amber" => Accent::Amber,
            _ => Accent::Green,
        }
    }

    pub fn color(self) -> Color32 {
        match self {
            Accent::Green => Color32::from_rgb(0x3C, 0x85, 0x27),
            Accent::Blue => Color32::from_rgb(0x2F, 0x6F, 0xBF),
            Accent::Violet => Color32::from_rgb(0x7C, 0x4D, 0xBF),
            Accent::Rose => Color32::from_rgb(0xC2, 0x4A, 0x6E),
            Accent::Amber => Color32::from_rgb(0xBF, 0x8A, 0x2F),
        }
    }

    /// Tono claro del acento, para textos y detalles sobre fondos oscuros.
    pub fn soft(self) -> Color32 {
        match self {
            Accent::Green => Color32::from_rgb(0x6F, 0xC4, 0x5B),
            Accent::Blue => Color32::from_rgb(0x6F, 0xB1, 0xE8),
            Accent::Violet => Color32::from_rgb(0xB1, 0x8C, 0xE8),
            Accent::Rose => Color32::from_rgb(0xE8, 0x8C, 0xA8),
            Accent::Amber => Color32::from_rgb(0xE8, 0xC1, 0x6F),
        }
    }
}

static ACCENT_IDX: AtomicUsize = AtomicUsize::new(0);

/// Cambia el acento global (índice dentro de `ACCENTS`).
pub fn set_accent(accent: Accent) {
    if let Some(idx) = ACCENTS.iter().position(|a| *a == accent) {
        ACCENT_IDX.store(idx, Ordering::Relaxed);
    }
}

/// Acento activo. Los widgets lo consultan cada frame: cambiarlo es instantáneo.
pub fn accent() -> Color32 {
    ACCENTS[ACCENT_IDX.load(Ordering::Relaxed)].color()
}

/// Tono claro del acento activo.
pub fn accent_soft() -> Color32 {
    ACCENTS[ACCENT_IDX.load(Ordering::Relaxed)].soft()
}

// ── Tipografía ───────────────────────────────────────────────────────────────

/// Inter (OFL) como fuente principal, con la familia "SemiBold" para títulos.
pub fn set_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    for (name, bytes) in [
        (
            "inter_regular",
            &include_bytes!("../../assets/Inter-Regular.ttf")[..],
        ),
        (
            "inter_semibold",
            &include_bytes!("../../assets/Inter-SemiBold.ttf")[..],
        ),
    ] {
        fonts
            .font_data
            .insert(name.to_owned(), FontData::from_static(bytes).into());
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, name.to_owned());
    }
    // Familia auxiliar para títulos y botones principales.
    fonts.families.insert(
        FontFamily::Name("SemiBold".into()),
        vec!["inter_semibold".into()],
    );
    ctx.set_fonts(fonts);
}

/// Familia de los títulos (Inter SemiBold).
pub fn semibold() -> FontFamily {
    FontFamily::Name("SemiBold".into())
}

// ── Tema global ──────────────────────────────────────────────────────────────

/// Aplica paleta, tipografía y animaciones al contexto. Una vez al abrir.
pub fn apply(ctx: &egui::Context) {
    set_fonts(ctx);

    let mut style = (*ctx.style()).clone();

    style
        .text_styles
        .insert(TextStyle::Heading, FontId::new(22.0, semibold()));
    style
        .text_styles
        .insert(TextStyle::Body, FontId::new(15.0, FontFamily::Proportional));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::new(15.0, semibold()));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::new(12.0, FontFamily::Proportional));

    style.spacing.item_spacing = egui::vec2(12.0, 10.0);
    style.spacing.button_padding = egui::vec2(16.0, 8.0);
    style.spacing.menu_margin = egui::Margin::same(10);
    // Transiciones más vivas (estilo CMClient: hover/selección se notan al tacto).
    style.animation_time = 0.12;

    let mut visuals = Visuals::dark();
    visuals.panel_fill = BG;
    visuals.window_fill = CARD;
    visuals.extreme_bg_color = INPUT;
    visuals.selection.bg_fill = accent().gamma_multiply(0.45);
    visuals.selection.stroke = Stroke::new(1.0_f32, accent_soft());
    visuals.hyperlink_color = accent_soft();
    visuals.override_text_color = Some(TEXT);
    visuals.window_stroke = Stroke::new(1.0_f32, BORDER);
    // Widgets: botones/inputs redondeados, con borde tenue y hover claro.
    visuals.widgets.noninteractive.bg_fill = CARD;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    visuals.widgets.inactive.bg_fill = CARD;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    visuals.widgets.hovered.bg_fill = CARD_ELEVATED;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, accent());
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, TEXT);
    visuals.widgets.active.bg_fill = accent().gamma_multiply(0.35);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, accent_soft());
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, TEXT);
    // Inputs sobre INPUT, borde al enfocar.
    visuals.widgets.open.bg_fill = INPUT;
    // Esquinas redondeadas de todos los widgets (estilo global), antes de publicar.
    let radius = CornerRadius::same(RADIUS as u8);
    visuals.widgets.noninteractive.corner_radius = radius;
    visuals.widgets.inactive.corner_radius = radius;
    visuals.widgets.hovered.corner_radius = radius;
    visuals.widgets.active.corner_radius = radius;
    visuals.widgets.open.corner_radius = radius;

    style.visuals = visuals;
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_style(style);
}

/// Reaplica colores dependientes del acento sin tocar fuentes ni spacing.
/// Se llama al cambiar el acento en Ajustes.
pub fn reapply_accent(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let visuals = &mut style.visuals;
    visuals.selection.bg_fill = accent().gamma_multiply(0.45);
    visuals.selection.stroke = Stroke::new(1.0_f32, accent_soft());
    visuals.hyperlink_color = accent_soft();
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, accent());
    visuals.widgets.active.bg_fill = accent().gamma_multiply(0.35);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, accent_soft());
    ctx.set_style(style);
}

// ── Textos ───────────────────────────────────────────────────────────────────

/// Título grande de pantalla.
pub fn title(text: &str) -> RichText {
    RichText::new(text)
        .size(24.0)
        .family(semibold())
        .color(TEXT)
}

/// Título con acento (nombre del launcher, cabeceras de detalle).
pub fn title_accent(text: &str) -> RichText {
    RichText::new(text)
        .size(24.0)
        .family(semibold())
        .color(accent_soft())
}

/// Texto secundario.
pub fn muted(text: impl AsRef<str>) -> RichText {
    RichText::new(text.as_ref()).color(MUTED)
}

// ── Avatares y logo ──────────────────────────────────────────────────────────

/// Avatar del nick (cuentas offline): iniciales sobre un color derivado del
/// nombre. Determinista: el mismo nick siempre pinta igual.
pub fn avatar_fill(nick: &str) -> Color32 {
    let mut hash: u32 = 0x3C8527;
    for byte in nick.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(byte as u32);
    }
    let (r, g, b) = (hash >> 16, hash >> 8, hash);
    Color32::from_rgb(
        0x2C + (r & 0x30) as u8,
        0x50 + (g & 0x60) as u8,
        0x2C + (b & 0x30) as u8,
    )
}

/// Círculo con las iniciales del nick, pintado a mano (tamaño exacto, sin
/// depender del layout). `size` en px.
pub fn avatar(ui: &mut egui::Ui, nick: &str, size: f32) {
    let (initials, fill) = if nick.trim().is_empty() {
        ("?".to_owned(), Color32::from_rgb(0x30, 0x38, 0x30))
    } else {
        let mut parts = nick.trim().split_whitespace();
        let initials = match (parts.next(), parts.next()) {
            (Some(first), Some(second)) => format!(
                "{}{}",
                first.chars().next().unwrap_or('?'),
                second.chars().next().unwrap_or('?')
            ),
            _ => nick
                .trim()
                .chars()
                .take(2)
                .collect::<String>()
                .to_uppercase(),
        };
        (initials.to_uppercase(), avatar_fill(nick))
    };
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), Sense::hover());
    let center = rect.center();
    ui.painter().circle_filled(center, size / 2.0, fill);
    ui.painter().text(
        center,
        Align2::CENTER_CENTER,
        &initials,
        FontId::new(size * 0.38, semibold()),
        Color32::WHITE,
    );
}

// ── Bloques de Minecraft (pixel-art procedural) ──────────────────────────────

/// Aspecto de un bloque: cara superior (iluminada), frontal (media) y lateral
/// (en sombra), más motas opcionales (menas: diamante, redstone…).
#[derive(Clone, Copy)]
struct BlockStyle {
    name: &'static str,
    top: Color32,
    front: Color32,
    side: Color32,
    speck: Option<Color32>,
}

/// Bloques reconocibles de un vistazo, con los tonos de Minecraft.
const BLOCKS: [BlockStyle; 8] = [
    BlockStyle { name: "Césped", top: Color32::from_rgb(0x62, 0xA1, 0x3F), front: Color32::from_rgb(0x8B, 0x62, 0x3A), side: Color32::from_rgb(0x6E, 0x4C, 0x2C), speck: None },
    BlockStyle { name: "Diamante", top: Color32::from_rgb(0x8E, 0x99, 0xA0), front: Color32::from_rgb(0x7C, 0x86, 0x8C), side: Color32::from_rgb(0x60, 0x69, 0x6E), speck: Some(Color32::from_rgb(0x4A, 0xEB, 0xE0)) },
    BlockStyle { name: "Oro", top: Color32::from_rgb(0x9A, 0x9A, 0x9A), front: Color32::from_rgb(0x86, 0x86, 0x86), side: Color32::from_rgb(0x69, 0x69, 0x69), speck: Some(Color32::from_rgb(0xFC, 0xEE, 0x4B)) },
    BlockStyle { name: "Redstone", top: Color32::from_rgb(0x8E, 0x99, 0xA0), front: Color32::from_rgb(0x7C, 0x86, 0x8C), side: Color32::from_rgb(0x60, 0x69, 0x6E), speck: Some(Color32::from_rgb(0xE0, 0x2A, 0x1F)) },
    BlockStyle { name: "Esmeralda", top: Color32::from_rgb(0x8E, 0x99, 0xA0), front: Color32::from_rgb(0x7C, 0x86, 0x8C), side: Color32::from_rgb(0x60, 0x69, 0x6E), speck: Some(Color32::from_rgb(0x35, 0xC7, 0x4A)) },
    BlockStyle { name: "Lapislázuli", top: Color32::from_rgb(0x8E, 0x99, 0xA0), front: Color32::from_rgb(0x7C, 0x86, 0x8C), side: Color32::from_rgb(0x60, 0x69, 0x6E), speck: Some(Color32::from_rgb(0x2C, 0x5A, 0xC8)) },
    BlockStyle { name: "Tierra", top: Color32::from_rgb(0x9A, 0x6C, 0x40), front: Color32::from_rgb(0x8B, 0x62, 0x3A), side: Color32::from_rgb(0x6E, 0x4C, 0x2C), speck: None },
    BlockStyle { name: "Piedra", top: Color32::from_rgb(0xA0, 0xA0, 0xA0), front: Color32::from_rgb(0x8C, 0x8C, 0x8C), side: Color32::from_rgb(0x70, 0x70, 0x70), speck: None },
];

/// Hash estable de un texto (mismo seed → mismo bloque, siempre).
fn seed_of(text: &str) -> u32 {
    let mut hash: u32 = 0x9E37_79B9;
    for byte in text.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(byte as u32);
    }
    hash
}

/// Bloque asignado a un texto (determinista) y su nombre legible.
pub fn block_for(seed: &str) -> &'static str {
    BLOCKS[(seed_of(seed) as usize) % BLOCKS.len()].name
}

/// Cubo isométrico con sombreado plano y motas: el «icono de Minecraft» que usa
/// el lateral y las cabeceras. Se dibuja a mano (sin red ni ficheros) para que
/// funcione siempre, incluso recién instalado y sin Internet.
pub fn block_icon(ui: &mut egui::Ui, seed: &str, size: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), Sense::hover());
    draw_block(ui, rect, &BLOCKS[(seed_of(seed) as usize) % BLOCKS.len()]);
    response.on_hover_text(format!("Bloque «{}»", block_for(seed)))
}

fn draw_block(ui: &egui::Ui, rect: Rect, style: &BlockStyle) {
    let p = ui.painter();
    let cx = rect.center().x;
    let dy = rect.height() * 0.22;
    let (top, bottom) = (rect.top(), rect.bottom());

    // Cara superior (rombo), iluminada.
    p.add(Shape::convex_polygon(
        vec![
            pos2(cx, top),
            pos2(rect.right(), top + dy),
            pos2(cx, top + dy * 2.0),
            pos2(rect.left(), top + dy),
        ],
        style.top,
        Stroke::NONE,
    ));
    // Cara izquierda (en sombra media).
    p.add(Shape::convex_polygon(
        vec![
            pos2(rect.left(), top + dy),
            pos2(cx, top + dy * 2.0),
            pos2(cx, bottom),
            pos2(rect.left(), bottom - dy),
        ],
        style.front,
        Stroke::NONE,
    ));
    // Cara derecha (la más oscura: la luz viene de la izquierda).
    p.add(Shape::convex_polygon(
        vec![
            pos2(cx, top + dy * 2.0),
            pos2(rect.right(), top + dy),
            pos2(rect.right(), bottom - dy),
            pos2(cx, bottom),
        ],
        style.side,
        Stroke::NONE,
    ));

    // Motas de mena: posiciones derivadas del propio rect (nada aleatorio, para
    // que el icono no «parpadee» al repintar).
    if let Some(speck) = style.speck {
        let s = (rect.width() * 0.10).max(2.0);
        let points = [
            (rect.left() + rect.width() * 0.22, top + dy * 2.1),
            (rect.left() + rect.width() * 0.42, bottom - dy * 1.4),
            (cx + rect.width() * 0.16, top + dy * 2.4),
            (cx + rect.width() * 0.30, bottom - dy * 0.9),
        ];
        for (x, y) in points {
            p.rect_filled(
                Rect::from_min_size(pos2(x, y), egui::vec2(s, s)),
                CornerRadius::same(1),
                speck,
            );
        }
    }
}

/// Bloque de hierba del logo (césped + tierra) en 3D: la cara de siempre del
/// launcher, pero con volumen.
pub fn grass_block(ui: &mut egui::Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), Sense::hover());
    draw_block(ui, rect, &BLOCKS[0]);
}

// ── Fichas de datos ──────────────────────────────────────────────────────────

/// Ficha compacta «etiqueta / valor» para las cabeceras (RAM, versión, ruta…).
pub fn chip(ui: &mut egui::Ui, label: &str, value: &str) {
    egui::Frame::new()
        .fill(INPUT)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(label).size(10.5).color(MUTED));
                ui.label(RichText::new(value).size(13.0).family(semibold()).color(TEXT));
            });
        });
}

// ── Contenedores y botones ───────────────────────────────────────────────────

/// Tarjeta estándar: fondo CARD, borde sutil, sombra suave y esquinas redondeadas.
pub fn card(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(CornerRadius::same(RADIUS as u8))
        .shadow(shadow())
        .inner_margin(egui::Margin::same(14))
        .show(ui, body);
}

/// Botón principal relleno con el acento (JUGAR, Crear, Guardar…).
pub fn primary_button(ui: &mut egui::Ui, text: &str, min_size: egui::Vec2) -> egui::Response {
    ui.add(
        egui::Button::new(
            RichText::new(text)
                .family(semibold())
                .color(Color32::WHITE),
        )
        .fill(accent())
        .corner_radius(CornerRadius::same(RADIUS as u8))
        .min_size(min_size),
    )
}

/// Cabecera de pantalla: título grande + subtítulo.
pub fn screen_header(ui: &mut egui::Ui, title_text: &str, subtitle: &str) {
    ui.add_space(4.0);
    ui.label(title(title_text));
    if !subtitle.is_empty() {
        ui.label(muted(subtitle));
    }
    ui.add_space(6.0);
}

/// Tarjeta con rótulo de sección dentro (el patrón de Ajustes, para toda la app).
pub fn card_section(ui: &mut egui::Ui, title_text: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(CornerRadius::same(RADIUS as u8))
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.label(
                RichText::new(title_text)
                    .small()
                    .strong()
                    .color(MUTED),
            );
            ui.add_space(6.0);
            body(ui);
        });
    ui.add_space(10.0);
}

/// Antirrebote del acordeón: la última cabecera pulsada (puntero del texto
/// estático) y el instante (ms). Dos clics del MISMO botón en <200 ms cuentan
/// como uno: algunos ratones/touchpads de Windows emiten doble evento.
static LAST_TOGGLE_KEY: AtomicU64 = AtomicU64::new(0);
static LAST_TOGGLE_MS: AtomicU64 = AtomicU64::new(0);

/// Sección plegable tipo acordeón: la cabecera (siempre visible) abre o cierra
/// el contenido con un clic. Devuelve true SOLO en el frame del clic (para
/// que el caller escriba su estado después del cierre). El estado abierto se
/// toma del parámetro `open`, no del retorno.
pub fn section_toggle(
    ui: &mut egui::Ui,
    open: bool,
    title_text: &str,
    hint: &str,
    body: impl FnOnce(&mut egui::Ui),
) -> bool {
    let mut new_open = open;

    // Cabecera como un ÚNICO widget clicable: se reserva el rect con sense
    // de clic y se pinta a mano. (Un Frame + interact extra registraba dos
    // widgets con el mismo id y egui disparaba el clic dos veces: la sección
    // se abría y cerraba al instante.)
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 26.0),
        Sense::click(),
    );
    let (bg, fg, icon_color) = if response.hovered() {
        (CARD_ELEVATED, TEXT, TEXT)
    } else {
        (CARD, MUTED, MUTED)
    };
    let p = ui.painter_at(rect);
    p.rect_filled(rect, CornerRadius::same(RADIUS as u8), bg);
    p.rect_stroke(
        rect,
        CornerRadius::same(RADIUS as u8),
        Stroke::new(1.0_f32, BORDER),
        egui::StrokeKind::Inside,
    );
    // Chevron en vez de +/-: se lee como «hay más aquí dentro» de un vistazo.
    let icon = if open { "▾" } else { "▸" };
    p.text(
        [rect.left() + 12.0, rect.center().y].into(),
        Align2::LEFT_CENTER,
        icon,
        FontId::proportional(13.0),
        icon_color,
    );
    let title_x = rect.left() + 12.0 + 8.0 + 10.0;
    p.text(
        [title_x, rect.center().y].into(),
        Align2::LEFT_CENTER,
        title_text,
        FontId::proportional(12.0),
        fg,
    );
    if !hint.is_empty() {
        p.text(
            [rect.right() - 12.0, rect.center().y].into(),
            Align2::RIGHT_CENTER,
            hint,
            FontId::proportional(12.0),
            icon_color,
        );
    }
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.clicked() {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let key = title_text.as_ptr() as u64; // los títulos son literales 'static
        let same_key = LAST_TOGGLE_KEY.load(Ordering::Relaxed) == key;
        let last_ms = LAST_TOGGLE_MS.load(Ordering::Relaxed);
        // Clic aceptado: otra sección, o la misma después del rebote.
        if !same_key || now_ms.saturating_sub(last_ms) > 200 {
            LAST_TOGGLE_KEY.store(key, Ordering::Relaxed);
            LAST_TOGGLE_MS.store(now_ms, Ordering::Relaxed);
            new_open = !open;
        }
    }
    ui.add_space(4.0);
    if new_open {
        egui::Frame::new()
            .fill(CARD)
            .stroke(Stroke::new(1.0_f32, BORDER))
            .corner_radius(CornerRadius::same(RADIUS as u8))
            .inner_margin(egui::Margin::same(14))
            .show(ui, body);
        ui.add_space(6.0);
    }
    ui.add_space(2.0);
    // Importante: devolver "hubo clic", NO el estado. Devolver el estado hacía
    // que el caller volviera a escribir `None` en el frame siguiente y la
    // sección se cerrara sola (~50 ms después de abrir).
    let clicked = new_open != open;
    clicked
}

/// Fila de formulario: etiqueta a ancho fijo + control alineado a la derecha
/// de la etiqueta. Da la verticalidad que los formularios sueltos no tienen.
pub fn form_row(ui: &mut egui::Ui, label: &str, control: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        let label_width = 150.0;
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(label_width, ui.spacing().interact_size.y),
            Sense::hover(),
        );
        ui.painter().text(
            [rect.left(), rect.center().y].into(),
            Align2::LEFT_CENTER,
            label,
            FontId::new(14.0, FontFamily::Proportional),
            TEXT,
        );
        control(ui);
    });
}

/// Botón secundario: mismo cuerpo que CARD_ELEVATED, borde visible.
pub fn ghost_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).color(TEXT))
            .fill(CARD_ELEVATED)
            .stroke(Stroke::new(1.0_f32, BORDER))
            .corner_radius(CornerRadius::same(RADIUS as u8)),
    )
}

/// Fila de tarjeta con acento vertical cuando está seleccionada.
pub fn selected_card(
    ui: &mut egui::Ui,
    selected: bool,
    body: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    let (fill, stroke) = if selected {
        (
            accent().gamma_multiply(0.18),
            Stroke::new(1.5_f32, accent()),
        )
    } else {
        (CARD, Stroke::new(1.0_f32, BORDER))
    };
    egui::Frame::new()
        .fill(fill)
        .stroke(stroke)
        .corner_radius(CornerRadius::same(RADIUS as u8))
        .inner_margin(egui::Margin::same(12))
        .show(ui, body)
        .response
}
// ── Tarjetas "vidrio" con resplandor (estilo CMClient) ───────────────────────

/// Dibuja un `Frame` tipo tarjeta de vidrio: fondo elevado, borde sutil del
/// acento y sombra con resplandor. Pensado para la tarjeta de instancia
/// seleccionada o el panel de "Jugar".
pub fn glass_card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(CARD_ELEVATED)
        .stroke(Stroke::new(1.0_f32, accent().gamma_multiply(0.55)))
        .corner_radius(CornerRadius::same(RADIUS as u8))
        .shadow(glow_shadow())
        .inner_margin(egui::Margin::same(14))
        .show(ui, add_contents);
}

/// Tarjeta plana estándar (sin resplandor), para listas e instancias no
/// seleccionadas: mismo radio y tipografía que `glass_card`, sombra normal.
pub fn flat_card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(CornerRadius::same(RADIUS as u8))
        .shadow(shadow())
        .inner_margin(egui::Margin::same(12))
        .show(ui, add_contents);
}
