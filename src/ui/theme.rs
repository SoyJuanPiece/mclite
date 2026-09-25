//! Paleta, tipografía y detalles visuales: tema oscuro con acento configurable.
//!
//! El acento es dinámico (`accent()`): vive en un atomic y se puede cambiar en
//! Ajustes al vuelo. La tipografía es Inter (regular + semibold, OFL), y el
//! logo es un bloque de hierba pintado a mano con el painter.

use std::sync::atomic::{AtomicUsize, Ordering};

use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Rect, RichText, Sense,
    Stroke, TextStyle, Visuals,
};

pub const ACCENT: Color32 = Color32::from_rgb(0x3C, 0x85, 0x27);
/// Fondo general (panel central).
pub const BG: Color32 = Color32::from_rgb(0x0E, 0x11, 0x0E);
/// Panel lateral y barra de estado.
pub const SIDEBAR: Color32 = Color32::from_rgb(0x15, 0x19, 0x15);
/// Tarjetas.
pub const CARD: Color32 = Color32::from_rgb(0x1B, 0x20, 0x1B);
/// Tarjeta elevada (hover / cabecera de detalle).
pub const CARD_ELEVATED: Color32 = Color32::from_rgb(0x22, 0x28, 0x22);
/// Fondo de los campos de texto.
pub const INPUT: Color32 = Color32::from_rgb(0x0A, 0x0D, 0x0A);
pub const TEXT: Color32 = Color32::from_rgb(0xE9, 0xEF, 0xE9);
pub const MUTED: Color32 = Color32::from_rgb(0x93, 0xA1, 0x93);
pub const DANGER: Color32 = Color32::from_rgb(0xE0, 0x5D, 0x56);
/// Borde sutil de tarjetas y separadores.
pub const BORDER: Color32 = Color32::from_rgb(0x2A, 0x30, 0x2A);

/// Radio de esquina estándar (tarjetas, botones, inputs).
const RADIUS: f32 = 8.0;

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

    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(14.0, 6.0);
    style.spacing.menu_margin = egui::Margin::same(8);
    // Transiciones suaves de hover/selección en toda la app.
    style.animation_time = 0.16;

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

/// Círculo con las iniciales del nick. `size` en px.
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
    egui::Frame::new()
        .fill(fill)
        .corner_radius(egui::CornerRadius::same(size as u8 / 2))
        .show(ui, |ui| {
            ui.set_min_size(egui::vec2(size, size));
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new(&initials)
                        .size(size * 0.42)
                        .family(semibold())
                        .color(Color32::WHITE),
                );
            });
        });
}

/// Logo: bloque de hierba de Minecraft pintado a mano (tierra + césped con
/// borde irregular). `size` en px, se dibuja cuadrado.
pub fn grass_block(ui: &mut egui::Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), Sense::hover());
    let p = ui.painter();
    let px = size / 16.0;

    let dirt_top = Color32::from_rgb(0x8A, 0x60, 0x38);
    let dirt_bottom = Color32::from_rgb(0x6B, 0x47, 0x2A);
    let grass = Color32::from_rgb(0x5D, 0x9C, 0x3F);

    // Tierra en dos bandas para dar algo de profundidad.
    let mid = rect.top() + size * 0.55;
    p.rect_filled(
        Rect::from_min_max(rect.left_top(), [rect.right(), mid].into()),
        0.0,
        dirt_top,
    );
    p.rect_filled(
        Rect::from_min_max([rect.left(), mid].into(), rect.right_bottom()),
        0.0,
        dirt_bottom,
    );
    // Capa de césped.
    let grass_h = 4.0 * px;
    p.rect_filled(
        Rect::from_min_max(
            rect.left_top(),
            [rect.right(), rect.top() + grass_h].into(),
        ),
        0.0,
        grass,
    );
    // Borde irregular: dientes de hierba alternados.
    let tooth = 2.0 * px;
    let mut x = rect.left();
    let mut down = true;
    while x < rect.right() {
        let w = tooth.min(rect.right() - x);
        let h = if down { grass_h + 2.5 * px } else { grass_h };
        p.rect_filled(
            Rect::from_min_max(
                [x, rect.top()].into(),
                [x + w, rect.top() + h].into(),
            ),
            0.0,
            grass,
        );
        x += w;
        down = !down;
    }
    // Marco sutil.
    p.rect_stroke(
        rect,
        CornerRadius::same((size * 0.18) as u8),
        Stroke::new(1.0_f32, BORDER),
        egui::StrokeKind::Inside,
    );
}

// ── Contenedores y botones ───────────────────────────────────────────────────

/// Tarjeta estándar: fondo CARD, borde sutil y esquinas redondeadas.
pub fn card(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(CornerRadius::same(RADIUS as u8))
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
