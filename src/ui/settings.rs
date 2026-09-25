//! Ajustes: cuenta offline, valores por defecto, Java y carpeta de datos.

use std::path::PathBuf;

use egui::Ui;

use crate::app::McLiteApp;
use crate::core::shell;
use crate::ui::{theme, widgets};

pub fn show(app: &mut McLiteApp, ui: &mut Ui) {
    // Datos precalculados para poder mutar `app` dentro de los closures.
    let javas: Vec<(PathBuf, u32, String, String)> = app
        .javas
        .iter()
        .map(|java| {
            (
                java.path.clone(),
                java.major,
                java.version.clone(),
                java.source.clone(),
            )
        })
        .collect();
    let javas_loading = app.javas_loading;
    let java_current = app.config.java_path.clone();

    let mut config_dirty = false;
    let mut detect = false;
    let mut open: Option<PathBuf> = None;

    egui::ScrollArea::vertical()
        .id_salt("settings")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(8.0);
            ui.label(theme::title("Ajustes"));
            ui.add_space(10.0);

            // ── Cuenta ───────────────────────────────────────────────────────
            card(ui, "CUENTA OFFLINE", |ui| {
                let mut nick = app.config.username.clone().unwrap_or_default();
                let response = ui.add(
                    egui::TextEdit::singleline(&mut nick).hint_text("Player (3–16 caracteres)"),
                );
                if response.changed() {
                    app.config.username = if nick.trim().is_empty() {
                        None
                    } else {
                        Some(nick.trim().to_string())
                    };
                    config_dirty = true;
                }
                ui.label(theme::muted(
                    "Sin login de Microsoft: el nick define tu nombre y UUID local.",
                ));
            });

            // ── Valores por defecto ──────────────────────────────────────────
            card(ui, "POR DEFECTO", |ui| {
                ui.horizontal(|ui| {
                    ui.label("RAM");
                    if ui
                        .add(
                            egui::Slider::new(&mut app.config.ram_mb, 512..=16384)
                                .logarithmic(true)
                                .suffix(" MB"),
                        )
                        .changed()
                    {
                        config_dirty = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Conexiones de descarga");
                    if ui
                        .add(egui::DragValue::new(&mut app.config.threads).range(1..=16))
                        .changed()
                    {
                        config_dirty = true;
                    }
                });
                let mut snapshots = app.config.show_snapshots;
                if ui
                    .checkbox(&mut snapshots, "Mostrar snapshots en las listas")
                    .changed()
                {
                    app.config.show_snapshots = snapshots;
                    config_dirty = true;
                }
                let mut old = app.config.show_old_versions;
                if ui
                    .checkbox(&mut old, "Mostrar beta/alpha históricas")
                    .changed()
                {
                    app.config.show_old_versions = old;
                    config_dirty = true;
                }
            });

            // ── Java ─────────────────────────────────────────────────────────
            card(ui, "JAVA", |ui| {
                let mut use_mojang = app.config.use_mojang_runtime;
                if ui
                    .checkbox(
                        &mut use_mojang,
                        "Bajar el Java de Mojang si el del sistema no sirve",
                    )
                    .changed()
                {
                    app.config.use_mojang_runtime = use_mojang;
                    config_dirty = true;
                }
                ui.label(theme::muted(
                    "Descarga el runtime oficial (jre-legacy / beta / delta) a runtime/, \
                     igual que hace el launcher oficial.",
                ));

                let current = match &java_current {
                    Some(path) => path.display().to_string(),
                    None => "Autodetectar al lanzar".to_string(),
                };
                ui.label(theme::muted(format!("Actual: {current}")));

                let mut manual = java_current
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default();
                let response = ui.add(
                    egui::TextEdit::singleline(&mut manual)
                        .hint_text("Ruta a java (vacío = autodetectar)"),
                );
                if response.changed() {
                    app.config.java_path = if manual.trim().is_empty() {
                        None
                    } else {
                        Some(manual.trim().into())
                    };
                    config_dirty = true;
                }

                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!javas_loading, egui::Button::new("Detectar Java"))
                        .clicked()
                    {
                        detect = true;
                    }
                    if javas_loading {
                        ui.spinner();
                        ui.label(theme::muted("Buscando JVMs en el sistema…"));
                    }
                });

                for (path, major, version, source) in &javas {
                    let selected = java_current.as_ref() == Some(path);
                    let label =
                        format!("Java {major} ({version}) — {}  [{source}]", path.display());
                    if ui.selectable_label(selected, label).clicked() {
                        app.config.java_path = Some(path.clone());
                        config_dirty = true;
                    }
                }
                if !javas.is_empty()
                    && ui
                        .selectable_label(
                            java_current.is_none(),
                            "Autodetectar (recomendado)",
                        )
                        .clicked()
                {
                    app.config.java_path = None;
                    config_dirty = true;
                }
            });

            // ── Datos ────────────────────────────────────────────────────────
            card(ui, "DATOS", |ui| {
                ui.label(theme::muted(format!(
                    "Carpeta: {}",
                    app.paths.root().display()
                )));
                ui.label(theme::muted(format!(
                    "{} instancias · índice en {}",
                    app.store.instances.len(),
                    app.paths.instances_file().display()
                )));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Abrir carpeta").clicked() {
                        open = Some(app.paths.root().to_path_buf());
                    }
                    if ui.button("Abrir logs").clicked() {
                        open = Some(app.paths.logs());
                    }
                    if ui.button("Abrir crashes").clicked() {
                        open = Some(app.paths.logs().join("crash"));
                    }
                });
                ui.label(theme::muted(
                    "launcher.log = arranque del launcher · crash/ = salida de cada partida",
                ));
            });

            ui.add_space(16.0);
        });

    if config_dirty && app.config.save(&app.paths).is_ok() {
        app.status = "Ajustes guardados".to_string();
    }
    if detect {
        app.detect_javas();
    }
    if let Some(dir) = open {
        if let Err(err) = shell::open_in_explorer(&dir) {
            app.error = Some(err.to_string());
        }
    }
}

/// Tarjeta con rótulo para cada sección de Ajustes.
fn card(ui: &mut Ui, title: &str, body: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .fill(theme::CARD)
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            widgets::section(ui, title);
            ui.add_space(4.0);
            body(ui);
        });
    ui.add_space(8.0);
}
