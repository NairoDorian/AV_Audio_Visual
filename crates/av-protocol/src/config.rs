//! `AppConfig` schema v1: domain settings (source, analysis, performance, placements). Ephemeral
//! UI state (hovered bin, panel width) never lives here (README section 4.8).
//!
//! Every numeric field has a row in `LIMITS` (clamp + parity fixture). Adding a field means:
//! a default, a limit row if numeric, and a migration if an old field moves.

use serde::{Deserialize, Serialize};

use crate::pipeline::{Limit, Versioned, limit, slugify};
use crate::placement::{Anchor, InputMode};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub schema_version: u32,
    pub audio: AudioConfig,
    pub analysis: AnalysisConfig,
    pub performance: PerformanceConfig,
    pub overlay: OverlayConfig,
    /// Slug of the active scene.
    pub scene: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            audio: AudioConfig::default(),
            analysis: AnalysisConfig::default(),
            performance: PerformanceConfig::default(),
            overlay: OverlayConfig::default(),
            scene: "bars".into(),
        }
    }
}

/// Where audio comes from. Never silently falls back from system output to the microphone.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceKind {
    #[default]
    SystemOutput,
    Microphone,
    File,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioConfig {
    pub source: SourceKind,
    /// Stable device id; `None` follows the system default.
    pub device_id: Option<String>,
    /// Positive delays the visuals (Bluetooth outputs); ADR-016 range -100..=400 ms.
    pub av_offset_ms: f32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            source: SourceKind::SystemOutput,
            device_id: None,
            av_offset_ms: 0.0,
        }
    }
}

/// FFT presets (ADR-017 defaults; `Layered` is ADR-020).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FftPreset {
    /// Layered "Super FFT": `layers` windows of 1024 * 2^(k-1), hop 512.
    #[default]
    Layered,
    /// 1024 / 256 hop.
    LowLatency,
    /// 2048 / 512 hop (75% overlap).
    Default,
    /// 8192 / 1024 hop, 30 fps.
    Precision,
    /// 1024 / 512 hop.
    LowPower,
}

impl FftPreset {
    /// `(window_samples, hop_samples)` at 48 kHz. For `Layered` the window is the base
    /// (shortest) layer.
    pub const fn sizes(self) -> (u32, u32) {
        match self {
            Self::Layered => (1024, 512),
            Self::LowLatency => (1024, 256),
            Self::Default => (2048, 512),
            Self::Precision => (8192, 1024),
            Self::LowPower => (1024, 512),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BandLayout {
    #[default]
    Log,
    Linear,
    Mel,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisConfig {
    pub preset: FftPreset,
    /// Layer count for the `Layered` preset (1-6; 6 reaches a 32,768-sample window).
    pub layers: u32,
    /// FFT size = largest window x this (1, 2 or 4): zero-padding for every preset.
    pub zero_pad: u32,
    pub bar_count: u32,
    pub layout: BandLayout,
    pub min_hz: f32,
    pub max_hz: f32,
    pub db_floor: f32,
    pub db_ceiling: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub auto_gain: bool,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            preset: FftPreset::Layered,
            layers: 4,
            zero_pad: 2,
            bar_count: 48,
            layout: BandLayout::Log,
            min_hz: 30.0,
            max_hz: 16_000.0,
            db_floor: -72.0,
            db_ceiling: 0.0,
            attack_ms: 12.0,
            release_ms: 220.0,
            auto_gain: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceConfig {
    pub fps_cap: u32,
    /// Reduced fps/effects regardless of power source.
    pub battery_mode: bool,
    /// Seconds of silence before deep idle stops loopback.
    pub deep_idle_after_s: f32,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            fps_cap: 60,
            battery_mode: false,
            deep_idle_after_s: 30.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayConfig {
    pub enabled: bool,
    pub input: InputMode,
    /// Visible to screen capture (default) or excluded (`WDA_EXCLUDEFROMCAPTURE`).
    pub capture_visible: bool,
    pub anchor: Anchor,
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            input: InputMode::Locked,
            capture_visible: true,
            anchor: Anchor::default(),
        }
    }
}

pub static LIMITS: &[Limit<AppConfig>] = &[
    limit!(AppConfig, "analysis.layers", 1, 6, int, analysis.layers),
    limit!(AppConfig, "analysis.zeroPad", 1, 4, int, analysis.zero_pad),
    limit!(
        AppConfig,
        "audio.avOffsetMs",
        -100,
        400,
        float,
        audio.av_offset_ms
    ),
    limit!(
        AppConfig,
        "analysis.barCount",
        1,
        128,
        int,
        analysis.bar_count
    ),
    limit!(
        AppConfig,
        "analysis.minHz",
        10,
        2_000,
        float,
        analysis.min_hz
    ),
    limit!(
        AppConfig,
        "analysis.maxHz",
        500,
        24_000,
        float,
        analysis.max_hz
    ),
    limit!(
        AppConfig,
        "analysis.dbFloor",
        -120,
        -20,
        float,
        analysis.db_floor
    ),
    limit!(
        AppConfig,
        "analysis.dbCeiling",
        -40,
        12,
        float,
        analysis.db_ceiling
    ),
    limit!(
        AppConfig,
        "analysis.attackMs",
        0,
        1_000,
        float,
        analysis.attack_ms
    ),
    limit!(
        AppConfig,
        "analysis.releaseMs",
        1,
        5_000,
        float,
        analysis.release_ms
    ),
    limit!(
        AppConfig,
        "performance.fpsCap",
        15,
        240,
        int,
        performance.fps_cap
    ),
    limit!(
        AppConfig,
        "performance.deepIdleAfterS",
        2,
        3_600,
        float,
        performance.deep_idle_after_s
    ),
    limit!(
        AppConfig,
        "overlay.anchor.offsetDip.0",
        -16_384,
        16_384,
        float,
        overlay.anchor.offset_dip[0]
    ),
    limit!(
        AppConfig,
        "overlay.anchor.offsetDip.1",
        -16_384,
        16_384,
        float,
        overlay.anchor.offset_dip[1]
    ),
    limit!(
        AppConfig,
        "overlay.anchor.sizeDip.0",
        48,
        16_384,
        float,
        overlay.anchor.size_dip[0]
    ),
    limit!(
        AppConfig,
        "overlay.anchor.sizeDip.1",
        16,
        16_384,
        float,
        overlay.anchor.size_dip[1]
    ),
];

/// Smallest gap kept between the two band edges and the two dB bounds.
pub const MIN_BAND_RATIO: f32 = 2.0;
pub const MIN_DB_RANGE: f32 = 12.0;

impl Versioned for AppConfig {
    const VERSION_KEY: &'static str = "schemaVersion";
    const CURRENT: u32 = SCHEMA_VERSION;

    fn limits() -> &'static [Limit<Self>] {
        LIMITS
    }

    fn normalize(&mut self, notes: &mut Vec<String>) {
        let a = &mut self.analysis;
        if a.zero_pad == 3 {
            a.zero_pad = 4;
            notes.push("analysis.zeroPad must be 1, 2 or 4; 3 raised to 4".into());
        }
        if a.max_hz < a.min_hz * MIN_BAND_RATIO {
            a.max_hz = (a.min_hz * MIN_BAND_RATIO).min(24_000.0);
            a.min_hz = a.min_hz.min(a.max_hz / MIN_BAND_RATIO);
            notes.push("analysis.maxHz raised to keep at least one octave above minHz".into());
        }
        if a.db_ceiling < a.db_floor + MIN_DB_RANGE {
            a.db_ceiling = a.db_floor + MIN_DB_RANGE;
            notes.push("analysis.dbCeiling raised to keep a 12 dB range".into());
        }
        let slug = slugify(&self.scene, "bars");
        if slug != self.scene {
            notes.push(format!("scene id {:?} normalized to {slug:?}", self.scene));
            self.scene = slug;
        }
        if let Some(id) = &self.audio.device_id
            && id.trim().is_empty()
        {
            self.audio.device_id = None;
            notes.push("audio.deviceId empty -> follow the system default".into());
        }
    }
}
