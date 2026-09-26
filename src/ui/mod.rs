//! Capa de presentación (egui).
//!
//! No hay lógica del launcher aquí: los callbacks leen y escriben el estado de
//! `app::McLiteApp` y delegan en `core` / `loaders` para todo lo demás.

pub mod edit_instance;
pub mod home;
pub mod icons;
pub mod instances;
pub mod modpacks;
pub mod new_instance;
pub mod settings;
pub mod theme;
pub mod widgets;
