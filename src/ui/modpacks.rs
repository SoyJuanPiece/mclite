//! Modpacks de Modrinth: buscar → detalle con icono y descripción → instalar.
//!
//! Los iconos se cargan por URL con el image loader de egui_extras (instalado al
//! arrancar); egui los cachea solo, así que scrollar la lista no re-descarga.

use egui::{RichText, Ui};

use crate::app::McLiteApp;
use crate::ui::{theme, widgets};

enum Action {
    Search,
    Open(String),
    Install { slug: String, version_id: String },
    Back,
}

pub fn show(app: &mut McLiteApp, ui: &mut Ui) {
    let packs: Vec<(String, String, String, u64, Vec<String>, Option<String>)> = app
        .packs
        .iter()
        .map(|pack| {
            (
                pack.slug.clone(),
                pack.title.clone(),
                pack.description.clone(),
                pack.downloads,
                pack.versions.clone(),
                pack.icon_url.clone(),
            )
        })
        .collect();
    let versions: Vec<(String, String, String)> = app
        .pack_versions
        .iter()
        .map(|version| {
            (
                version.id.clone(),
                version.version_number.clone(),
                version.game_versions.first().cloned().unwrap_or_default(),
            )
        })
        .collect();
    let loading = app.packs_loading;
    let detail = app.pack_detail.clone();
    let search_text = app.pack_search.clone();

    let mut action: Option<Action> = None;

    egui::ScrollArea::vertical()
        .id_salt("modpacks")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(8.0);

            // ── Detalle del pack ─────────────────────────────────────────────
            if let Some(detail) = &detail {
                if ui.button("← Volver a la lista").clicked() {
                    action = Some(Action::Back);
                }
                ui.add_space(8.0);
                pack_header(ui, detail, &versions, loading, &mut action);
                ui.add_space(12.0);
                widgets::section(ui, "DESCRIPCIÓN");
                ui.add_space(2.0);
                // El body es Markdown; lo mostramos plano y legible (los links
                // quedan como texto, sin romper nada).
                let body = detail.body.replace("\r\n", "\n");
                if body.trim().is_empty() {
                    ui.label(theme::muted("Sin descripción."));
                } else {
                    for block in body.split("\n\n").take(24) {
                        let block = block.trim();
                        if block.is_empty() {
                            continue;
                        }
                        // Los títulos Markdown (## …) van un poco más grandes.
                        if let Some(title) = block.strip_prefix("## ").or_else(|| block.strip_prefix("# ")) {
                            ui.add_space(4.0);
                            ui.label(RichText::new(collapse(title)).strong());
                        } else {
                            ui.label(RichText::new(collapse(block)).size(13.0));
                        }
                        ui.add_space(3.0);
                    }
                }
                ui.add_space(16.0);
                return; // en detalle no se pinta la lista
            }

            // ── Búsqueda ─────────────────────────────────────────────────────
            ui.label(theme::title("Modpacks (Modrinth)"));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut app.pack_search)
                        .hint_text("Buscar modpacks… (vacío = populares)")
                        .desired_width(320.0),
                );
                if ui.button("Buscar").clicked()
                    || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                {
                    action = Some(Action::Search);
                }
                if loading {
                    ui.spinner();
                }
            });
            ui.add_space(8.0);

            if packs.is_empty() && !loading {
                ui.label(theme::muted(
                    "Busca un modpack por nombre, o pulsa Buscar para ver los populares.",
                ));
            }
            for (slug, title, description, downloads, mc_versions, icon) in &packs {
                theme::card(ui, |ui| {
                        ui.horizontal(|ui| {
                            // Icono del pack (72×72). Mientras carga, cuadro vacío.
                            match icon.as_deref().filter(|url| !url.is_empty()) {
                                Some(url) => {
                                    ui.add(
                                        egui::Image::from_uri(url)
                                            .fit_to_exact_size(egui::vec2(72.0, 72.0))
                                            .corner_radius(6.0),
                                    );
                                }
                                None => {
                                    let (rect, _) = ui.allocate_exact_size(
                                        egui::vec2(72.0, 72.0),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter()
                                        .rect_filled(rect, 6.0, theme::SIDEBAR);
                                    ui.painter().text(
                                        rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        "📦",
                                        egui::TextStyle::Heading.resolve(ui.style()),
                                        theme::MUTED,
                                    );
                                }
                            }
                            ui.add_space(8.0);
                            ui.vertical(|ui| {
                                ui.label(RichText::new(title).strong().size(15.0));
                                ui.label(theme::muted(format!(
                                    "{downloads} descargas · MC {}",
                                    mc_versions.first().cloned().unwrap_or_default()
                                )));
                                if !description.is_empty() {
                                    ui.label(theme::muted(collapse(description)));
                                }
                                if ui.button("Ver detalle e instalar").clicked() {
                                    action = Some(Action::Open(slug.clone()));
                                }
                            });
                        });
                    });
                ui.add_space(4.0);
            }
            ui.add_space(16.0);
        });

    match action {
        Some(Action::Search) => {
            app.pack_detail = None;
            app.pack_versions_slug = None;
            app.pack_versions.clear();
            app.search_packs();
        }
        Some(Action::Open(slug)) => app.fetch_pack_versions(slug),
        Some(Action::Install { slug, version_id }) => {
            let Some(hit) = app.packs.iter().find(|pack| pack.slug == slug).cloned() else {
                return;
            };
            let Some(version) = app
                .pack_versions
                .iter()
                .find(|version| version.id == version_id)
                .cloned()
            else {
                return;
            };
            app.start_pack_install(&hit, &version);
        }
        Some(Action::Back) => {
            app.pack_detail = None;
            app.pack_versions_slug = None;
            app.pack_versions.clear();
        }
        None => {}
    }
    let _ = search_text;
}

/// Cabecera del detalle: icono grande, título, autor, stats y selector de versión.
fn pack_header(
    ui: &mut Ui,
    detail: &crate::core::modrinth::PackDetail,
    versions: &[(String, String, String)],
    loading: bool,
    action: &mut Option<Action>,
) {
    theme::card(ui, |ui| {
            ui.horizontal(|ui| {
                match detail.icon_url.as_deref().filter(|url| !url.is_empty()) {
                    Some(url) => {
                        ui.add(
                            egui::Image::from_uri(url)
                                .fit_to_exact_size(egui::vec2(96.0, 96.0))
                                .corner_radius(8.0),
                        );
                    }
                    None => {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(96.0, 96.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 8.0, theme::SIDEBAR);
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "📦",
                            egui::TextStyle::Heading.resolve(ui.style()),
                            theme::MUTED,
                        );
                    }
                }
                ui.add_space(10.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(&detail.title).strong().size(20.0));
                    ui.label(theme::muted(format!(
                        "por {} · {} descargas · {} seguidores",
                        detail.author(),
                        detail.downloads,
                        detail.followers
                    )));
                    if !detail.description.is_empty() {
                        ui.label(theme::muted(&detail.description));
                    }
                    ui.horizontal(|ui| {
                        for category in detail.categories.iter().take(5) {
                            widgets::badge(ui, category, theme::accent());
                        }
                    });
                    if let Some(published) = &detail.published {
                        ui.label(theme::muted(format!(
                            "Publicado: {}",
                            published.get(..10).unwrap_or(published)
                        )));
                    }
                });
            });

            ui.add_space(10.0);
            widgets::section(ui, "VERSIONES");
            if versions.is_empty() && !loading {
                ui.label(theme::muted("Sin versiones publicadas."));
            }
            if loading && versions.is_empty() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(theme::muted("Cargando versiones…"));
                });
            }
            for (id, number, game) in versions {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(number).strong());
                    if !game.is_empty() {
                        widgets::badge(ui, game, theme::accent());
                    }
                    if ui.button("Instalar").clicked() {
                        *action = Some(Action::Install {
                            slug: detail.slug.clone(),
                            version_id: id.clone(),
                        });
                    }
                });
            }
        });
}

/// Colapsa espacios y recorta: las descripciones largas no rompen el layout.
fn collapse(text: &str) -> String {
    let mut out = String::new();
    for word in text.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
        if out.len() > 280 {
            out.push('…');
            break;
        }
    }
    out
}
