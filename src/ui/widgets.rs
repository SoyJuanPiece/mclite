//! Widgets reutilizables: badges, control segmentado y progreso.

use egui::{Color32, RichText, Ui};
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

/// Barra de progreso con fase textual; con `total == 0` muestra spinner.
/// Con `eta` (velocidad + tiempo restante), lo añade al texto.
pub fn progress(ui: &mut Ui, label: &str, phase: &str, done: u64, total: u64, eta: Option<String>) {
    if total > 0 {
        let fraction = (done as f32 / total as f32).clamp(0.0, 1.0);
        let suffix = eta.map(|eta| format!(" · {eta}")).unwrap_or_default();
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
}
