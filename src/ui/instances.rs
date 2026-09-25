//! Panel lateral: marca, cuenta compacta, navegación e instancias agrupadas.
//!
//! Las filas se pintan a mano (painter + ui hijo) para tener avatar o icono,
//! dos líneas de texto y barra de acento en la seleccionada. Las instancias
//! nacidas de un pack de Modrinth se agrupan en su propia sección.

use egui::{Color32, CornerRadius, RichText, Sense, Ui, Vec2};

use crate::LAUNCHER_VERSION;

use crate::app::{McLiteApp, Screen};
use crate::loaders::LoaderKind;
use crate::ui::{theme, widgets};

/// Fila del lateral. Se pinta el fondo a mano y el contenido con un ui hijo
/// anclado al rect (permite mezclar imagen de icono y textos). `dot` añade un
/// círculo de color; `icon` una miniatura (URL, p. ej. pack de Modrinth).
fn side_item(
    ui: &mut Ui,
    selected: bool,
    title: &str,
    sub: Option<&str>,
    dot: Option<Color32>,
    icon: Option<&str>,
    title_color: Color32,
) -> egui::Response {
    let height = if sub.is_some() { 44.0 } else { 32.0 };
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());

    // Fondo: seleccionado = tinte de acento; hover = tarjeta elevada; resto = transparente.
    let fill = if selected {
        theme::accent().gamma_multiply(0.20)
    } else if response.hovered() {
        theme::CARD_ELEVATED
    } else {
        Color32::TRANSPARENT
    };
    if fill != Color32::TRANSPARENT {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(8), fill);
    }
    if selected {
        // Barra de acento a la izquierda, como los launchers modernos.
        let bar = egui::Rect::from_min_max(
            [rect.left(), rect.top() + 6.0].into(),
            [rect.left() + 3.0, rect.bottom() - 6.0].into(),
        );
        ui.painter()
            .rect_filled(bar, CornerRadius::same(2), theme::accent_soft());
    }

    // Contenido, sobre el rect reservado.
    let mut row = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    row.horizontal_centered(|ui| {
        ui.add_space(12.0);
        if let Some(url) = icon {
            ui.add(
                egui::Image::from_uri(url)
                    .max_size(Vec2::new(26.0, 26.0))
                    .corner_radius(CornerRadius::same(6)),
            );
            ui.add_space(8.0);
        } else if let Some(color) = dot {
            let (dot_rect, _) = ui.allocate_exact_size(Vec2::new(10.0, 10.0), Sense::hover());
            ui.painter()
                .circle_filled(dot_rect.center(), 4.5, color);
            ui.add_space(6.0);
        }
        match sub {
            None => {
                ui.label(RichText::new(title).size(14.5).color(title_color));
            }
            Some(sub) => {
                ui.vertical(|ui| {
                    ui.label(RichText::new(title).size(14.0).color(title_color));
                    ui.label(RichText::new(sub).size(11.5).color(theme::MUTED));
                });
            }
        }
    });

    response
}

/// Datos de una fila de instancia ya filtrada.
struct InstanceRow {
    slug: String,
    name: String,
    sub: String,
    loader: LoaderKind,
    icon: Option<String>,
    selected: bool,
}

pub fn show(app: &mut McLiteApp, ui: &mut Ui) {
    // Filtrado por nombre o versión de MC (insensible a mayúsculas).
    let filter = app.sidebar_search.trim().to_lowercase();
    let matches = |name: &str, mc: &str| {
        filter.is_empty() || name.to_lowercase().contains(&filter) || mc.to_lowercase().contains(&filter)
    };

    let mut regular: Vec<InstanceRow> = Vec::new();
    let mut packs: Vec<InstanceRow> = Vec::new();
    for instance in &app.store.instances {
        if !matches(&instance.name, &instance.mc_version) {
            continue;
        }
        // El icono pasa por la caché de disco: 2ª sesión = instantáneo y offline.
        let icon = instance
            .icon
            .as_deref()
            .map(|url| crate::core::icons::resolve(&app.paths, url));
        let row = InstanceRow {
            slug: instance.slug.clone(),
            name: instance.name.clone(),
            sub: format!("{} · {}", instance.mc_version, instance.loader.label()),
            loader: instance.loader,
            icon,
            selected: app.selected.as_deref() == Some(instance.slug.as_str()),
        };
        if instance.from_pack.is_some() {
            packs.push(row);
        } else {
            regular.push(row);
        }
    }

    let screen = app.screen;
    let nick = app.config.username_or_default();
    let mut picked: Option<String> = None;
    let mut go_new = false;
    let mut go_home = false;
    let mut go_modpacks = false;
    let mut go_settings = false;
    // Doble clic sobre una fila = JUGAR esa instancia.
    let mut play: Option<String> = None;
    // Menú contextual abierto (slug): se muestra tras el frame.
    let mut menu_for: Option<String> = None;

    egui::Frame::new()
        .fill(theme::SIDEBAR)
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            // ── Marca + cuenta, en una fila compacta ─────────────────────────
            ui.horizontal(|ui| {
                theme::grass_block(ui, 26.0);
                ui.label(
                    RichText::new("McLite")
                        .size(19.0)
                        .family(theme::semibold())
                        .color(theme::TEXT),
                );
                ui.label(theme::muted(format!("v{LAUNCHER_VERSION}")));
            });
            ui.add_space(6.0);

            // Cuenta: avatar pequeño + nick; el clic lleva a Ajustes.
            let user = ui
                .horizontal(|ui| {
                    theme::avatar(ui, &nick, 18.0);
                    ui.label(RichText::new(&nick).size(12.5).color(theme::MUTED));
                })
                .response
                .interact(Sense::click());
            if user.clicked() {
                go_settings = true;
            }
            user.on_hover_text(format!("Cuenta offline de {nick} — clic para cambiar el nick"));
            ui.add_space(8.0);

            // ── Navegación ──────────────────────────────────────────────────
            if side_item(ui, screen == Screen::Home, "▶  Jugar", None, None, None, theme::TEXT)
                .clicked()
            {
                go_home = true;
            }
            if side_item(
                ui,
                screen == Screen::Modpacks,
                "■  Modpacks",
                None,
                None,
                None,
                theme::TEXT,
            )
            .clicked()
            {
                go_modpacks = true;
            }
            if side_item(
                ui,
                screen == Screen::Settings,
                "⚙  Ajustes",
                None,
                None,
                None,
                theme::TEXT,
            )
            .clicked()
            {
                go_settings = true;
            }
            // Acción destacada, no navegación.
            let create =
                egui::Button::new(RichText::new("+  Nueva instancia").family(theme::semibold()))
                    .fill(theme::accent().gamma_multiply(0.25))
                    .stroke(egui::Stroke::new(1.0_f32, theme::accent()))
                    .corner_radius(CornerRadius::same(8))
                    .min_size(Vec2::new(ui.available_width(), 30.0));
            if ui.add(create).clicked() {
                go_new = true;
            }

            ui.add_space(8.0);

            // ── Lista agrupada ──────────────────────────────────────────────
            let total = regular.len() + packs.len();
            widgets_section(ui, &format!("INSTANCIAS ({total})"));
            if total > 4 {
                ui.add(
                    egui::TextEdit::singleline(&mut app.sidebar_search)
                        .hint_text("Buscar…")
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(2.0);
            }

            egui::ScrollArea::vertical()
                .id_salt("sidebar-instances")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if total == 0 {
                        ui.add_space(4.0);
                        ui.label(theme::muted(if app.store.instances.is_empty() {
                            "Todavía no hay ninguna."
                        } else {
                            "Ninguna coincide con la búsqueda."
                        }));
                        return;
                    }

                    // Sección 1: las que creaste a mano.
                    if !regular.is_empty() {
                        widgets_section(ui, &format!("TUS INSTANCIAS ({})", regular.len()));
                        ui.add_space(2.0);
                    }
                    for row in &regular {
                        handle_instance_row(ui, row, &mut picked, &mut play, &mut menu_for);
                    }

                    // Sección 2: modpacks instalados desde Modrinth.
                    if !packs.is_empty() {
                        ui.add_space(6.0);
                        widgets_section(ui, &format!("MODPACKS ({})", packs.len()));
                        ui.add_space(2.0);
                    }
                    for row in &packs {
                        handle_instance_row(ui, row, &mut picked, &mut play, &mut menu_for);
                    }
                });
        });

    if go_home {
        app.screen = Screen::Home;
        app.confirm_delete = None;
    }
    if go_new {
        app.open_new();
    }
    if go_modpacks {
        app.screen = Screen::Modpacks;
        // Primera vez que se abre: populares sin escribir nada.
        if app.packs.is_empty() && !app.packs_loading {
            app.search_packs();
        }
    }
    if go_settings {
        app.screen = Screen::Settings;
    }
    if let Some(slug) = picked {
        app.selected = Some(slug);
        app.screen = Screen::Home;
        app.confirm_delete = None;
    }
    if let Some(slug) = play {
        // Doble clic: seleccionar y lanzar.
        app.selected = Some(slug.clone());
        app.screen = Screen::Home;
        app.start_play(&slug);
    }
    if let Some(slug) = menu_for {
        // Menú contextual: Jugar / Editar / Reparar / Carpeta / Borrar.
        app.selected = Some(slug.clone());
        app.screen = Screen::Home;
        let delete_label = if app.confirm_delete.as_deref() == Some(slug.as_str()) {
            "¿Borrar de verdad?"
        } else {
            "Borrar"
        };
        egui::Area::new(egui::Id::new("ctx-menu"))
            .fixed_pos(ui.input(|i| i.pointer.latest_pos().unwrap_or_default()))
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(theme::CARD_ELEVATED)
                    .stroke(egui::Stroke::new(1.0_f32, theme::BORDER))
                    .corner_radius(egui::CornerRadius::same(8))
                    .inner_margin(egui::Margin::same(4))
                    .show(ui, |ui| {
                        ui.set_min_width(150.0);
                        if ui.button("▶  Jugar").clicked() {
                            app.start_play(&slug);
                        }
                        if ui.button("Editar").clicked() {
                            app.open_edit(&slug);
                        }
                        if ui.button("Reparar").clicked() {
                            app.start_repair(&slug);
                        }
                        if ui.button("Carpeta").clicked() {
                            if let Some(instance) = app.store.find(&slug) {
                                let dir = instance.game_dir(&app.paths);
                                if let Err(err) = crate::core::shell::open_in_explorer(&dir) {
                                    app.error = Some(err.to_string());
                                }
                            }
                        }
                        ui.separator();
                        if ui.button(RichText::new(delete_label).color(theme::DANGER)).clicked() {
                            if app.confirm_delete.as_deref() == Some(slug.as_str()) {
                                match app.store.remove(&slug, true, &app.paths) {
                                    Ok(_) => {
                                        app.selected = None;
                                        app.confirm_delete = None;
                                        app.notify("Instancia borrada", crate::app::ToastKind::Ok);
                                    }
                                    Err(err) => app.error = Some(err.to_string()),
                                }
                            } else {
                                app.confirm_delete = Some(slug.clone());
                            }
                        }
                    });
            });
    }
}

/// Una fila de instancia: selección con clic, JUGAR con doble clic y menú
/// contextual con clic derecho. Encapsulado porque se usa en ambas secciones.
fn handle_instance_row(
    ui: &mut Ui,
    row: &InstanceRow,
    picked: &mut Option<String>,
    play: &mut Option<String>,
    menu_for: &mut Option<String>,
) {
    let response = side_item(
        ui,
        row.selected,
        &row.name,
        Some(&row.sub),
        Some(widgets::loader_color(row.loader)),
        row.icon.as_deref(),
        theme::TEXT,
    );
    if response.clicked() {
        *picked = Some(row.slug.clone());
    }
    if response.double_clicked() {
        *play = Some(row.slug.clone());
    }
    if response.secondary_clicked() {
        *menu_for = Some(row.slug.clone());
    }
    response.on_hover_text(format!(
        "Seleccionar «{}» (doble clic para jugar, clic derecho para más)",
        row.name
    ));
}

/// Rótulo de sección en gris.
fn widgets_section(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).small().strong().color(theme::MUTED));
}
