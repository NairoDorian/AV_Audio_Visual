//! Tauri commands. Commands never block the webview on audio, GPU or shell work.
//!
//! Typed bindings (ADR-014) arrive in Track A step A3; until then the TypeScript side mirrors
//! these shapes by hand in `src/lib/ipc.ts`.

use std::sync::Arc;

use av_protocol::{AppConfig, Report, Scene};
use serde::Serialize;

use crate::audio::{AudioService, AudioStatus, SourceInfo, SourceRequest};
use crate::config_store::ConfigStore;
use crate::presets::{PresetInfo, PresetStore};

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
    audio: tauri::State<'_, Arc<AudioService>>,
    patch: serde_json::Value,
) -> Applied {
    let before = store.get();
    let (config, report) = store.patch(&patch);
    if config.analysis != before.analysis {
        audio.reconfigure(&config.analysis);
    }
    Applied { config, report }
}

/// Starts the demo track or an audio file, replacing the current source.
#[tauri::command]
pub async fn start_source(
    store: tauri::State<'_, Arc<ConfigStore>>,
    audio: tauri::State<'_, Arc<AudioService>>,
    request: SourceRequest,
) -> Result<SourceInfo, String> {
    audio.start(request, &store.get().analysis)
}

#[tauri::command]
pub async fn stop_source(audio: tauri::State<'_, Arc<AudioService>>) -> Result<(), String> {
    audio.stop();
    Ok(())
}

#[tauri::command]
pub async fn get_audio_status(
    audio: tauri::State<'_, Arc<AudioService>>,
) -> Result<AudioStatus, String> {
    Ok(audio.status())
}

/// Raw wire bytes for the preview (ADR-015: pull, never pushed as events). A header-only reply
/// when `last_sequence` is current. Async so the encode never runs on the main thread.
#[tauri::command]
pub async fn get_frame(
    audio: tauri::State<'_, Arc<AudioService>>,
    last_sequence: Option<u32>,
    want_bins: bool,
) -> Result<tauri::ipc::Response, String> {
    Ok(tauri::ipc::Response::new(
        audio.frame(last_sequence, want_bins),
    ))
}

/// Built-in scenes (generated in Rust; user scene files arrive in Phase 3).
#[tauri::command]
pub fn list_scenes() -> Vec<Scene> {
    av_protocol::scene::builtin_scenes()
}

/// The FFT CHOP's Reset: clears the filter, ballistics, AGC and feature state.
#[tauri::command]
pub fn reset_analysis(audio: tauri::State<'_, Arc<AudioService>>) {
    audio.reset();
}

/// The built-in default and every user preset.
#[tauri::command]
pub fn list_presets(presets: tauri::State<'_, Arc<PresetStore>>) -> Vec<PresetInfo> {
    presets.list()
}

/// Saves the current analysis settings as a named preset (same name overwrites).
#[tauri::command]
pub fn save_preset(
    store: tauri::State<'_, Arc<ConfigStore>>,
    presets: tauri::State<'_, Arc<PresetStore>>,
    name: String,
) -> Result<PresetInfo, String> {
    presets.save(&name, store.get().analysis)
}

/// Copies a preset's settings into the config. Nothing keeps tracking the preset afterwards:
/// every parameter stays editable.
#[tauri::command]
pub fn apply_preset(
    store: tauri::State<'_, Arc<ConfigStore>>,
    audio: tauri::State<'_, Arc<AudioService>>,
    presets: tauri::State<'_, Arc<PresetStore>>,
    id: String,
) -> Result<Applied, String> {
    let preset = presets.get(&id).ok_or("no such preset")?;
    let analysis = serde_json::to_value(&preset.analysis).map_err(|e| e.to_string())?;
    Ok(patch_config(
        store,
        audio,
        serde_json::json!({ "analysis": analysis }),
    ))
}

#[tauri::command]
pub fn delete_preset(
    presets: tauri::State<'_, Arc<PresetStore>>,
    id: String,
) -> Result<(), String> {
    presets.delete(&id)
}

/// The scene the config selects, falling back to the default scene.
#[tauri::command]
pub fn get_active_scene(store: tauri::State<'_, Arc<ConfigStore>>) -> Scene {
    av_protocol::scene::builtin(&store.get().scene)
}
