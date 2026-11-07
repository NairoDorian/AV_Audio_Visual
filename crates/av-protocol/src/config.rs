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
    /// The one FFT size every layer is zero-padded to (a power of two, at least the largest
    /// window). Not a multiple of the window: it is the same for every layer, so their bins line up.
    pub fft_size: u32,
    pub window: WindowShape,
    /// Kaiser shape (FFT CHOP default 15).
    pub kaiser_beta: f32,
    pub bar_count: u32,
    pub layout: BandLayout,
    /// 0 = linear axis, 1 = the layout's perceptual axis (FFT CHOP "Warp"); bar edges blend both.
    pub warp_blend: f32,
    pub min_hz: f32,
    pub max_hz: f32,
    pub db_floor: f32,
    pub db_ceiling: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub auto_gain: bool,
    /// Pre-analysis shelf EQ (FFT CHOP "EQ" page).
    pub eq: EqSettings,
}

/// Analysis window shape.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WindowShape {
    /// FFT CHOP default (beta 15): low leakage with a continuous control.
    #[default]
    Kaiser,
    Hann,
    BlackmanHarris,
}

/// Shelf EQ applied to new samples before the window. Product default: high shelf +24 dB at
/// 1 kHz, Q 0.707, no low shelf.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EqSettings {
    pub enabled: bool,
    pub high_shelf: bool,
    pub high_gain_db: f32,
    pub high_cutoff_hz: f32,
    pub low_shelf: bool,
    pub low_gain_db: f32,
    pub low_cutoff_hz: f32,
    pub q: f32,
    /// Dry/wet: 0 = bypass, 1 = fully filtered.
    pub amount: f32,
}

impl Default for EqSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            high_shelf: true,
            high_gain_db: 24.0,
            high_cutoff_hz: 1000.0,
            low_shelf: false,
            low_gain_db: 0.0,
            low_cutoff_hz: 200.0,
            q: 0.707,
            amount: 1.0,
        }
    }
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            preset: FftPreset::Layered,
            layers: 4,
            fft_size: 32_768,
            window: WindowShape::Kaiser,
            kaiser_beta: 15.0,
            bar_count: 48,
            layout: BandLayout::Log,
            warp_blend: 0.93,
            min_hz: 20.0,
            max_hz: 24_000.0,
            db_floor: -72.0,
            db_ceiling: 0.0,
            attack_ms: 12.0,
            release_ms: 220.0,
            auto_gain: true,
            eq: EqSettings::default(),
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
    limit!(
        AppConfig,
        "analysis.fftSize",
        1024,
        131_072,
        int,
        analysis.fft_size
    ),
    limit!(
        AppConfig,
        "analysis.kaiserBeta",
        0,
        60,
        float,
        analysis.kaiser_beta
    ),
    limit!(
        AppConfig,
        "analysis.warpBlend",
        0,
        1,
        float,
        analysis.warp_blend
    ),
    limit!(
        AppConfig,
        "analysis.eq.highGainDb",
        -24,
        36,
        float,
        analysis.eq.high_gain_db
    ),
    limit!(
        AppConfig,
        "analysis.eq.highCutoffHz",
        20,
        20_000,
        float,
        analysis.eq.high_cutoff_hz
    ),
    limit!(
        AppConfig,
        "analysis.eq.lowGainDb",
        -24,
        36,
        float,
        analysis.eq.low_gain_db
    ),
    limit!(
        AppConfig,
        "analysis.eq.lowCutoffHz",
        20,
        20_000,
        float,
        analysis.eq.low_cutoff_hz
    ),
    limit!(AppConfig, "analysis.eq.q", 0.1, 10, float, analysis.eq.q),
    limit!(
        AppConfig,
        "analysis.eq.amount",
        0,
        2,
        float,
        analysis.eq.amount
    ),
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
        if !a.fft_size.is_power_of_two() {
            a.fft_size = a.fft_size.next_power_of_two().min(131_072);
            notes.push(format!(
                "analysis.fftSize raised to the power of two {}",
                a.fft_size
            ));
        }
        let largest = if a.preset == FftPreset::Layered {
            1024u32 << (a.layers.clamp(1, 6) - 1)
        } else {
            a.preset.sizes().0
        };
        if a.fft_size < largest {
            a.fft_size = largest;
            notes.push(format!(
                "analysis.fftSize raised to {largest}, the longest window (zero-padding needs room)"
            ));
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
