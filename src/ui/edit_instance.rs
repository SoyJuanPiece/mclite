//! Edición de una instancia existente: nombre, RAM, resolución, versión y cargador.
//!
//! Reutiliza el mismo `Form` que «Nueva instancia» (lo rellena `app::open_edit`).
//! Si cambia la versión o el cargador, guardar relanza la instalación (repone lo
//! que falte; mods y mundos no se tocan).

use egui::Ui;

use crate::loaders::{LoaderKind, ALL_KINDS};
use crate::app::McLiteApp;
use crate::ui::{theme, widgets};

enum Action {
    Save,
    Sodium,
    Cancel,
}

pub fn show(app: &mut McLiteApp, ui: &mut Ui) {
    let Some(slug) = app.editing_slug.clone() else {
        app.screen = crate::app::Screen::Home;
        return;
    };
    let Some(instance) = app.store.find(&slug).cloned() else {
        app.screen = crate::app::Screen::Home;
        return;
    };

    let loader_options: Vec<(LoaderKind, &'static str, bool, Option<&'static str>)> = ALL_KINDS
        .iter()
        .map(|kind| (*kind, kind.label(), kind.is_implemented(), kind.note()))
        .collect();
    let loader_versions: Vec<(String, bool)> = app
        .form
        .loader_versions
        .iter()
        .map(|version| (version.id.clone(), version.stable))
        .collect();
    let loader_loading = app.form.loader_loading;
    let busy = app.job.is_some();

    let mut action: Option<Action> = None;

    egui::ScrollArea::vertical()
        .id_salt("edit-instance")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(8.0);
            ui.label(theme::title(&format!("Editar «{}»", instance.name)));
            if let Some(pack) = &instance.from_pack {
                ui.label(theme::muted(format!(
                    "Instalada del pack «{pack}» de Modrinth — su icono se conserva al editar"
                )));
            }
            ui.add_space(10.0);

            theme::card_section(ui, "NOMBRE", |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut app.form.name)
                        .hint_text("Mi instancia")
                        .desired_width(f32::INFINITY),
                );
            });

            theme::card_section(ui, "VERSIÓN DE MINECRAFT", |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut app.form.mc).hint_text("1.21.4"),
                );
                ui.label(theme::muted(
                    "Si cambias la versión, se reinstala lo que falte al guardar (mundos y mods se quedan).",
                ));
            });

            theme::card_section(ui, "CARGADOR", |ui| {
            if let Some(kind) = widgets::segmented(ui, app.form.loader, &loader_options) {
                if kind != app.form.loader {
                    app.form.loader = kind;
                    app.form.loader_version.clear();
                    app.form.loader_versions.clear();
                    app.form.loader_query = None;
                    if kind != LoaderKind::Vanilla {
                        app.begin_loader_fetch();
                    }
                }
            }
            if app.form.loader != LoaderKind::Vanilla {
                ui.add_space(6.0);
                theme::form_row(ui, "Versión del cargador", |ui| {
                    let selected = if app.form.loader_version.is_empty() {
                        "Última estable".to_string()
                    } else {
                        app.form.loader_version.clone()
                    };
                    egui::ComboBox::from_id_salt("edit-loader-version")
                        .selected_text(selected)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut app.form.loader_version,
                                String::new(),
                                "Última estable",
                            );
                            for (id, stable) in &loader_versions {
                                let label = if *stable {
                                    id.clone()
                                } else {
                                    format!("{id}  (inestable)")
                                };
                                ui.selectable_value(
                                    &mut app.form.loader_version,
                                    id.clone(),
                                    label,
                                );
                            }
                        });
                    if loader_loading {
                        ui.spinner();
                    }
                });
            }
            });

            theme::card_section(ui, "OPCIONES", |ui| {
                theme::form_row(ui, "RAM", |ui| {
                    ui.add(
                        egui::Slider::new(&mut app.form.ram_mb, 512..=16384)
                            .logarithmic(true)
                            .suffix(" MB"),
                    );
                });
                theme::form_row(ui, "Resolución", |ui| {
                    ui.add(egui::DragValue::new(&mut app.form.width).range(320..=7680));
                    ui.label("×");
                    ui.add(egui::DragValue::new(&mut app.form.height).range(200..=4320));
                });
            });

            // Sodium: solo Fabric, y en una acción aparte (no requiere reinstalar).
            if instance.loader == LoaderKind::Fabric {
                ui.horizontal(|ui| {
                    if theme::ghost_button(ui, "Instalar Sodium").clicked() {
                        action = Some(Action::Sodium);
                    }
                    ui.label(theme::muted(
                        "Baja Sodium a mods/ de esta instancia (necesita Internet una vez).",
                    ));
                });
                ui.add_space(4.0);
            }

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if theme::primary_button(ui, "Guardar", egui::vec2(140.0, 36.0)).clicked() {
                    action = Some(Action::Save);
                }
                if theme::ghost_button(ui, "Cancelar").clicked() {
                    action = Some(Action::Cancel);
                }
                if busy {
                    ui.spinner();
                }
            });
            ui.add_space(16.0);
        });

    match action {
        Some(Action::Save) => app.save_edit(),
        Some(Action::Sodium) => app.add_sodium(&slug),
        Some(Action::Cancel) => {
            app.editing_slug = None;
            app.screen = crate::app::Screen::Home;
        }
        None => {}
    }
}
