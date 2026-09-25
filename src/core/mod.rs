//! Núcleo del launcher: sin dependencias de UI.
//!
//! Todo lo que hay aquí se puede usar desde la CLI, desde la GUI o desde un test.
//! Las capas de arriba (`main.rs`, `app.rs`) solo orquestan.

pub mod assets;
pub mod auth;
pub mod config;
pub mod crash;
pub mod endpoints;
pub mod error;
pub mod hash;
pub mod http;
pub mod install;
pub mod instance;
pub mod java;
pub mod launch;
pub mod libraries;
pub mod logging;
pub mod manifest;
pub mod modrinth;
pub mod natives;
pub mod paths;
pub mod process;
pub mod progress;
pub mod rules;
pub mod runtime;
pub mod shell;
pub mod sodium;
pub mod version_json;
