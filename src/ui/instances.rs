//! Panel lateral: marca, cuenta, navegación y lista de instancias.
//!
//! Las filas se pintan a mano (painter) para tener avatar, dos líneas de texto
//! y barra de acento en la seleccionada, algo que los widgets estándar no dan.

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

pub fn show(app: &mut McLiteApp, ui: &mut Ui) {
    // Filas filtradas por el buscador (nombre o versión de MC).
    let filter = app.sidebar_search.trim().to_lowercase();
    let rows: Vec<(String, String, String, LoaderKind, Option<String>)> = app
        .store
        .instances
        .iter()
        .filter(|instance| {
            filter.is_empty()
                || instance.name.to_lowercase().contains(&filter)
                || instance.mc_version.to_lowercase().contains(&filter)
        })
        .map(|instance| {
            (
                instance.slug.clone(),
                instance.name.clone(),
                format!("{} · {}", instance.mc_version, instance.loader.label()),
                instance.loader,
                instance.icon.clone(),
            )
        })
        .collect();

    let screen = app.screen;
    let nick = app.config.username_or_default();
    let mut picked: Option<String> = None;
    let mut go_new = false;
    let mut go_home = false;
    let mut go_modpacks = false;
    let mut go_settings = false;

    egui::Frame::new()
        .fill(theme::SIDEBAR)
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            // ── Marca ────────────────────────────────────────────────────────
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
            ui.add_space(10.0);

            // ── Cuenta (clic = Ajustes) ─────────────────────────────────────
            let user = egui::Frame::new()
                .fill(theme::CARD)
                .stroke(egui::Stroke::new(1.0_f32, theme::BORDER))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        theme::avatar(ui, &nick, 30.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&nick).strong().size(14.0));
                            ui.label(theme::muted("cuenta offline"));
                        });
                    });
                })
                .response
                .interact(Sense::click());
            if user.clicked() {
                go_settings = true;
            }
            user.on_hover_text("Cambiar tu nick en Ajustes");
            ui.add_space(10.0);

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
            let create = egui::Button::new(RichText::new("+  Nueva instancia").family(theme::semibold()))
                .fill(theme::accent().gamma_multiply(0.25))
                .stroke(egui::Stroke::new(1.0_f32, theme::accent()))
                .corner_radius(CornerRadius::same(8))
                .min_size(Vec2::new(ui.available_width(), 30.0));
            if ui.add(create).clicked() {
                go_new = true;
            }

            ui.add_space(10.0);
            ui.separator();

            // ── Lista de instancias ─────────────────────────────────────────
            widgets_section(ui, &format!("INSTANCIAS ({})", rows.len()));
            if app.store.instances.len() > 4 {
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
                    if rows.is_empty() {
                        ui.add_space(4.0);
                        ui.label(theme::muted(if app.store.instances.is_empty() {
                            "Todavía no hay ninguna."
                        } else {
                            "Ninguna coincide con la búsqueda."
                        }));
                    }
                    for (slug, name, sub, loader, icon) in &rows {
                        let response = side_item(
                            ui,
                            app.selected.as_deref() == Some(slug.as_str()),
                            name,
                            Some(sub),
                            Some(widgets::loader_color(*loader)),
                            icon.as_deref(),
                            theme::TEXT,
                        );
                        if response.clicked() {
                            picked = Some(slug.clone());
                        }
                        response.on_hover_text(format!("Seleccionar «{name}»"));
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
}

/// Rótulo de sección en gris.
fn widgets_section(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).small().strong().color(theme::MUTED));
}
