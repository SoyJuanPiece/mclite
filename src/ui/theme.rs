//! Paleta y tipografía: tema oscuro con acento verde Minecraft (`#3C8527`).
//!
//! Objetivos de la pasada visual: jerarquía clara (fondo → panel → tarjeta),
//! esquinas suaves, botones con acento solo en la acción principal, hover
//! perceptible y bordes apenas insinuados para separar sin ensuciar.

use egui::{Color32, CornerRadius, FontFamily, FontId, RichText, Stroke, TextStyle, Visuals};

pub const ACCENT: Color32 = Color32::from_rgb(0x3C, 0x85, 0x27);
/// Verde claro para textos sobre el acento y detalles positivos.
pub const ACCENT_SOFT: Color32 = Color32::from_rgb(0x6F, 0xC4, 0x5B);
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

/// Avatar del nick (cuentas offline): iniciales sobre un verde derivado del
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
            (Some(first), Some(second)) => {
                format!("{}{}", first.chars().next().unwrap_or('?'), second.chars().next().unwrap_or('?'))
            }
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
                        .strong()
                        .color(Color32::WHITE),
                );
            });
        });
}

/// Aplica la paleta y la tipografía al contexto. Se llama una vez al abrir la ventana.
pub fn apply(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();

    style
        .text_styles
        .insert(TextStyle::Heading, FontId::new(22.0, FontFamily::Proportional));
    style
        .text_styles
        .insert(TextStyle::Body, FontId::new(15.0, FontFamily::Proportional));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::new(15.0, FontFamily::Proportional));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::new(12.0, FontFamily::Proportional));

    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(14.0, 6.0);
    style.spacing.menu_margin = egui::Margin::same(8);

    let mut visuals = Visuals::dark();
    visuals.panel_fill = BG;
    visuals.window_fill = CARD;
    visuals.extreme_bg_color = INPUT;
    visuals.selection.bg_fill = ACCENT.gamma_multiply(0.45);
    visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT_SOFT);
    visuals.hyperlink_color = ACCENT_SOFT;
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
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, TEXT);
    visuals.widgets.active.bg_fill = ACCENT.gamma_multiply(0.35);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT_SOFT);
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

/// Título grande de pantalla.
pub fn title(text: &str) -> RichText {
    RichText::new(text).size(24.0).strong().color(TEXT)
}

/// Título con acento (nombre del launcher, cabeceras de detalle).
pub fn title_accent(text: &str) -> RichText {
    RichText::new(text).size(24.0).strong().color(ACCENT_SOFT)
}

/// Texto secundario.
pub fn muted(text: impl AsRef<str>) -> RichText {
    RichText::new(text.as_ref()).color(MUTED)
}

/// Tarjeta estándar: fondo CARD, borde sutil y esquinas redondeadas.
pub fn card(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(CornerRadius::same(RADIUS as u8))
        .inner_margin(egui::Margin::same(14))
        .show(ui, body);
}

/// Cabecera de pantalla: título grande + subtítulo a la derecha de la columna.
pub fn screen_header(ui: &mut egui::Ui, title_text: &str, subtitle: &str) {
    ui.add_space(4.0);
    ui.label(title(title_text));
    if !subtitle.is_empty() {
        ui.label(muted(subtitle));
    }
    ui.add_space(6.0);
}

/// Botón principal relleno con el acento (JUGAR, Crear, Guardar…).
pub fn primary_button(ui: &mut egui::Ui, text: &str, min_size: egui::Vec2) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).strong())
            .fill(ACCENT)
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
            ACCENT.gamma_multiply(0.18),
            Stroke::new(1.5_f32, ACCENT),
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
