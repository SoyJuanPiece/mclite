//! Formulario de «Nueva instancia»: versión de Minecraft, cargador y opciones.

use egui::Ui;

use crate::core::instance::Instance;
use crate::core::manifest::{VersionFilter, VersionManifest, VersionType};
use crate::loaders::{LoaderKind, ALL_KINDS};

use crate::app::McLiteApp;
use crate::ui::{theme, widgets};

/// Sin búsqueda se pintan pocas filas (egui repinta todo cada frame); con
/// búsqueda, más, para que el filtrado sea útil.
const ROWS_IDLE: usize = 80;
const ROWS_SEARCH: usize = 200;

/// Una fila de la lista de versiones. Aquí solo llegan las soportadas por el
/// cargador elegido (el filtro se hace en `build_groups`).
struct Row {
    id: String,
    /// ¿Es la última release/snapshot que publica Mojang?
    latest: bool,
}

/// Bloque de la lista con su contador real (aunque se muestren menos filas).
struct Group {
    title: &'static str,
    total: usize,
    /// Cuántas de las versiones del grupo soporta el cargador actual.
    supported_total: usize,
    rows: Vec<Row>,
}

enum Action {
    Create,
    RetryManifest,
    FetchLoaders,
}

fn build_groups(
    manifest: &VersionManifest,
    filter: &VersionFilter,
    search: &str,
    supported: Option<&[String]>,
) -> Vec<Group> {
    let cap = if search.is_empty() {
        ROWS_IDLE
    } else {
        ROWS_SEARCH
    };
    let mut groups = vec![
        Group {
            title: "OFICIALES",
            total: 0,
            supported_total: 0,
            rows: Vec::new(),
        },
        Group {
            title: "SNAPSHOTS",
            total: 0,
            supported_total: 0,
            rows: Vec::new(),
        },
        Group {
            title: "BETA / ALPHA",
            total: 0,
            supported_total: 0,
            rows: Vec::new(),
        },
    ];

    for entry in &manifest.versions {
        if !filter.allows(entry.version_type) {
            continue;
        }
        if !search.is_empty() && !entry.id.to_lowercase().contains(search) {
            continue;
        }
        // Con datos de soporte: las versiones sin el cargador NO se muestran.
        // La lista es "qué puedo instalar ahora", no un catálogo histórico.
        if let Some(list) = supported {
            if !list.iter().any(|id| id == &entry.id) {
                continue;
            }
        }
        let index = match entry.version_type {
            VersionType::Release => 0,
            VersionType::Snapshot => 1,
            VersionType::OldBeta | VersionType::OldAlpha => 2,
        };
        let group = &mut groups[index];
        group.total += 1;
        if supported.is_some() {
            group.supported_total += 1;
        }
        if group.rows.len() < cap {
            let latest = match entry.version_type {
                VersionType::Release => manifest.is_latest_release(&entry.id),
                VersionType::Snapshot => manifest.is_latest_snapshot(&entry.id),
                _ => false,
            };
            group.rows.push(Row {
                id: entry.id.clone(),
                latest,
            });
        }
    }

    groups.retain(|group| group.total > 0);
    groups
}

pub fn show(app: &mut McLiteApp, ui: &mut Ui) {
    // ── Datos precalculados: así los closures pueden mutar `app` sin roces ──
    let search = app.form.search.trim().to_lowercase();
    let filter = app.config.version_filter();
    let manifest_missing = app.manifest.is_none();
    let manifest_loading = app.manifest_loading;

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
    let supported: Option<Vec<String>> = if app.form.loader == LoaderKind::Vanilla {
        None
    } else {
        app.form.supported_mcs.clone()
    };
    let groups: Vec<Group> = match &app.manifest {
        Some(manifest) => build_groups(
            manifest,
            &filter,
            &search,
            supported.as_deref(),
        ),
        None => Vec::new(),
    };

    // Mientras se consulta qué versiones soporta el cargador, no se permite
    // crear: la versión elegida podría desaparecer de la lista al llegar el dato.
    let probing = app.form.loader != LoaderKind::Vanilla && supported.is_none();
    let can_create = app.job.is_none()
        && !app.form.name.trim().is_empty()
        && !app.form.mc.is_empty()
        && (app.form.loader == LoaderKind::Vanilla || !loader_loading)
        && !probing;

    let mut action: Option<Action> = None;
    let mut config_dirty = false;

    egui::ScrollArea::vertical()
        .id_salt("new-instance")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(8.0);
            ui.label(theme::title("Nueva instancia"));
            ui.add_space(10.0);

            // ── Nombre ───────────────────────────────────────────────────────
            widgets::section(ui, "NOMBRE");
            let response = ui.add(
                egui::TextEdit::singleline(&mut app.form.name).hint_text("Mi instancia"),
            );
            if response.changed() {
                app.form.name_edited = true;
            }

            ui.add_space(8.0);

            // ── Versión de Minecraft ─────────────────────────────────────────
            widgets::section(ui, "VERSIÓN DE MINECRAFT");
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut app.form.search)
                        .hint_text("Buscar…")
                        .desired_width(180.0),
                );
                let mut snapshots = app.config.show_snapshots;
                if ui.checkbox(&mut snapshots, "Mostrar snapshots").changed() {
                    app.config.show_snapshots = snapshots;
                    config_dirty = true;
                }
                let mut old = app.config.show_old_versions;
                if ui.checkbox(&mut old, "Beta/Alpha").changed() {
                    app.config.show_old_versions = old;
                    config_dirty = true;
                }
            });

            if manifest_missing {
                ui.horizontal(|ui| {
                    if manifest_loading {
                        ui.spinner();
                        ui.label(theme::muted("Cargando el manifiesto de versiones…"));
                    } else {
                        ui.label(theme::muted("No pude cargar el manifiesto."));
                        if ui.button("Reintentar").clicked() {
                            action = Some(Action::RetryManifest);
                        }
                    }
                });
            } else {
                let mut picked: Option<String> = None;
                egui::ScrollArea::vertical()
                    .id_salt("version-list")
                    .max_height(250.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for group in &groups {
                            ui.add_space(6.0);
                            let title = if supported.is_some() {
                                format!(
                                    "{} ({} con {})",
                                    group.title,
                                    group.supported_total,
                                    app.form.loader.label()
                                )
                            } else {
                                format!("{} ({})", group.title, group.total)
                            };
                            widgets::section(ui, &title);
                            for row in &group.rows {
                                // Aquí solo llegan versiones soportadas: con datos
                                // de soporte las otras ni se pintan.
                                let text = if row.latest {
                                    format!("{}  (última)", row.id)
                                } else {
                                    row.id.clone()
                                };
                                let selected = app.form.mc == row.id;
                                if ui
                                    .selectable_label(selected, text)
                                    .clicked()
                                {
                                    picked = Some(row.id.clone());
                                }
                            }
                            if group.total > group.rows.len() {
                                ui.label(theme::muted(format!(
                                    "  … y {} más (usa el buscador)",
                                    group.total - group.rows.len()
                                )));
                            }
                        }
                    });
                // Mientras llega la sonda de soporte, mejor no dejar elegir:
                // cualquier elección podría ser de una versión que va a desaparecer.
                if supported.is_none() && app.form.loader != LoaderKind::Vanilla {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(theme::muted(format!(
                            "Consultando qué versiones tienen {}…",
                            app.form.loader.label()
                        )));
                    });
                }
                if let Some(id) = picked {
                    let changed = app.form.mc != id;
                    app.form.mc = id;
                    if !app.form.name_edited {
                        app.form.name =
                            Instance::suggested_name(app.form.loader, &app.form.mc);
                    }
                    if changed && app.form.loader != LoaderKind::Vanilla {
                        action = Some(Action::FetchLoaders);
                    }
                }
            }

            ui.add_space(10.0);

            // ── Cargador ─────────────────────────────────────────────────────
            widgets::section(ui, "CARGADOR");
            if let Some(kind) = widgets::segmented(ui, app.form.loader, &loader_options) {
                if kind != app.form.loader {
                    app.form.loader = kind;
                    app.form.loader_version.clear();
                    app.form.loader_versions.clear();
                    app.form.supported_mcs = None;
                    if !app.form.name_edited && !app.form.mc.is_empty() {
                        app.form.name = Instance::suggested_name(kind, &app.form.mc);
                    }
                    action = Some(Action::FetchLoaders);
                    if kind != LoaderKind::Vanilla {
                        app.begin_supported_fetch();
                    }
                }
            }

            if app.form.loader != LoaderKind::Vanilla {
                ui.horizontal(|ui| {
                    ui.label("Versión del cargador");
                    let selected = if app.form.loader_version.is_empty() {
                        "Última estable".to_string()
                    } else {
                        app.form.loader_version.clone()
                    };
                    egui::ComboBox::from_id_salt("loader-version")
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
                if let Some(note) = app.form.loader.note() {
                    ui.label(theme::muted(note));
                }
            }

            // Sodium: solo para Fabric (OptiFine y Sodium no conviven, y en
            // vanilla/Forge/NeoForge no aplica).
            if app.form.loader == LoaderKind::Fabric {
                let mut with_sodium = app.form.with_sodium;
                if ui
                    .checkbox(&mut with_sodium, "Instalar Sodium (rendimiento)")
                    .changed()
                {
                    app.form.with_sodium = with_sodium;
                }
                ui.label(theme::muted(
                    "El reemplazo moderno de OptiFine para Fabric. Se baja de Modrinth.",
                ));
            }

            ui.add_space(10.0);

            // ── Opciones ─────────────────────────────────────────────────────
            widgets::section(ui, "OPCIONES");
            ui.horizontal(|ui| {
                ui.label("RAM");
                ui.add(
                    egui::Slider::new(&mut app.form.ram_mb, 512..=16384)
                        .logarithmic(true)
                        .suffix(" MB"),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Resolución");
                ui.add(egui::DragValue::new(&mut app.form.width).range(320..=7680));
                ui.label("×");
                ui.add(egui::DragValue::new(&mut app.form.height).range(200..=4320));
            });

            // ── Crear ────────────────────────────────────────────────────────
            ui.add_space(12.0);
            let width = ui.available_width();
            let create = egui::Button::new(egui::RichText::new("Crear instancia")
                .size(17.0)
                .strong())
            .fill(theme::ACCENT)
            .min_size(egui::vec2(width, 44.0));
            if ui.add_enabled(can_create, create).clicked() {
                action = Some(Action::Create);
            }
            if app.job.is_some() {
                ui.label(theme::muted(
                    "Espera a que termine lo que está en curso.",
                ));
            }
            ui.add_space(16.0);
        });

    if config_dirty {
        let _ = app.config.save(&app.paths);
    }

    match action {
        Some(Action::Create) => app.start_create(),
        Some(Action::RetryManifest) => app.retry_manifest(),
        Some(Action::FetchLoaders) => app.begin_loader_fetch(),
        None => {}
    }
}
