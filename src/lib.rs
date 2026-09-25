//! McLite — launcher lite de Minecraft.
//!
//! La lógica vive aquí (biblioteca) y las interfaces (`main.rs` CLI y la GUI egui)
//! son capas finas encima. Así el core se puede testear sin levantar ventanas.

pub mod core;
pub mod loaders;

#[cfg(feature = "gui")]
pub mod app;
#[cfg(feature = "gui")]
pub mod ui;

pub use core::error::{Error, Result};

/// Nombre y versión con los que nos presentamos a los servidores de Mojang.
pub const LAUNCHER_NAME: &str = "mclite";
pub const LAUNCHER_VERSION: &str = env!("CARGO_PKG_VERSION");
