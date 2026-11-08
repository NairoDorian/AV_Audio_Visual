//! Tauri commands. Commands never block the webview on audio, GPU or shell work.
//!
//! Typed bindings (ADR-014) arrive in Track A step A3; until then the TypeScript side mirrors
//! these shapes by hand in `src/lib/ipc.ts`.

use std::sync::{Arc, Mutex};

use av_protocol::{AppConfig, Report, Scene};
use serde::Serialize;

use crate::audio::{AudioService, AudioStatus, SourceInfo, SourceRequest};
use crate::config_store::ConfigStore;
use crate::displays::{DisplayState, Displays, Intent, Placement};
use crate::presets::{PresetInfo, PresetStore};
use crate::scenes::SceneStore;

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

/// The built-in defaults (the Studio shows them in the info bubbles and resets to them).
#[tauri::command]
pub fn get_defaults() -> AppConfig {
    AppConfig::default()
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
    displays: tauri::State<'_, Arc<Mutex<Displays>>>,
    scenes: tauri::State<'_, Arc<SceneStore>>,
    patch: serde_json::Value,
) -> Applied {
    apply_patch(&store, &audio, &displays, &scenes, &patch)
}

fn apply_patch(
    store: &ConfigStore,
    audio: &AudioService,
    displays: &Mutex<Displays>,
    scenes: &SceneStore,
    patch: &serde_json::Value,
) -> Applied {
    let before = store.get();
    let (config, report) = store.patch(patch);
    if config.analysis != before.analysis {
        audio.reconfigure(&config.analysis);
    }
    if config.overlay != before.overlay
        || config.background != before.background
        || config.taskbar != before.taskbar
        || config.scene != before.scene
    {
        apply_displays(displays, &config, scenes);
    }
    Applied { config, report }
}

/// Sends the displays what the config enables, each with its resolved scene.
pub fn apply_displays(displays: &Mutex<Displays>, config: &AppConfig, scenes: &SceneStore) {
    let pick = |id: &str| scenes.get(if id.is_empty() { &config.scene } else { id });
    let intent = Intent {
        config: config.clone(),
        scenes: vec![
            (Placement::Background, pick(&config.background.scene)),
            (Placement::Taskbar, pick(&config.taskbar.scene)),
            (Placement::Overlay, pick(&config.overlay.scene)),
        ],
    };
    if let Ok(d) = displays.lock() {
        d.apply(intent);
    }
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

/// Every scene: the built-ins (with any saved edit) and the user's own.
#[tauri::command]
pub fn list_scenes(scenes: tauri::State<'_, Arc<SceneStore>>) -> Vec<Scene> {
    scenes.list()
}

/// Saves a scene (an edited built-in keeps its id) and redraws the displays that use it.
#[tauri::command]
pub fn save_scene(
    store: tauri::State<'_, Arc<ConfigStore>>,
    scenes: tauri::State<'_, Arc<SceneStore>>,
    displays: tauri::State<'_, Arc<Mutex<Displays>>>,
    scene: Scene,
) -> Result<Scene, String> {
    let saved = scenes.save(&scene)?;
    apply_displays(&displays, &store.get(), &scenes);
    Ok(saved)
}

/// Deletes a saved scene: an edited built-in returns to its original look.
#[tauri::command]
pub fn reset_scene(
    store: tauri::State<'_, Arc<ConfigStore>>,
    scenes: tauri::State<'_, Arc<SceneStore>>,
    displays: tauri::State<'_, Arc<Mutex<Displays>>>,
    id: String,
) -> Result<Scene, String> {
    scenes.reset(&id)?;
    apply_displays(&displays, &store.get(), &scenes);
    Ok(scenes.get(&id))
}

/// True when `id` is a built-in scene with saved edits (the Studio offers to reset it).
#[tauri::command]
pub fn scene_is_modified(scenes: tauri::State<'_, Arc<SceneStore>>, id: String) -> bool {
    scenes.is_modified(&id)
}

/// A monitor for the Studio's pickers.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorInfo {
    pub id: String,
    pub width: i32,
    pub height: i32,
    pub scale: f32,
    pub primary: bool,
}

/// The connected monitors (stable ids). Async: it waits on the host thread.
#[tauri::command]
pub async fn list_monitors() -> Result<Vec<MonitorInfo>, String> {
    let hosts = av_platform::Hosts::start()?;
    Ok(hosts
        .monitors()
        .into_iter()
        .map(|m| MonitorInfo {
            id: m.id,
            width: m.rect.w,
            height: m.rect.h,
            scale: m.scale,
            primary: m.primary,
        })
        .collect())
}

/// What each enabled display is doing (running, starting, or why it failed).
#[tauri::command]
pub fn get_display_states(displays: tauri::State<'_, Arc<Mutex<Displays>>>) -> Vec<DisplayState> {
    displays.lock().map(|d| d.states()).unwrap_or_default()
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
    displays: tauri::State<'_, Arc<Mutex<Displays>>>,
    scenes: tauri::State<'_, Arc<SceneStore>>,
    id: String,
) -> Result<Applied, String> {
    let preset = presets.get(&id).ok_or("no such preset")?;
    let analysis = serde_json::to_value(&preset.analysis).map_err(|e| e.to_string())?;
    Ok(apply_patch(
        &store,
        &audio,
        &displays,
        &scenes,
        &serde_json::json!({ "analysis": analysis }),
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
pub fn get_active_scene(
    store: tauri::State<'_, Arc<ConfigStore>>,
    scenes: tauri::State<'_, Arc<SceneStore>>,
) -> Scene {
    scenes.get(&store.get().scene)
}
