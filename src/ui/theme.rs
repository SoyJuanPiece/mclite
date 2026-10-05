//! Paleta, tipografía y detalles visuales: TEMA PREMIUM CMCLIENT EDITION
//!
//! Rediseño completo al estilo de launchers premium modernos

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use egui::{pos2, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Rect, RichText, Sense, Stroke, TextStyle, Visuals};

// ═══════════════════════════════════════════════════════════════════════════════
// PALETA PREMIUM CMCLIENT
// ═══════════════════════════════════════════════════════════════════════════════

pub const ACCENT: Color32 = Color32::from_rgb(0x3C, 0x85, 0x27);
pub const ACCENT_GLOW: Color32 = Color32::from_rgb(0x6F, 0xC4, 0x5B);
pub const BG: Color32 = Color32::from_rgb(0x08, 0x0A, 0x09);
pub const CARD: Color32 = Color32::from_rgb(0x14, 0x18, 0x15);
pub const CARD_ELEVATED: Color32 = Color32::from_rgb(0x1E, 0x24, 0x1F);
pub const CARD_PREMIUM: Color32 = Color32::from_rgb(0x25, 0x32, 0x26);
pub const INPUT: Color32 = Color32::from_rgb(0x0A, 0x0D, 0x0B);
pub const TEXT: Color32 = Color32::from_rgb(0xF0, 0xF5, 0xF0);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x9A, 0xA8, 0x9C);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x6A, 0x75, 0x6A);
pub const DANGER: Color32 = Color32::from_rgb(0xE8, 0x5A, 0x52);
pub const BORDER: Color32 = Color32::from_rgb(0x22, 0x28, 0x24);

pub const RADIUS: f32 = 16.0;
pub const RADIUS_SM: f32 = 10.0;
pub const RADIUS_LG: f32 = 20.0;

pub fn shadow() -> egui::epaint::Shadow {
    egui::epaint::Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(80) }
}

pub fn shadow_elevated() -> egui::epaint::Shadow {
    egui::epaint::Shadow { offset: [0, 12], blur: 32, spread: 0, color: Color32::from_black_alpha(100) }
}

pub fn glow_shadow() -> egui::epaint::Shadow {
    let [r, g, b, _] = ACCENT.to_array();
    egui::epaint::Shadow { offset: [0, 0], blur: 32, spread: 2, color: Color32::from_rgba_unmultiplied(r, g, b, 80) }
}

pub fn glow_shadow_intense() -> egui::epaint::Shadow {
    let [r, g, b, _] = ACCENT_GLOW.to_array();
    egui::epaint::Shadow { offset: [0, 4], blur: 48, spread: 4, color: Color32::from_rgba_unmultiplied(r, g, b, 120) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accent { Green, Blue, Violet, Rose, Amber }

pub const ACCENTS: [Accent; 5] = [Accent::Green, Accent::Blue, Accent::Violet, Accent::Rose, Accent::Amber];

impl Accent {
    pub fn name(self) -> &'static str {
        match self { Accent::Green => "Verde", Accent::Blue => "Azul", Accent::Violet => "Violeta", Accent::Rose => "Rosa", Accent::Amber => "Ámbar" }
    }
    pub fn key(self) -> &'static str {
        match self { Accent::Green => "green", Accent::Blue => "blue", Accent::Violet => "violet", Accent::Rose => "rose", Accent::Amber => "amber" }
    }
    pub fn from_key(key: &str) -> Self {
        match key { "blue" => Accent::Blue, "violet" => Accent::Violet, "rose" => Accent::Rose, "amber" => Accent::Amber, _ => Accent::Green }
    }
    pub fn color(self) -> Color32 {
        match self { Accent::Green => Color32::from_rgb(0x3C, 0x85, 0x27), Accent::Blue => Color32::from_rgb(0x2F, 0x7F, 0xBF), Accent::Violet => Color32::from_rgb(0x8A, 0x4F, 0xC2), Accent::Rose => Color32::from_rgb(0xC2, 0x4A, 0x6E), Accent::Amber => Color32::from_rgb(0xBF, 0x8A, 0x2F) }
    }
    pub fn glow(self) -> Color32 {
        match self { Accent::Green => Color32::from_rgb(0x6F, 0xC4, 0x5B), Accent::Blue => Color32::from_rgb(0x6F, 0xB8, 0xE8), Accent::Violet => Color32::from_rgb(0xB8, 0x8C, 0xE8), Accent::Rose => Color32::from_rgb(0xE8, 0x8C, 0xA8), Accent::Amber => Color32::from_rgb(0xE8, 0xC4, 0x6F) }
    }
}

static ACCENT_IDX: AtomicUsize = AtomicUsize::new(0);

pub fn set_accent(accent: Accent) {
    if let Some(idx) = ACCENTS.iter().position(|a| *a == accent) { ACCENT_IDX.store(idx, Ordering::Relaxed); }
}

pub fn accent() -> Color32 { ACCENTS[ACCENT_IDX.load(Ordering::Relaxed)].color() }
pub fn accent_glow() -> Color32 { ACCENTS[ACCENT_IDX.load(Ordering::Relaxed)].glow() }

pub fn set_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    for (name, bytes) in [
        ("inter_regular", &include_bytes!("../../assets/Inter-Regular.ttf")[..]),
        ("inter_medium", &include_bytes!("../../assets/Inter-Medium.ttf")[..]),
        ("inter_semibold", &include_bytes!("../../assets/Inter-SemiBold.ttf")[..]),
    ] {
        fonts.font_data.insert(name.to_owned(), FontData::from_static(bytes).into());
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, name.to_owned());
    }
    fonts.families.insert(FontFamily::Name("Medium".into()), vec!["inter_medium".into()]);
    fonts.families.insert(FontFamily::Name("SemiBold".into()), vec!["inter_semibold".into()]);
    ctx.set_fonts(fonts);
}

pub fn font_medium() -> FontFamily { FontFamily::Name("Medium".into()) }
pub fn font_semibold() -> FontFamily { FontFamily::Name("SemiBold".into()) }

pub fn apply(ctx: &egui::Context) {
    set_fonts(ctx);
    let mut style = (*ctx.style()).clone();
    style.text_styles.insert(TextStyle::Heading, FontId::new(26.0, font_semibold()));
    style.text_styles.insert(TextStyle::Body, FontId::new(15.0, FontFamily::Proportional));
    style.text_styles.insert(TextStyle::Button, FontId::new(15.0, font_medium()));
    style.text_styles.insert(TextStyle::Small, FontId::new(12.5, FontFamily::Proportional));
    style.spacing.item_spacing = egui::vec2(14.0, 12.0);
    style.spacing.button_padding = egui::vec2(18.0, 10.0);
    style.spacing.menu_margin = egui::Margin::same(12);
    style.animation_time = 0.12;

    let mut visuals = Visuals::dark();
    visuals.panel_fill = BG;
    visuals.window_fill = CARD;
    visuals.extreme_bg_color = INPUT;
    visuals.selection.bg_fill = accent().gamma_multiply(0.35);
    visuals.selection.stroke = Stroke::new(1.5, accent_glow());
    visuals.hyperlink_color = accent_glow();
    visuals.override_text_color = Some(TEXT);
    visuals.window_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.noninteractive.bg_fill = CARD;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.inactive.bg_fill = CARD;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.hovered.bg_fill = CARD_ELEVATED;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.5, accent());
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.active.bg_fill = accent().gamma_multiply(0.4);
    visuals.widgets.active.bg_stroke = Stroke::new(2.0, accent_glow());
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.open.bg_fill = INPUT;

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

pub fn reapply_accent(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let visuals = &mut style.visuals;
    visuals.selection.bg_fill = accent().gamma_multiply(0.35);
    visuals.selection.stroke = Stroke::new(1.5, accent_glow());
    visuals.hyperlink_color = accent_glow();
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.5, accent());
    visuals.widgets.active.bg_fill = accent().gamma_multiply(0.4);
    visuals.widgets.active.bg_stroke = Stroke::new(2.0, accent_glow());
    ctx.set_style(style);
}

pub fn glass_card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new().fill(CARD).stroke(Stroke::new(1.0, BORDER)).corner_radius(CornerRadius::same(RADIUS as u8)).shadow(shadow()).inner_margin(egui::Margin::same(16)).show(ui, add_contents);
}

pub fn glass_card_elevated(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new().fill(CARD_ELEVATED).stroke(Stroke::new(1.5, accent().gamma_multiply(0.5))).corner_radius(CornerRadius::same(RADIUS as u8)).shadow(shadow_elevated()).inner_margin(egui::Margin::same(16)).show(ui, add_contents);
}

pub fn glass_card_premium(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new().fill(CARD_PREMIUM).stroke(Stroke::new(2.0, accent())).corner_radius(CornerRadius::same(RADIUS_LG as u8)).shadow(glow_shadow()).inner_margin(egui::Margin::same(20)).show(ui, add_contents);
}

pub fn glow_button(ui: &mut egui::Ui, text: &str, min_size: egui::Vec2) -> egui::Response {
    let button = egui::Button::new(RichText::new(text).family(font_semibold()).size(18.0).color(Color32::WHITE)).fill(accent()).corner_radius(CornerRadius::same(RADIUS as u8)).min_size(min_size);
    egui::Frame::new().shadow(glow_shadow_intense()).show(ui, |ui| ui.add(button)).inner
}

pub fn ghost_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(text).color(TEXT)).fill(Color32::TRANSPARENT).stroke(Stroke::new(1.0, BORDER)).corner_radius(CornerRadius::same(RADIUS_SM as u8)))
}

pub fn primary_button(ui: &mut egui::Ui, text: &str, min_size: egui::Vec2) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(text).family(font_medium()).color(Color32::WHITE)).fill(accent()).corner_radius(CornerRadius::same(RADIUS as u8)).min_size(min_size))
}

pub fn title(text: &str) -> RichText { RichText::new(text).size(26.0).family(font_semibold()).color(TEXT) }
pub fn title_accent(text: &str) -> RichText { RichText::new(text).size(26.0).family(font_semibold()).color(accent_glow()) }
pub fn subtitle(text: &str) -> RichText { RichText::new(text).size(18.0).family(font_medium()).color(TEXT) }
pub fn muted(text: impl AsRef<str>) -> RichText { RichText::new(text.as_ref()).size(14.0).color(TEXT_MUTED) }
pub fn dim(text: impl AsRef<str>) -> RichText { RichText::new(text.as_ref()).size(12.5).color(TEXT_DIM) }

pub fn separator(ui: &mut egui::Ui) {
    ui.add_space(8.0);
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0, BORDER);
    ui.add_space(8.0);
}

pub fn accent_line(ui: &mut egui::Ui, width: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 3.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(2), accent());
}

pub fn avatar_fill(nick: &str) -> Color32 {
    let mut hash: u32 = 0x3C8527;
    for byte in nick.bytes() { hash = hash.wrapping_mul(31).wrapping_add(byte as u32); }
    let (r, g, b) = (hash >> 16, hash >> 8, hash);
    Color32::from_rgb(0x2C + (r & 0x30) as u8, 0x50 + (g & 0x60) as u8, 0x2C + (b & 0x30) as u8)
}

pub fn avatar(ui: &mut egui::Ui, nick: &str, size: f32) {
    let (initials, fill) = if nick.trim().is_empty() { ("?".to_owned(), Color32::from_rgb(0x30, 0x38, 0x30)) } else {
        let mut parts = nick.trim().split_whitespace();
        let initials = match (parts.next(), parts.next()) {
            (Some(first), Some(second)) => format!("{}{}", first.chars().next().unwrap_or('?'), second.chars().next().unwrap_or('?')),
            _ => nick.trim().chars().take(2).collect::<String>().to_uppercase(),
        };
        (initials.to_uppercase(), avatar_fill(nick))
    };
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), Sense::hover());
    let center = rect.center();
    ui.painter().circle_filled(center, size / 2.0, fill);
    ui.painter().text(center, Align2::CENTER_CENTER, &initials, FontId::new(size * 0.38, font_semibold()), Color32::WHITE);
}

pub fn block_icon(ui: &mut egui::Ui, seed: &str, size: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), Sense::hover());
    let mut hash: u32 = 0;
    for b in seed.bytes() { hash = hash.wrapping_mul(31).wrapping_add(b as u32); }
    let base_color = Color32::from_rgb(0x30 + ((hash >> 16) & 0x3F) as u8, 0x50 + ((hash >> 8) & 0x4F) as u8, 0x30 + (hash & 0x3F) as u8);
    let highlight = Color32::from_rgb((base_color.r() as u16 + 40).min(255) as u8, (base_color.g() as u16 + 40).min(255) as u8, (base_color.b() as u16 + 40).min(255) as u8);
    let shadow = Color32::from_rgb(base_color.r().saturating_sub(30), base_color.g().saturating_sub(30), base_color.b().saturating_sub(30));
    let p = ui.painter_at(rect);
    let r = CornerRadius::same((size * 0.15) as u8);
    p.rect_filled(rect, r, base_color);
    p.rect_filled(egui::Rect::from_min_size(rect.min, egui::vec2(size * 0.6, size * 0.4)), r, highlight);
    p.rect_filled(egui::Rect::from_min_size([rect.max.x - size * 0.4, rect.max.y - size * 0.6].into(), egui::vec2(size * 0.4, size * 0.6)), r, shadow);
    p.rect_stroke(rect, r, Stroke::new(2.0, BORDER));
    response
}

pub fn grass_block(ui: &mut egui::Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), Sense::hover());
    let p = ui.painter_at(rect);
    let r = CornerRadius::same((size * 0.12) as u8);
    let dirt = Color32::from_rgb(0x5B, 0x3A, 0x22);
    let dirt_dark = Color32::from_rgb(0x47, 0x2C, 0x18);
    let grass = Color32::from_rgb(0x3C, 0x85, 0x27);
    let grass_light = Color32::from_rgb(0x6F, 0xC4, 0x5B);
    p.rect_filled(rect, r, dirt);
    let grass_rect = egui::Rect::from_min_max(rect.min, [rect.max.x, rect.min.y + size * 0.6].into());
    p.rect_filled(grass_rect, r, grass);
    p.rect_filled(egui::Rect::from_min_max(grass_rect.min, [grass_rect.min.x + size * 0.5, grass_rect.min.y + size * 0.25].into()), r, grass_light);
    for i in 0..3 {
        let x = rect.min.x + size * (0.2 + i as f32 * 0.25);
        let y = rect.min.y + size * (0.7 + (i % 2) as f32 * 0.15);
        p.rect_filled(egui::Rect::from_min_size([x, y].into(), egui::vec2(size * 0.08, size * 0.08)), CornerRadius::same(2), dirt_dark);
    }
    p.rect_stroke(rect, r, Stroke::new(2.0, Color32::from_rgb(0x1A, 0x1A, 0x1A)));
}

pub fn screen_header(ui: &mut egui::Ui, title_text: &str, subtitle_text: &str) {
    ui.add_space(8.0);
    ui.label(title(title_text));
    if !subtitle_text.is_empty() { ui.label(muted(subtitle_text)); }
    accent_line(ui, 40.0);
    ui.add_space(8.0);
}

pub fn card_section(ui: &mut egui::Ui, title_text: &str, body: impl FnOnce(&mut egui::Ui)) {
    glass_card(ui, |ui| {
        ui.label(RichText::new(title_text).small().family(font_medium()).color(TEXT_MUTED));
        ui.add_space(8.0);
        body(ui);
    });
    ui.add_space(12.0);
}

static LAST_TOGGLE_KEY: AtomicU64 = AtomicU64::new(0);
static LAST_TOGGLE_MS: AtomicU64 = AtomicU64::new(0);

pub fn section_toggle(ui: &mut egui::Ui, open: bool, title_text: &str, hint: &str, body: impl FnOnce(&mut egui::Ui)) -> bool {
    let mut new_open = open;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 32.0), Sense::click());
    let (bg, fg, icon_color) = if response.hovered() { (CARD_ELEVATED, TEXT, accent()) } else if open { (CARD, TEXT, accent_glow()) } else { (CARD, TEXT_MUTED, TEXT_DIM) };
    let p = ui.painter_at(rect);
    p.rect_filled(rect, CornerRadius::same(RADIUS_SM as u8), bg);
    p.rect_stroke(rect, CornerRadius::same(RADIUS_SM as u8), Stroke::new(1.0, BORDER));
    if open {
        let bar = egui::Rect::from_min_max([rect.left(), rect.top() + 8.0].into(), [rect.left() + 3.0, rect.bottom() - 8.0].into());
        p.rect_filled(bar, CornerRadius::same(2), accent_glow());
    }
    let icon = if open { "▼" } else { "▶" };
    p.text([rect.left() + 12.0, rect.center().y].into(), Align2::LEFT_CENTER, icon, FontId::new(10.0, FontFamily::Proportional), icon_color);
    p.text([rect.left() + 28.0, rect.center().y].into(), Align2::LEFT_CENTER, title_text, FontId::new(14.0, font_medium()), fg);
    if !hint.is_empty() {
        p.text([rect.right() - 12.0, rect.center().y].into(), Align2::RIGHT_CENTER, hint, FontId::new(12.0, FontFamily::Proportional), TEXT_DIM);
    }
    if response.clicked() {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
        let key = title_text.as_ptr() as u64;
        let last_key = LAST_TOGGLE_KEY.load(Ordering::Relaxed);
        let last_ms = LAST_TOGGLE_MS.load(Ordering::Relaxed);
        if key != last_key || now.saturating_sub(last_ms) > 200 {
            LAST_TOGGLE_KEY.store(key, Ordering::Relaxed);
            LAST_TOGGLE_MS.store(now, Ordering::Relaxed);
            new_open = !open;
        }
    }
    if new_open {
        ui.add_space(4.0);
        body(ui);
        ui.add_space(4.0);
    }
    new_open != open
}
