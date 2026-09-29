//! Panel de crash: cuando el juego falla, los logs se abren delante de ti.
//!
//! Antes un crash dejaba un aviso pequeño en Home y había que ir a buscar los
//! ficheros. Ahora, al fallar la partida, este panel se abre solo con:
//!
//! * la causa en grande (lo primero que hay que leer);
//! * pestañas con TODO lo que haya caído en disco — resumen, sesión completa,
//!   log de mods, crash report de Mojang, `hs_err` de la JVM y `latest.log`;
//! * botones para copiar el texto, abrir la carpeta del expediente y cerrar.
//!
//! Solo se leen las últimas (o primeras) líneas de cada fichero: un `latest.log`
//! puede tener megas y no queremos congelar la ventana por pintarlo.

use egui::{Align2, Color32, CornerRadius, RichText, Stroke, Ui};

use crate::app::McLiteApp;
use crate::core::crash::GameExit;
use crate::ui::theme;

pub fn show(app: &mut McLiteApp, ctx: &egui::Context) {
    if !app.crash_open {
        return;
    }
    let Some(exit) = app.last_game_exit.clone() else {
        app.crash_open = false;
        return;
    };

    let reports = exit.reports();
    if reports.is_empty() {
        app.crash_open = false;
        return;
    }
    if app.crash_tab >= reports.len() {
        app.crash_tab = 0;
    }

    let mut close = false;
    let mut copy: Option<String> = None;
    let mut open_dir: Option<std::path::PathBuf> = None;

    egui::Window::new("crash-panel")
        .title_bar(false)
        .collapsible(false)
        .resizable(true)
        .default_size([820.0, 580.0])
        .min_width(520.0)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .frame(
            egui::Frame::new()
                .fill(theme::CARD)
                .stroke(Stroke::new(1.0_f32, theme::BORDER))
                .corner_radius(CornerRadius::same(12))
                .shadow(theme::shadow())
                .inner_margin(egui::Margin::same(16)),
        )
        .show(ctx, |ui| {
            header(ui, &exit, &mut close);
            ui.add_space(10.0);
            theme::separator(ui);

            // ── Pestañas ────────────────────────────────────────────────────
            ui.horizontal_wrapped(|ui| {
                for (index, (label, path)) in reports.iter().enumerate() {
                    let selected = index == app.crash_tab;
                    let exists = path.is_file();
                    let text = RichText::new(*label)
                        .size(12.5)
                        .family(theme::semibold())
                        .color(if selected {
                            Color32::WHITE
                        } else if exists {
                            theme::TEXT
                        } else {
                            theme::MUTED
                        });
                    let button = egui::Button::new(text)
                        .fill(if selected { theme::accent() } else { theme::INPUT })
                        .stroke(Stroke::new(1.0_f32, theme::BORDER))
                        .corner_radius(CornerRadius::same(14));
                    if ui.add(button).clicked() {
                        app.crash_tab = index;
                    }
                }
            });
            ui.add_space(8.0);

            // ── Contenido del fichero elegido ───────────────────────────────
            let (label, path) = &reports[app.crash_tab];
            if !path.is_file() {
                ui.label(theme::muted(format!(
                    "«{label}» no se guardó para esta sesión (el juego no lo dejó en disco)."
                )));
            } else {
                // La sesión y los mods interesan por el final; los reportes, por el
                // principio (la descripción del fallo va arriba).
                let tail = matches!(*label, "Sesión" | "Mods" | "latest.log");
                let text = app.report_text(path, tail);
                ui.horizontal(|ui| {
                    if theme::ghost_button(ui, "Copiar").clicked() {
                        copy = Some(text.clone());
                    }
                    if theme::ghost_button(ui, "Abrir carpeta").clicked() {
                        open_dir = Some(path.parent().map(std::path::Path::to_path_buf).unwrap_or_default());
                    }
                    ui.label(theme::muted(format!("{} · {}", *label, short_path(path))));
                });
                ui.add_space(6.0);
                egui::ScrollArea::vertical()
                    .id_salt(("crash-body", app.crash_tab))
                    .max_height((ui.available_height() - 46.0).max(140.0))
                    .auto_shrink([false, false])
                    .stick_to_bottom(tail)
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        for line in text.lines() {
                            ui.label(
                                RichText::new(line)
                                    .monospace()
                                    .size(12.0)
                                    .color(log_line_color(line)),
                            );
                        }
                    });
            }

            ui.add_space(10.0);
            theme::separator(ui);
            ui.horizontal(|ui| {
                if let Some(dir) = &exit.crash_dir {
                    if theme::primary_button(ui, "Abrir expediente", egui::vec2(170.0, 32.0)).clicked() {
                        open_dir = Some(dir.clone());
                    }
                }
                if theme::ghost_button(ui, "Carpeta de logs").clicked() {
                    open_dir = Some(app.paths.logs());
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if theme::ghost_button(ui, "Cerrar").clicked() {
                        close = true;
                    }
                });
            });
        });

    if let Some(text) = copy {
        ctx.copy_text(text);
        app.notify("Log copiado al portapapeles", crate::app::ToastKind::Ok);
    }
    if let Some(dir) = open_dir {
        if !dir.as_os_str().is_empty() {
            if let Err(err) = crate::core::shell::open_in_explorer(&dir) {
                app.error = Some(err.to_string());
            }
        }
    }
    if close {
        app.crash_open = false;
    }
}

fn header(ui: &mut Ui, exit: &GameExit, close: &mut bool) {
    egui::Frame::new()
        .fill(Color32::from_rgb(0x33, 0x1A, 0x1A))
        .stroke(Stroke::new(1.0_f32, theme::DANGER))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new("⚠  El juego se cerró inesperadamente")
                            .size(17.0)
                            .family(theme::semibold())
                            .color(theme::DANGER),
                    );
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(
                            exit.cause
                                .clone()
                                .unwrap_or_else(|| "causa desconocida".to_string()),
                        )
                        .size(13.5)
                        .color(theme::TEXT),
                    );
                    ui.label(theme::muted(format!(
                        "código de salida {} · estos logs ya están guardados en logs/",
                        exit.code
                    )));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if theme::ghost_button(ui, "✕").clicked() {
                        *close = true;
                    }
                });
            });
        });
}

/// Colorea la línea del log según su nivel: errores en rojo, avisos en ámbar,
/// el resto en el color normal. Ver un log entero en un color es ilegible.
fn log_line_color(line: &str) -> Color32 {
    let lower = line.to_lowercase();
    if lower.contains("/error") || lower.contains("exception") || lower.contains("\terror\t") {
        theme::DANGER
    } else if lower.contains("/warn") {
        Color32::from_rgb(0xE8, 0xC1, 0x6F)
    } else if lower.contains("/debug") || lower.contains("/trace") {
        theme::MUTED
    } else {
        theme::TEXT
    }
}

/// Ruta recortada al final (lo útil de una ruta larga es el nombre del fichero).
fn short_path(path: &std::path::Path) -> String {
    let parts: Vec<String> = path
        .components()
        .rev()
        .take(3)
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect();
    parts.into_iter().rev().collect::<Vec<_>>().join("/")
}
