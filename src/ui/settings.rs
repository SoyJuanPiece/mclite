//! Ajustes: cuenta offline, valores por defecto, Java y carpeta de datos.

use std::path::PathBuf;

use egui::Ui;

use crate::app::McLiteApp;
use crate::LAUNCHER_VERSION;
use crate::core::shell;
use crate::ui::theme;

/// Segundos → texto compacto para el slider de rollback ("0", "1 d", "90 d").
fn format_secs(secs: u64) -> String {
    const DAY: u64 = 86_400;
    if secs == 0 {
        "borrar ya".to_string()
    } else if secs < DAY {
        format!("{} h", (secs + 3599) / 3600)
    } else {
        format!("{} d", (secs + DAY - 1) / DAY)
    }
}

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
    let mut open_dir: Option<PathBuf> = None;
    let mut accent_change: Option<crate::ui::theme::Accent> = None;
    let mut start_update = false;
    let mut apply_premium = false;
    let mut remove_skin = false;
    let mut start_msa = false;
    let mut cancel_msa = false;
    let mut logout_msa = false;
    let ctx = ui.ctx().clone();

    egui::ScrollArea::vertical()
        .id_salt("settings")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(8.0);
            ui.label(theme::title("Ajustes"));
            ui.add_space(2.0);
            ui.label(theme::muted(
                "Haz clic en una sección para abrirla. Solo hay una abierta a la vez.",
            ));
            ui.add_space(6.0);

            // ── Actualizaciones ────────────────────────────────────────────
            let open = app.settings_open == Some("UPDATES");
            let update_hint = app
                .update_available
                .as_ref()
                .map(|(v, _)| format!("¡{v} disponible!"))
                .unwrap_or_default();
            let toggled = theme::section_toggle(ui, open, "ACTUALIZACIONES", &update_hint, |ui| {
                let mut check = app.config.check_updates;
                if ui
                    .checkbox(
                        &mut check,
                        "Buscar versiones nuevas al arrancar (GitHub)",
                    )
                    .changed()
                {
                    app.config.check_updates = check;
                    config_dirty = true;
                }
                match app.update_available.clone() {
                    Some((version, _url)) => {
                        ui.label(theme::muted(format!(
                            "Estás en {LAUNCHER_VERSION} — hay {version} disponible"
                        )));
                        let updating = app.job.is_some();
                        if ui
                            .add_enabled(
                                !updating,
                                egui::Button::new(
                                    egui::RichText::new(format!("Actualizar a {version}"))
                                        .family(theme::semibold()),
                                )
                                .fill(theme::accent())
                                .corner_radius(egui::CornerRadius::same(6)),
                            )
                            .clicked()
                        {
                            start_update = true;
                        }
                        if updating {
                            ui.spinner();
                            ui.label(theme::muted("Descargando la versión nueva…"));
                        }
                    }
                    None => {
                        ui.label(theme::muted(format!(
                            "Estás en la última versión ({LAUNCHER_VERSION})"
                        )));
                    }
                }
                ui.add_space(6.0);
                // Ventana de rollback: cuánto tiempo sobrevive el exe viejo.
                ui.horizontal(|ui| {
                    ui.label("Guardar el exe viejo:");
                    let mut keep = app.config.keep_old_secs;
                    if ui
                        .add(
                            egui::Slider::new(&mut keep, 0..=7_776_000_u64)
                                .logarithmic(true)
                                .custom_formatter(|n, _| format_secs(n as u64)),
                        )
                        .changed()
                    {
                        app.config.keep_old_secs = keep;
                        config_dirty = true;
                    }
                });
                ui.label(theme::muted(
                    "Si una actualización fallara, renombra ese .old a .exe para volver. 0 = borrar al reiniciar.",
                ));
            });
            if toggled {
                app.settings_open = if open { None } else { Some("UPDATES") };
            }

            let open = app.settings_open == Some("APPEARANCE");
            let toggled = theme::section_toggle(ui, open, "APARIENCIA", "", |ui| {
                ui.horizontal(|ui| {
                    ui.label("Color de acento:");
                    for accent in crate::ui::theme::ACCENTS {
                        let chosen = app.config.accent.as_deref()
                            == Some(accent.key())
                            || (app.config.accent.is_none() && accent == crate::ui::theme::Accent::Green);
                        // Pastilla de color; la elegida lleva anillo.
                        let (rect, response) = ui.allocate_exact_size(
                            egui::vec2(26.0, 26.0),
                            egui::Sense::click(),
                        );
                        ui.painter().circle_filled(rect.center(), 11.0, accent.color());
                        if chosen {
                            ui.painter().circle_stroke(
                                rect.center(),
                                13.0,
                                egui::Stroke::new(2.0_f32, theme::TEXT),
                            );
                        }
                        if response.clicked() {
                            accent_change = Some(accent);
                        }
                        response.on_hover_text(accent.name());
                    }
                });
            });
            if toggled {
                app.settings_open = if open { None } else { Some("APPEARANCE") };
            }

            // ── Cuenta ───────────────────────────────────────────────────────
            let open = app.settings_open == Some("ACCOUNT");
            let toggled = theme::section_toggle(ui, open, "CUENTA OFFLINE", "", |ui| {
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
                ui.add_space(6.0);
                // Preview: cara de la skin cacheada del nick (o avatar inicial).
                let nick = app.config.username_or_default();
                let cached = app
                    .paths
                    .root()
                    .join("cache")
                    .join("skins")
                    .join(format!("{}.png", crate::core::paths::sanitize(&nick)));
                if let Ok(bytes) = std::fs::read(&cached) {
                    if let Some(pixels) = crate::core::skins::face_rgba(&bytes, 6) {
                        let size = [8 * 6, 8 * 6];
                        let image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
                        let texture = ui
                            .ctx()
                            .load_texture("skin-preview", image, egui::TextureOptions::NEAREST);
                        ui.add(egui::Image::new((texture.id(), texture.size_vec2())));
                    }
                } else {
                    theme::avatar(ui, &nick, 48.0);
                }
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(&nick).strong());
                    ui.label(theme::muted(
                        "La skin la carga el mod CustomSkinLoader (botón Skins en cada instancia)",
                    ));
                    ui.horizontal(|ui| {
                        if ui.button("Usar la skin de mi nick (premium)").clicked() {
                            apply_premium = true;
                        }
                        if ui.button("Quitar skin").clicked() {
                            remove_skin = true;
                        }
                    });
                    ui.label(theme::muted(
                        "También puedes arrastrar un .png de skin a la ventana",
                    ));
                });
            });
            if toggled {
                app.settings_open = if open { None } else { Some("ACCOUNT") };
            }

            // ── Cuenta Microsoft (opcional: requiere Client ID) ─────
            let open = app.settings_open == Some("MSA");
            let msa_hint = match (&app.msa_session, &app.msa_login) {
                (Some(session), _) => session.username.clone(),
                (None, Some(login)) => format!("código {}", login.user_code),
                (None, None) => "sin cuenta".to_string(),
            };
            let toggled = theme::section_toggle(ui, open, "CUENTA MICROSOFT", &msa_hint, |ui| {
                match (&app.msa_session, &app.msa_login) {
                    (Some(session), _) => {
                        ui.label(egui::RichText::new(format!(
                            "Sesión iniciada como {}",
                            session.username
                        )).strong());
                        ui.label(theme::muted(format!(
                            "UUID: {} — el juego usa tu skin y nombre reales",
                            session.uuid
                        )));
                        ui.label(theme::muted(
                            "El token se renueva solo al lanzar el juego.",
                        ));
                        if ui.button("Cerrar sesión de Microsoft").clicked() {
                            logout_msa = true;
                        }
                    }
                    (None, Some(login)) => {
                        ui.label(theme::muted("1. Abre este enlace (clic para copiar):"));
                        if ui.link(login.verify_url.clone()).clicked() {
                            ui.ctx().copy_text(login.verify_url.clone());
                        }
                        ui.label(theme::muted("2. Escribe este código:"));
                        ui.label(
                            egui::RichText::new(&login.user_code)
                                .strong()
                                .size(22.0),
                        );
                        if ui.button("Copiar código").clicked() {
                            ui.ctx().copy_text(login.user_code.clone());
                        }
                        if ui.button("Abrir el navegador").clicked() {
                            let _ = crate::core::shell::open_url(&login.url_with_code);
                        }
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(theme::muted("Esperando tu confirmación…"));
                            if ui.button("Cancelar").clicked() {
                                cancel_msa = true;
                            }
                        });
                    }
                    (None, None) => {
                        if app.config.msa_client_id.is_some() {
                            ui.label(theme::muted(
                                "Inicia sesión para jugar con tu skin y nombre reales, y entrar a servidores con login. El login usa un código corto en el navegador.",
                            ));
                            if ui
                                .add(
                                    egui::Button::new(egui::RichText::new("Iniciar sesión con Microsoft").family(theme::semibold()))
                                        .fill(theme::accent()),
                                )
                                .clicked()
                            {
                                start_msa = true;
                            }
                        } else {
                            ui.label(theme::muted(
                                "Para usar tu cuenta de Minecraft necesitas un Client ID gratuito de Azure (5 minutos, solo la primera vez). Guía paso a paso:",
                            ));
                            ui.hyperlink_to(
                                "docs/MICROSOFT-ACCOUNT.md (guía en el repo)",
                                "https://github.com/SoyJuanPiece/mclite/blob/main/docs/MICROSOFT-ACCOUNT.md",
                            );
                            ui.add_space(4.0);
                            ui.label("Client ID:");
                            let mut client_id = app
                                .config
                                .msa_client_id
                                .clone()
                                .unwrap_or_default();
                            let response = ui.add(
                                egui::TextEdit::singleline(&mut client_id)
                                    .hint_text("00000000-0000-0000-0000-000000000000"),
                            );
                            if response.changed() {
                                app.config.msa_client_id = if client_id.trim().is_empty() {
                                    None
                                } else {
                                    Some(client_id.trim().to_string())
                                };
                                config_dirty = true;
                            }
                        }
                    }
                }
            });
            if toggled {
                app.settings_open = if open { None } else { Some("MSA") };
            }

            // ── Valores por defecto ──────────────────────────────────────────
            let open = app.settings_open == Some("DEFAULTS");
            let toggled = theme::section_toggle(ui, open, "POR DEFECTO", "", |ui| {
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
            if toggled {
                app.settings_open = if open { None } else { Some("DEFAULTS") };
            }

            // ── Java ─────────────────────────────────────────────────────────
            let open = app.settings_open == Some("JAVA");
            let toggled = theme::section_toggle(ui, open, "JAVA", "", |ui| {
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
            if toggled {
                app.settings_open = if open { None } else { Some("JAVA") };
            }

            // ── Datos ────────────────────────────────────────────────────────
            let open = app.settings_open == Some("DATA");
            let toggled = theme::section_toggle(ui, open, "DATOS", "", |ui| {
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
                        open_dir = Some(app.paths.root().to_path_buf());
                    }
                    if ui.button("Abrir logs").clicked() {
                        open_dir = Some(app.paths.logs());
                    }
                    if ui.button("Abrir crashes").clicked() {
                        open_dir = Some(app.paths.logs().join("crash"));
                    }
                });
                ui.label(theme::muted(
                    "launcher.log = arranque del launcher · crash/ = salida de cada partida",
                ));
            });
            if toggled {
                app.settings_open = if open { None } else { Some("DATA") };
            }

            ui.add_space(16.0);
        });

    if let Some(accent) = accent_change {
        app.config.accent = Some(accent.key().to_string());
        crate::ui::theme::set_accent(accent);
        crate::ui::theme::reapply_accent(&ctx);
        if app.config.save(&app.paths).is_ok() {
            app.notify(format!("Color de acento: {}", accent.name()), crate::app::ToastKind::Ok);
        }
    }
    if config_dirty && app.config.save(&app.paths).is_ok() {
        app.status = "Ajustes guardados".to_string();
    }
    if detect {
        app.detect_javas();
    }
    if start_update {
        app.start_update();
    }
    if start_msa {
        app.msa_begin_login();
    }
    if cancel_msa {
        app.msa_cancel_login();
    }
    if logout_msa {
        crate::core::msa::clear_session(&app.paths);
        app.msa_session = None;
        app.notify("Sesión de Microsoft cerrada", crate::app::ToastKind::Ok);
    }
    if apply_premium {
        app.apply_premium_skin();
    }
    if remove_skin {
        let nick = app.config.username_or_default();
        let cached = app
            .paths
            .root()
            .join("cache")
            .join("skins")
            .join(format!("{}.png", crate::core::paths::sanitize(&nick)));
        let _ = std::fs::remove_file(&cached);
        let _ = cached;
        if let Some(slug) = app.selected.clone() {
            if let Some(instance) = app.store.find(&slug) {
                let dest = instance
                    .game_dir(&app.paths)
                    .join("CustomSkinLoader")
                    .join("LocalSkin")
                    .join(format!("{}.png", crate::core::paths::sanitize(&nick)));
                let _ = std::fs::remove_file(dest);
            }
        }
        app.notify("Skin quitada", crate::app::ToastKind::Ok);
    }
    if let Some(dir) = open_dir {
        if let Err(err) = shell::open_in_explorer(&dir) {
            app.error = Some(err.to_string());
        }
    }
}


