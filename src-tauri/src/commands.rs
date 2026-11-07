//! Tauri commands. Commands never block the webview on audio, GPU or shell work.
//!
//! Typed bindings (ADR-014) arrive in Track A step A3; until then the TypeScript side mirrors
//! these shapes by hand in `src/lib/ipc.ts`.

use std::sync::Arc;

use av_protocol::{AppConfig, Report};
use serde::Serialize;

use crate::config_store::ConfigStore;

/// Identity and build facts shown in the Studio diagnostics section.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub identifier: String,
    pub os: &'static str,
    pub arch: &'static str,
    pub debug: bool,
}

#[tauri::command]
pub fn get_app_info(app: tauri::AppHandle) -> AppInfo {
    let package = app.package_info();
    AppInfo {
        name: package.name.clone(),
        version: package.version.to_string(),
        identifier: app.config().identifier.clone(),
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        debug: cfg!(debug_assertions),
    }
}

#[tauri::command]
pub fn get_config(store: tauri::State<'_, Arc<ConfigStore>>) -> AppConfig {
    store.get()
}

/// The applied (sanitized) config after a partial update, and what the pipeline changed so the
/// Studio can show where Rust clamped a value.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Applied {
    pub config: AppConfig,
    pub report: Report,
}

#[tauri::command]
pub fn patch_config(
    store: tauri::State<'_, Arc<ConfigStore>>,
    patch: serde_json::Value,
) -> Applied {
    let (config, report) = store.patch(&patch);
    Applied { config, report }
}
