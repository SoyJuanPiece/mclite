//! Pantalla de inicio: cabecera de la instancia, botón JUGAR, progreso y log.

use egui::{Color32, CornerRadius, RichText, Stroke, Ui};

use crate::app::McLiteApp;
use crate::ui::{theme, widgets};

enum Action {
    Play,
    Edit,
    Repair,
    Delete,
}

pub fn show(app: &mut McLiteApp, ui: &mut Ui) {
    let selected = app.selected.clone();
    egui::ScrollArea::vertical()
        .id_salt("home")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(12.0);
            ui.add_space(2.0);
            match &selected {
                None => welcome(ui, app),
                Some(slug) => detail(ui, app, slug),
            }
            ui.add_space(10.0);
        });
}

fn welcome(ui: &mut Ui, app: &mut McLiteApp) {
    ui.vertical_centered(|ui| {
        ui.add_space(40.0);
        theme::grass_block(ui, 84.0);
        ui.add_space(8.0);
        ui.label(theme::title("McLite"));
        ui.add_space(4.0);
        ui.label(theme::muted("Launcher lite de Minecraft · cuentas offline"));
        ui.add_space(16.0);
    });

    // Onboarding de 2 clics: nick + acento + crear, todo en una tarjeta.
    let mut dirty = false;
    let mut go_new = false;
    theme::card_section(ui, "EMPIEZA AQUÍ", |ui| {
        ui.label(theme::muted(
            "1 · ¿Cómo te llamas en el juego? (podrás cambiarlo en Ajustes)",
        ));
        let mut nick = app.config.username.clone().unwrap_or_default();
        let response = ui.add(
            egui::TextEdit::singleline(&mut nick)
                .hint_text("Tu nick (3–16 caracteres)")
                .desired_width(260.0),
        );
        if response.changed() {
            app.config.username = if nick.trim().is_empty() {
                None
            } else {
                Some(nick.trim().to_string())
            };
            dirty = true;
        }
        ui.add_space(8.0);
        ui.label(theme::muted("2 · Elige tu color:"));
        ui.horizontal(|ui| {
            for accent in theme::ACCENTS {
                let chosen = app.config.accent.as_deref() == Some(accent.key());
                let (rect, response) = ui.allocate_exact_size(
                    egui::vec2(24.0, 24.0),
                    egui::Sense::click(),
                );
                ui.painter().circle_filled(rect.center(), 10.0, accent.color());
                if chosen {
                    ui.painter().circle_stroke(
                        rect.center(),
                        12.0,
                        egui::Stroke::new(2.0_f32, theme::TEXT),
                    );
                }
                if response.clicked() {
                    app.config.accent = Some(accent.key().to_string());
                    theme::set_accent(accent);
                    theme::reapply_accent(ui.ctx());
                    dirty = true;
                }
                response.on_hover_text(accent.name());
            }
        });
        ui.add_space(10.0);
        if theme::primary_button(ui, "Crear mi primera instancia →", egui::vec2(260.0, 36.0))
            .clicked()
        {
            go_new = true;
        }
        ui.label(theme::muted(
            "Consejo: también puedes arrastrar un .mrpack aquí para instalar un modpack",
        ));
    });

    if dirty {
        let _ = app.config.save(&app.paths);
    }
    if go_new {
        app.open_new();
    }
}

/// Banner de la instancia: icono/avatar, nombre, datos y badges; la ruta de
/// la carpeta vive en el tooltip para no ensuciar (ni desbordar) el layout.
fn hero(
    ui: &mut Ui,
    name: &str,
    nick: &str,
    sub: &str,
    dir: &str,
    icon: Option<&str>,
    loader: crate::loaders::LoaderKind,
    mc: &str,
    loader_version: Option<&str>,
) {
    let inner = egui::Frame::new()
        .fill(Color32::from_rgb(0x1B, 0x2B, 0x1B))
        .stroke(Stroke::new(1.0_f32, theme::BORDER))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                if let Some(url) = icon {
                    // Instancia con icono (pack de Modrinth): miniatura del pack.
                    ui.add(
                        egui::Image::from_uri(url)
                            .max_size(egui::vec2(46.0, 46.0))
                            .corner_radius(CornerRadius::same(10)),
                    );
                } else {
                    theme::avatar(ui, nick, 46.0);
                }
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(name)
                                .size(22.0)
                                .family(theme::semibold()),
                        );
                        ui.label(theme::muted(format!("· {nick}")));
                    });
                    ui.label(theme::muted(sub));
                });
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                widgets::badge(ui, loader.label(), widgets::loader_color(loader));
                widgets::badge(ui, mc, theme::accent());
                if let Some(version) = loader_version {
                    widgets::badge(ui, version, theme::CARD_ELEVATED);
                }
            });
            ui.add_space(2.0);
        });
    inner
        .response
        .on_hover_text(format!("Carpeta: {dir}"));

    // Línea de acento bajo el banner (recorre todo el ancho).
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 3.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(2), theme::accent());
}

/// "2026-09-25T04:40:02Z" → "25/09/2026 04:40". Si no encaja el formato, tal cual.
fn pretty_timestamp(raw: &str) -> String {
    let bytes = raw.as_bytes();
    if raw.len() >= 16 && bytes[4] == b'-' && bytes[7] == b'-' && bytes[10] == b'T' {
        format!(
            "{}/{}/{} {}",
            &raw[8..10],
            &raw[5..7],
            &raw[0..4],
            &raw[11..16]
        )
    } else {
        raw.to_string()
    }
}

fn detail(ui: &mut Ui, app: &mut McLiteApp, slug: &str) {
    let Some(instance) = app.store.find(slug).cloned() else {
        ui.label(theme::muted("Esa instancia ya no existe."));
        return;
    };

    let busy = app.job.is_some();
    let confirm = app.confirm_delete.as_deref() == Some(slug);
    let mut action: Option<Action> = None;
    let mut open_dir = false;
    let mut open_log: Option<std::path::PathBuf> = None;
    let nick = app.config.username_or_default();

    // ── Cabecera ─────────────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label(theme::title("Jugar"));            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(theme::muted(format!(
                    "Última partida: {}",
                    instance
                        .last_played
                        .as_deref()
                        .map(pretty_timestamp)
                        .unwrap_or_else(|| "nunca".to_string())
                )));
            });
    });
    ui.add_space(4.0);

    let icon_uri = instance
        .icon
        .as_deref()
        .map(|url| crate::core::icons::resolve(&app.paths, url));
    hero(
        ui,
        &instance.name,
        &nick,
        &format!(
            "{} · {} MB de RAM · {}×{}",
            instance.version_id(),
            instance.ram_clamped(),
            instance.width,
            instance.height
        ),
        &instance.game_dir(&app.paths).display().to_string(),
        icon_uri.as_deref(),
        instance.loader,
        &instance.mc_version,
        instance.loader_version.as_deref(),
    );
    ui.add_space(14.0);

    // ── Acciones: JUGAR + columna de gestión a la derecha ────────────────────
    ui.horizontal(|ui| {
        let width = ui.available_width() - 130.0;
        let play = egui::Button::new(RichText::new("▶  JUGAR").size(22.0).strong())
        .fill(theme::accent())
        .corner_radius(CornerRadius::same(10))
        .min_size(egui::vec2(width, 58.0));
        if ui.add_enabled(!busy, play).clicked() {
            action = Some(Action::Play);
        }

        ui.vertical(|ui| {
            let small = |text: &str| {
                egui::Button::new(RichText::new(text).size(12.5))
                    .fill(theme::CARD)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER))
                    .corner_radius(CornerRadius::same(8))
                    .min_size(egui::vec2(120.0, 12.0))
            };
            if ui.add(small("Carpeta")).clicked() {
                open_dir = true;
            }
            if ui.add_enabled(!busy, small("Editar")).clicked() {
                action = Some(Action::Edit);
            }
            if ui.add_enabled(!busy, small("Reparar")).clicked() {
                action = Some(Action::Repair);
            }
            let label = if confirm {
                "¿Borrar de verdad?"
            } else {
                "Borrar"
            };
            if ui
                .add(
                    egui::Button::new(RichText::new(label).size(12.5).color(if confirm {
                        theme::DANGER
                    } else {
                        theme::TEXT
                    }))
                    .fill(theme::CARD)
                    .stroke(Stroke::new(
                        1.0_f32,
                        if confirm { theme::DANGER } else { theme::BORDER },
                    ))
                    .corner_radius(CornerRadius::same(8))
                    .min_size(egui::vec2(120.0, 12.0)),
                )
                .clicked()
            {
                if confirm {
                    action = Some(Action::Delete);
                } else {
                    app.confirm_delete = Some(slug.to_string());
                }
            }
        });
    });

    if let Some(job) = &app.job {
        ui.add_space(6.0);
        let eta = job
            .speed_and_eta()
            .map(|(speed, eta)| format!("{:.0}/s · queda {}", speed, eta));
        widgets::progress(ui, &job.label, &job.phase, job.done, job.total, eta);
    }

    // ── Aviso de crash de la última partida ─────────────────────────────────
    if let Some(exit) = &app.last_game_exit {
        if !exit.ok {
            ui.add_space(6.0);
            egui::Frame::new()
                .fill(Color32::from_rgb(0x2A, 0x18, 0x18))
                .stroke(Stroke::new(1.0_f32, theme::DANGER))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(format!(
                            "⚠ La última partida se cerró inesperadamente ({})",
                            exit.cause.as_deref().unwrap_or("causa desconocida")
                        ))
                        .color(theme::DANGER),
                    );
                    ui.horizontal(|ui| {
                        if theme::ghost_button(ui, "Ver log del juego").clicked() {
                            open_log = Some(exit.log_path.clone());
                        }
                        if let Some(report) = &exit.crash_report {
                            if theme::ghost_button(ui, "Abrir crash report").clicked() {
                                open_log = Some(report.clone());
                            }
                        }
                    });
                });
        }
    }

    // ── Log ──────────────────────────────────────────────────────────────────
    ui.add_space(8.0);
    let log_len = app.log.len();
    ui.collapsing(format!("Log ({log_len})"), |ui| {
        egui::ScrollArea::vertical()
            .id_salt("home-log")
            .max_height(220.0)
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if app.log.is_empty() {
                    ui.label(theme::muted("Sin actividad todavía."));
                }
                for line in &app.log {
                    ui.label(RichText::new(line).monospace().size(12.0));
                }
            });
    });

    if open_dir {
        let dir = instance.game_dir(&app.paths);
        if let Err(err) = crate::core::shell::open_in_explorer(&dir) {
            app.error = Some(err.to_string());
        }
    }
    if let Some(path) = open_log {
        let dir = path
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| app.paths.logs());
        if let Err(err) = crate::core::shell::open_in_explorer(&dir) {
            app.error = Some(err.to_string());
        }
    }

    match action {
        Some(Action::Play) => app.start_play(slug),
        Some(Action::Edit) => app.open_edit(slug),
        Some(Action::Repair) => app.start_repair(slug),
        Some(Action::Delete) => match app.store.remove(slug, true, &app.paths) {
            Ok(_) => {
                app.status = format!("«{}» borrada", instance.name);
                if app.selected.as_deref() == Some(slug) {
                    app.selected = None;
                }
                app.confirm_delete = None;
            }
            Err(err) => {
                app.error = Some(err.to_string());
                app.confirm_delete = None;
            }
        },
        None => {}
    }
}
