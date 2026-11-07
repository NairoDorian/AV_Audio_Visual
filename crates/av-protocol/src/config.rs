//! `AppConfig` schema v3: domain settings (source, analysis, performance, placements). Ephemeral
//! UI state (hovered bin, panel width) never lives here (README section 4.8).
//!
//! Every numeric field has a row in `LIMITS` (clamp + parity fixture). Adding a field means:
//! a default, a limit row if numeric, and a migration if an old field moves.
//!
//! Schema v3 (2026-11-08) follows FFT CHOP v2.13's Multi-FFT controls: `layerRefresh` and
//! `layerThreads` (which replaces `parallelLayers`, [`migrate_v2`]).
//!
//! Schema v2 (2026-11-07) replaced the fixed FFT presets with every parameter of the FFT CHOP
//! (Spectrum, EQ, Window & Weighting, Loudness & Ballistics, Performance pages), each visible and
//! adjustable on its own; named sets of them are user presets (`crate::preset`). Migration from
//! v1 is [`migrate_v1`].

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::pipeline::{Limit, Migration, Versioned, limit, slugify};
use crate::placement::{Anchor, InputMode};

pub const SCHEMA_VERSION: u32 = 3;

/// Largest analysis FFT (zero-padded transform length).
pub const MAX_FFT_SIZE: u32 = 131_072;
/// Smallest analysis FFT the size menu offers.
pub const MIN_FFT_SIZE: u32 = 1024;
/// Longest window any layer may use (FFT CHOP `kMaxWinSamples`).
pub const MAX_WINDOW: u32 = 65_536;
pub const MIN_WINDOW: u32 = 16;
pub const MAX_LAYERS: u32 = 6;
/// Output bins: the CHOP's hard minimum, and `MAX_FFT_SIZE / 2 + 1` so Raw RFFT Bins always fit.
pub const MIN_OUTPUT_BINS: u32 = 8;
pub const MAX_OUTPUT_BINS: u32 = MAX_FFT_SIZE / 2 + 1;

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

// ---------------------------------------------------------------------------------------------
// Analysis (FFT CHOP parameter pages). Menu enums keep the CHOP's order.
// ---------------------------------------------------------------------------------------------

/// How the source's channels become the one analysed signal (CHOP "Channels").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChannelMode {
    /// Average of every channel (sum / count, so more channels do not read louder).
    #[default]
    MonoMix,
    /// First channel only.
    Left,
    /// Second channel (the first when the source is mono).
    Right,
}

/// Frequency axis of the output bins (CHOP "Scale").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Scale {
    /// Logarithmic from the log floor.
    #[default]
    Log,
    /// HTK mel.
    Mel,
    /// Moore & Glasberg ERB bandwidth polynomial used as a warp.
    Erb,
    /// Traunmuller Bark.
    Bark,
    /// Pitch (semitones), from a fixed 20 Hz.
    Chroma,
    Linear,
    /// 50/50 blend of Mel and Log.
    Melog,
}

/// Fixed = `outputBins` values; Auto = the transform's own `N/2 + 1`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OutputBinsMode {
    #[default]
    Fixed,
    Auto,
}

/// Kernel for an output bin that falls between two FFT bins.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WarpInterp {
    /// 2 taps.
    #[default]
    Linear,
    /// Catmull-Rom, 4 taps: smoother lobes when the output grid is finer than the FFT's.
    Cubic,
}

/// What an output bin reports when it owns two or more FFT bins (the coarse end of the axis).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WarpAggregation {
    /// Interpolate (narrow peaks between taps can be skipped).
    Off,
    /// Largest FFT bin in the range: no peak is ever dropped.
    #[default]
    Peak,
    /// Power mean of the range: energy-faithful.
    Rms,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WindowLengthMode {
    #[default]
    Samples,
    Milliseconds,
}

/// Analysis window (CHOP "Window Type").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WindowShape {
    /// Continuous leakage control (beta).
    #[default]
    Kaiser,
    Hann,
    Hamming,
    Blackman,
    /// 4-term, 92 dB sidelobes.
    BlackmanHarris,
    /// No taper.
    Rectangular,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KaiserBetaMode {
    #[default]
    Manual,
    /// The smallest beta whose sidelobes sit below the dB range floor.
    Auto,
}

/// Equal-loudness weighting of the output bins.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Weighting {
    #[default]
    Off,
    /// IEC 61672 A.
    A,
    /// IEC 61672 C.
    C,
    /// ITU-R 468 noise weighting.
    Itu468,
}

/// The scale a bin's magnitude is normalized to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MagnitudeNorm {
    /// `mean(window) == 1`: a full-scale sine reads `window / 2` (CHOP default).
    #[default]
    CoherentGain,
    /// `sum(window) == 2`: a full-scale sine reads 1.
    FullScale,
}

/// How the display value is formed from the magnitude.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoudnessMode {
    /// Linear magnitude relative to the reference (0..1).
    Off,
    /// Decibels relative to the reference, the dB range mapped onto 0..1 (the CHOP's "dB" and
    /// "dB Normalized" draw the same; frames always carry display values).
    #[default]
    Db,
}

/// What full height means.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DbReference {
    /// This frame's loudest bin.
    FramePeak,
    /// Absolute: a full-scale sine reaches the top.
    Dbfs,
    /// A slow peak follower.
    #[default]
    Agc,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BallisticsMode {
    /// Per-hop smoothing factor.
    Coefficient,
    /// Time constants, independent of the hop rate.
    #[default]
    Milliseconds,
}

/// How often the long layers are recomputed (FFT CHOP v2.13 "Multi-FFT Long-Layer Refresh").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LayerRefresh {
    /// Every layer on every hop (exact).
    #[default]
    EveryFrame,
    /// Layer 4 every 2nd hop, layer 5 every 4th, layer 6 every 8th (phase-offset); their previous
    /// magnitude is reused in between. Long windows change slowly, so this costs little detail.
    HopScaled,
}

/// The analysis settings: every FFT CHOP parameter that applies to a visualizer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisConfig {
    // --- Spectrum ---
    pub channel_mode: ChannelMode,
    /// The rfft magnitudes untouched: `N/2 + 1` bins DC..Nyquist (overrides the axis settings).
    pub raw_bins: bool,
    pub scale: Scale,
    /// Top of the axis; clamped to Nyquist at run time.
    pub display_max_hz: f32,
    pub output_bins_mode: OutputBinsMode,
    /// Output bins in Fixed mode; may exceed `N/2 + 1` (interpolated).
    pub output_bins: u32,
    /// 0 = linear axis, 1 = the scale's own axis.
    pub warp_blend: f32,
    pub warp_interp: WarpInterp,
    pub warp_aggregation: WarpAggregation,
    /// Bottom of the Log / Melog axis.
    pub log_floor_hz: f32,
    pub window_length_mode: WindowLengthMode,
    /// Window of the first (shortest) layer, samples.
    pub window_samples: u32,
    /// Window of the first layer when the mode is Milliseconds.
    pub window_ms: f32,
    /// Layered "Super FFT" (ADR-020): 1 = one window; each extra layer doubles it.
    pub layers: u32,
    /// Samples between analyses (the frame rate is `rate / hop`).
    pub hop_samples: u32,
    /// Off: the transform is the longest window itself and `fftSize` is ignored.
    pub zero_padding: bool,
    /// The one FFT size of every layer (power of two, grown to the longest window). Not a
    /// multiple of the window: every layer shares it, so their bins line up.
    pub fft_size: u32,
    // --- EQ ---
    pub eq: EqSettings,
    // --- Window & Weighting ---
    pub window: WindowShape,
    pub kaiser_beta_mode: KaiserBetaMode,
    pub kaiser_beta: f32,
    pub weighting: Weighting,
    pub magnitude_norm: MagnitudeNorm,
    // --- Loudness & Ballistics ---
    pub loudness: LoudnessMode,
    pub db_reference: DbReference,
    /// Floor of the display, dB below the reference.
    pub db_range: f32,
    /// How fast the AGC reference falls after loud passages.
    pub agc_release_ms: f32,
    pub ballistics: bool,
    pub ballistics_mode: BallisticsMode,
    /// Per-hop attack coefficient, 0 = instant.
    pub attack: f32,
    pub release: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    // --- Performance ---
    /// Centroid, rolloff, flatness, flux, RMS and band powers in the status.
    pub spectral_features: bool,
    pub layer_refresh: LayerRefresh,
    /// Threads the layers are spread over, the analysis thread included: 0 = auto (one per layer
    /// from 3 layers up), 1 = all on the analysis thread, n = n threads. Results are identical at
    /// any count.
    pub layer_threads: u32,
    /// Raw audio scope: the newest samples, half-sine tapered, sent with every frame (0 = off;
    /// 800 is one 60 fps frame at 48 kHz). docs/super-fft.md section 4.
    pub waveform_samples: u32,
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
    /// Dry/wet: 0 = bypass, 1 = fully filtered, above 1 over-applied (CHOP slider to 5).
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
    /// FFT CHOP defaults, ZER0's visual defaults for loudness and ballistics, and the product
    /// owner's overrides (layered 4 x 1,024, FFT 32,768, warp 0.93, +24 dB high shelf).
    fn default() -> Self {
        Self {
            channel_mode: ChannelMode::MonoMix,
            raw_bins: false,
            scale: Scale::Log,
            display_max_hz: 24_000.0,
            output_bins_mode: OutputBinsMode::Fixed,
            output_bins: 1024,
            warp_blend: 0.93,
            warp_interp: WarpInterp::Linear,
            warp_aggregation: WarpAggregation::Peak,
            log_floor_hz: 20.0,
            window_length_mode: WindowLengthMode::Samples,
            window_samples: 1024,
            window_ms: 21.333,
            layers: 4,
            hop_samples: 512,
            zero_padding: true,
            fft_size: 32_768,
            eq: EqSettings::default(),
            window: WindowShape::Kaiser,
            kaiser_beta_mode: KaiserBetaMode::Manual,
            kaiser_beta: 15.0,
            weighting: Weighting::Off,
            magnitude_norm: MagnitudeNorm::CoherentGain,
            loudness: LoudnessMode::Db,
            db_reference: DbReference::Agc,
            db_range: 80.0,
            agc_release_ms: 1500.0,
            ballistics: true,
            ballistics_mode: BallisticsMode::Milliseconds,
            attack: 0.0,
            release: 0.0,
            attack_ms: 15.0,
            release_ms: 250.0,
            spectral_features: false,
            waveform_samples: 800,
            layer_refresh: LayerRefresh::EveryFrame,
            layer_threads: 0,
        }
    }
}

impl AnalysisConfig {
    /// Longest window in samples mode (`window_samples * 2^(layers-1)`).
    pub fn largest_window(&self) -> u32 {
        self.window_samples
            .saturating_mul(1 << (self.layers.clamp(1, MAX_LAYERS) - 1))
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

/// The analysis rows of [`LIMITS`], also used for presets. Ranges are the FFT CHOP's hard
/// limits (wider than its sliders), except where this engine's own bounds are tighter.
macro_rules! analysis_limits {
    ($ty:ty, $($root:tt)+) => {
        [
            limit!($ty, "analysis.displayMaxHz", 100, 192_000, float, $($root)+.display_max_hz),
            limit!($ty, "analysis.outputBins", $crate::config::MIN_OUTPUT_BINS, $crate::config::MAX_OUTPUT_BINS, int, $($root)+.output_bins),
            limit!($ty, "analysis.warpBlend", 0, 1, float, $($root)+.warp_blend),
            limit!($ty, "analysis.logFloorHz", 1, 5_000, float, $($root)+.log_floor_hz),
            limit!($ty, "analysis.windowSamples", $crate::config::MIN_WINDOW, $crate::config::MAX_WINDOW, int, $($root)+.window_samples),
            limit!($ty, "analysis.windowMs", 0.5, 5_000, float, $($root)+.window_ms),
            limit!($ty, "analysis.layers", 1, $crate::config::MAX_LAYERS, int, $($root)+.layers),
            limit!($ty, "analysis.hopSamples", 32, 16_384, int, $($root)+.hop_samples),
            limit!($ty, "analysis.fftSize", $crate::config::MIN_FFT_SIZE, $crate::config::MAX_FFT_SIZE, int, $($root)+.fft_size),
            limit!($ty, "analysis.eq.highGainDb", -24, 36, float, $($root)+.eq.high_gain_db),
            limit!($ty, "analysis.eq.highCutoffHz", 20, 20_000, float, $($root)+.eq.high_cutoff_hz),
            limit!($ty, "analysis.eq.lowGainDb", -24, 36, float, $($root)+.eq.low_gain_db),
            limit!($ty, "analysis.eq.lowCutoffHz", 20, 20_000, float, $($root)+.eq.low_cutoff_hz),
            limit!($ty, "analysis.eq.q", 0.1, 10, float, $($root)+.eq.q),
            limit!($ty, "analysis.eq.amount", 0, 5, float, $($root)+.eq.amount),
            limit!($ty, "analysis.kaiserBeta", 0, 100, float, $($root)+.kaiser_beta),
            limit!($ty, "analysis.dbRange", 10, 160, float, $($root)+.db_range),
            limit!($ty, "analysis.agcReleaseMs", 10, 60_000, float, $($root)+.agc_release_ms),
            limit!($ty, "analysis.attack", 0, 0.99, float, $($root)+.attack),
            limit!($ty, "analysis.release", 0, 0.99, float, $($root)+.release),
            limit!($ty, "analysis.attackMs", 0, 2_000, float, $($root)+.attack_ms),
            limit!($ty, "analysis.releaseMs", 0, 5_000, float, $($root)+.release_ms),
            limit!($ty, "analysis.waveformSamples", 0, $crate::frame::MAX_WAVEFORM, int, $($root)+.waveform_samples),
            limit!($ty, "analysis.layerThreads", 0, $crate::config::MAX_LAYERS, int, $($root)+.layer_threads),
        ]
    };
}
pub(crate) use analysis_limits;

static ANALYSIS_LIMITS: [Limit<AppConfig>; 24] = analysis_limits!(AppConfig, analysis);

static OTHER_LIMITS: [Limit<AppConfig>; 7] = [
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

/// Every numeric row: analysis first, then the rest.
pub static LIMITS: std::sync::LazyLock<Vec<Limit<AppConfig>>> = std::sync::LazyLock::new(|| {
    ANALYSIS_LIMITS
        .iter()
        .chain(OTHER_LIMITS.iter())
        .copied()
        .collect()
});

/// Fixes the analysis cross-field invariants. Shared with presets.
///
/// The zero-pad length is never raised here even when the longest window outgrows it: the
/// analyzer grows the transform at run time (`SpectrumConfig::new`), so the user's choice comes
/// back when the window shrinks again.
pub fn normalize_analysis(a: &mut AnalysisConfig, notes: &mut Vec<String>) {
    if !a.fft_size.is_power_of_two() {
        a.fft_size = a.fft_size.next_power_of_two().min(MAX_FFT_SIZE);
        notes.push(format!(
            "analysis.fftSize raised to the power of two {}",
            a.fft_size
        ));
    }
    if a.window_length_mode == WindowLengthMode::Samples {
        while a.layers > 1 && a.largest_window() > MAX_WINDOW {
            a.layers -= 1;
            notes.push(format!(
                "analysis.layers lowered to {}: the longest window must stay at most {MAX_WINDOW}",
                a.layers
            ));
        }
    }
    if a.log_floor_hz * 2.0 > a.display_max_hz {
        a.log_floor_hz = (a.display_max_hz / 2.0).max(1.0);
        notes.push("analysis.logFloorHz lowered to keep an octave below displayMaxHz".into());
    }
}

/// v1 -> v2: the presets become the window/hop they stood for; band layout, edges, dB bounds and
/// auto-gain become the CHOP's scale, log floor, display max, dB range and reference.
pub fn migrate_v1(doc: &mut Map<String, Value>) {
    let Some(Value::Object(a)) = doc.get_mut("analysis") else {
        return;
    };
    let preset = a.remove("preset");
    let (window, hop, single) = match preset.as_ref().and_then(Value::as_str) {
        Some("lowLatency") => (1024, 256, true),
        Some("default") => (2048, 512, true),
        Some("precision") => (8192, 1024, true),
        Some("lowPower") => (1024, 512, true),
        _ => (1024, 512, false),
    };
    a.insert("windowSamples".into(), window.into());
    a.insert("hopSamples".into(), hop.into());
    if single {
        a.insert("layers".into(), 1.into());
    }
    if let Some(layout) = a.remove("layout") {
        a.insert("scale".into(), layout);
    }
    if let Some(v) = a.remove("minHz") {
        a.insert("logFloorHz".into(), v);
    }
    if let Some(v) = a.remove("maxHz") {
        a.insert("displayMaxHz".into(), v);
    }
    let floor = a.remove("dbFloor").and_then(|v| v.as_f64());
    let ceiling = a.remove("dbCeiling").and_then(|v| v.as_f64());
    if let (Some(f), Some(c)) = (floor, ceiling) {
        a.insert("dbRange".into(), (c - f).into());
    }
    if let Some(auto) = a.remove("autoGain").and_then(|v| v.as_bool()) {
        a.insert(
            "dbReference".into(),
            if auto { "agc" } else { "dbfs" }.into(),
        );
    }
    if a.contains_key("attackMs") || a.contains_key("releaseMs") {
        a.insert("ballistics".into(), true.into());
        a.insert("ballisticsMode".into(), "milliseconds".into());
    }
    // The bar count is a scene setting now (each scene pools the spectrum itself).
    a.remove("barCount");
    a.remove("zeroPad");
}

/// v2 -> v3: `parallelLayers` (auto / always / never) becomes `layerThreads` (0 = auto, one
/// thread per layer for "always", 1 for "never").
pub fn migrate_v2(doc: &mut Map<String, Value>) {
    let Some(Value::Object(a)) = doc.get_mut("analysis") else {
        return;
    };
    let threads = match a.remove("parallelLayers").as_ref().and_then(Value::as_str) {
        Some("never") => 1,
        Some("always") => MAX_LAYERS,
        _ => 0,
    };
    a.insert("layerThreads".into(), threads.into());
}

static MIGRATIONS: &[Migration] = &[
    Migration {
        from: 1,
        apply: migrate_v1,
    },
    Migration {
        from: 2,
        apply: migrate_v2,
    },
];

impl Versioned for AppConfig {
    const VERSION_KEY: &'static str = "schemaVersion";
    const CURRENT: u32 = SCHEMA_VERSION;

    fn migrations() -> &'static [Migration] {
        MIGRATIONS
    }

    fn limits() -> &'static [Limit<Self>] {
        LIMITS.as_slice()
    }

    fn normalize(&mut self, notes: &mut Vec<String>) {
        normalize_analysis(&mut self.analysis, notes);
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
