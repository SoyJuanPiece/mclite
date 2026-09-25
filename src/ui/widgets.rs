//! Widgets reutilizables: badges, control segmentado y progreso.

use egui::{Color32, CornerRadius, FontId, RichText, Stroke, Ui};
use crate::loaders::LoaderKind;

use crate::ui::theme;

/// Color de badge propio de cada cargador.
pub fn loader_color(kind: LoaderKind) -> Color32 {
    match kind {
        LoaderKind::Vanilla => Color32::from_rgb(0x3C, 0x85, 0x27),
        LoaderKind::Fabric => Color32::from_rgb(0xC2, 0x9A, 0x62),
        LoaderKind::Quilt => Color32::from_rgb(0x8A, 0x5C, 0xC0),
        LoaderKind::Forge => Color32::from_rgb(0xA6, 0x3A, 0x2E),
        LoaderKind::NeoForge => Color32::from_rgb(0x1F, 0x8A, 0x70),
        LoaderKind::OptiFine => Color32::from_rgb(0x2F, 0x7F, 0xBF),
    }
}

/// Pastilla redondeada de color con texto blanco.
pub fn badge(ui: &mut Ui, text: &str, fill: Color32) {
    egui::Frame::new()
        .fill(fill)
        .inner_margin(egui::Margin::same(5))
        .show(ui, |ui| {
            ui.label(RichText::new(text).strong().small().color(Color32::WHITE));
        });
}

/// Rótulo de sección en gris.
pub fn section(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).small().strong().color(theme::MUTED));
}

/// Control segmentado (`Vanilla | Fabric | ...`).
///
/// Las opciones deshabilitadas se muestran atenuadas y con la nota al pasar el
/// ratón, en vez de ocultarse: así el usuario ve que están previstas.
/// Devuelve la opción elegida, si cambió.
pub fn segmented<T: PartialEq + Copy>(
    ui: &mut Ui,
    current: T,
    options: &[(T, &'static str, bool, Option<&'static str>)],
) -> Option<T> {
    let mut chosen = None;
    ui.horizontal_wrapped(|ui| {
        for (value, label, enabled, note) in options {
            let response = if *enabled {
                ui.selectable_label(*value == current, *label)
            } else {
                ui.add_enabled(false, egui::Button::new(*label))
            };
            if response.clicked() {
                chosen = Some(*value);
            }
            match note {
                Some(note) if *enabled => {
                    response.on_hover_text(*note);
                }
                Some(note) => {
                    response.on_disabled_hover_text(*note);
                }
                None => {}
            }
        }
    });
    chosen
}

/// Barra de progreso propia: pista oscura redondeada, relleno de acento con
/// punta brillante y texto encima (fase, contador y ETA). Con `total == 0`
/// muestra spinner y fase.
pub fn progress(ui: &mut Ui, label: &str, phase: &str, done: u64, total: u64, eta: Option<String>) {
    if total == 0 {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(format!("{label} · {phase}…"));
        });
        return;
    }

    let fraction = (done as f32 / total as f32).clamp(0.0, 1.0);
    let height = 22.0;
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let painter = ui.painter();
    let radius = CornerRadius::same(height as u8 / 2);

    // Pista.
    painter.rect_filled(rect, radius, theme::INPUT);
    painter.rect_stroke(rect, radius, Stroke::new(1.0_f32, theme::BORDER), egui::StrokeKind::Inside);

    // Relleno.
    if fraction > 0.0 {
        let fill_rect = egui::Rect::from_min_max(
            rect.left_top(),
            [rect.left() + (rect.width() * fraction).max(height), rect.bottom()].into(),
        );
        painter.rect_filled(fill_rect, radius, theme::accent());
        // Punta más clara (efecto de avance).
        let tip_w = 8.0_f32.min(fill_rect.width() * 0.3);
        painter.rect_filled(
            egui::Rect::from_min_max(
                [fill_rect.right() - tip_w, fill_rect.top()].into(),
                fill_rect.right_bottom(),
            ),
            radius,
            theme::accent_soft(),
        );
    }

    // Texto centrado: fase · done/total · eta.
    let suffix = eta.map(|eta| format!(" · {eta}")).unwrap_or_default();
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        format!("{label} · {phase}: {done}/{total}{suffix}"),
        FontId::proportional(12.5),
        theme::TEXT,
    );
}
