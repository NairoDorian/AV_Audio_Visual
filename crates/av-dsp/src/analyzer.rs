//! The analyzer (pre-plan section 5.3): hop in, bars out.
//!
//! `process` runs DC blocker -> sliding history -> for each layer: window the newest
//! `window_samples * 2^(k-1)` samples, zero-pad to `fft_size`, real FFT, magnitude scaled by
//! `2 / sum(w_k)` -> average over layers -> calibrated bin power -> per-bar power -> weighting ->
//! dB -> auto-gain -> normalization -> ballistics -> peak hold.
//!
//! Layers (ADR-020, `docs/super-fft.md`): with one layer this is a classic zero-padded FFT. With
//! more, each layer doubles the window and its magnitude is divided by its window sum, which is
//! the `1 / 2^(k-1)` window-length normalization up to one global constant, so every layer reads
//! the same level for the same tone and the per-bin average is calibrated (full-scale sine = 0 dB). Every buffer is allocated in `new`; `process` never allocates, locks, logs or plans.
//! FFT plans come from a [`PlanCache`] the engine owns and builds off the analysis thread.

use std::sync::Arc;

use av_protocol::config::{AnalysisConfig, BandLayout, FftPreset};
use realfft::num_complex::Complex32;
use realfft::{RealFftPlanner, RealToComplex};

use crate::ballistics::{Ballistics, PeakHold};
use crate::bands::{BandMap, BandSpec, LevelRef};
use crate::dc::DcBlocker;
use crate::weighting::Weighting;
use crate::window::{self, WindowKind};

/// Peak level below which a window counts as silent (-100 dBFS).
pub const SILENCE_THRESHOLD: f32 = 1e-5;
/// Smallest dB range auto-gain may compress the display to.
pub const AUTO_GAIN_MIN_RANGE_DB: f32 = 24.0;
/// Headroom above the tracked peak.
pub const AUTO_GAIN_HEADROOM_DB: f32 = 3.0;
/// How slowly the tracked peak falls after loud passages.
pub const AUTO_GAIN_RELEASE_MS: f32 = 4000.0;
/// Largest analysis FFT (32,768-sample window x zero-pad 4). Frames from FFTs above the wire
/// limit (`av_protocol::frame::MAX_FFT_SIZE`) carry bars only.
pub const MAX_FFT_SIZE: usize = 131_072;
/// Largest window any layer may use.
pub const MAX_WINDOW: usize = 32_768;
pub const MAX_LAYERS: usize = 6;
pub const MAX_BARS: usize = av_protocol::frame::MAX_BARS;

/// Everything the analyzer needs. Changing any field means building a new analyzer (a
/// generation bump at the next hop boundary), never mutating this one.
#[derive(Debug, Clone, PartialEq)]
pub struct SpectrumConfig {
    pub sample_rate: u32,
    /// Window of the first (shortest) layer.
    pub window_samples: usize,
    /// 1 = classic single window; up to 6, each doubling the window.
    pub layers: usize,
    /// Common FFT size; `>= largest window`, the rest is zero-padding.
    pub fft_size: usize,
    pub hop_samples: usize,
    pub window: WindowKind,
    pub layout: BandLayout,
    pub bar_count: usize,
    pub min_hz: f32,
    pub max_hz: f32,
    pub weighting: Weighting,
    pub level_ref: LevelRef,
    pub db_floor: f32,
    pub db_ceiling: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub auto_gain: bool,
    pub peak_hold_ms: f32,
    pub peak_fall_per_s: f32,
    /// Also compute normalized per-bin levels (preview spectrogram).
    pub emit_bins: bool,
}

impl SpectrumConfig {
    /// The analyzer settings implied by the persisted config, at the analysis rate.
    pub fn from_app(a: &AnalysisConfig, sample_rate: u32) -> Self {
        let (window, hop) = a.preset.sizes();
        let layers = if a.preset == FftPreset::Layered {
            (a.layers as usize).clamp(1, MAX_LAYERS)
        } else {
            1
        };
        let pad = match a.zero_pad {
            0 | 1 => 1,
            2 => 2,
            _ => 4,
        };
        let largest = (window as usize) << (layers - 1);
        Self {
            sample_rate,
            window_samples: window as usize,
            layers,
            fft_size: largest * pad,
            hop_samples: hop as usize,
            window: WindowKind::Hann,
            layout: a.layout,
            bar_count: a.bar_count as usize,
            min_hz: a.min_hz,
            max_hz: a.max_hz,
            weighting: Weighting::Z,
            level_ref: LevelRef::ThirdOctave,
            db_floor: a.db_floor,
            db_ceiling: a.db_ceiling,
            attack_ms: a.attack_ms,
            release_ms: a.release_ms,
            auto_gain: a.auto_gain,
            peak_hold_ms: 400.0,
            peak_fall_per_s: 1.5,
            emit_bins: false,
        }
    }

    /// A classic single-window analysis (tests, benches, explicit presets).
    pub fn single(window_samples: usize, fft_size: usize, hop_samples: usize) -> Self {
        Self {
            window_samples,
            layers: 1,
            fft_size,
            hop_samples,
            ..Self::default()
        }
    }

    /// A layered analysis with a 1,024 base window and the given zero-pad factor.
    pub fn layered(layers: usize, zero_pad: usize) -> Self {
        let window_samples = 1024;
        Self {
            window_samples,
            layers,
            fft_size: (window_samples << (layers.max(1) - 1)) * zero_pad,
            hop_samples: 512,
            ..Self::default()
        }
    }

    pub fn bin_count(&self) -> usize {
        self.fft_size / 2 + 1
    }

    /// The window of the longest layer.
    pub fn largest_window(&self) -> usize {
        self.window_samples << (self.layers.clamp(1, MAX_LAYERS) - 1)
    }

    pub fn validate(&self) -> Result<(), DspError> {
        if self.layers == 0 || self.layers > MAX_LAYERS {
            return Err(DspError::InvalidLayers(self.layers));
        }
        if self.window_samples < 16 || self.largest_window() > MAX_WINDOW {
            return Err(DspError::InvalidWindow(self.window_samples));
        }
        if self.fft_size < self.largest_window()
            || self.fft_size > MAX_FFT_SIZE
            || !self.fft_size.is_multiple_of(2)
        {
            return Err(DspError::InvalidFftSize(self.fft_size));
        }
        if self.hop_samples == 0 || self.hop_samples > self.fft_size {
            return Err(DspError::InvalidHop(self.hop_samples));
        }
        if self.bar_count == 0 || self.bar_count > MAX_BARS {
            return Err(DspError::InvalidBarCount(self.bar_count));
        }
        if self.sample_rate < 8_000 {
            return Err(DspError::InvalidSampleRate(self.sample_rate));
        }
        // Also rejects NaN bounds, which compare as unordered.
        if self.db_ceiling.partial_cmp(&self.db_floor) != Some(std::cmp::Ordering::Greater) {
            return Err(DspError::InvalidRange);
        }
        Ok(())
    }
}

impl Default for SpectrumConfig {
    fn default() -> Self {
        Self::from_app(&AnalysisConfig::default(), 48_000)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DspError {
    InvalidFftSize(usize),
    InvalidHop(usize),
    InvalidBarCount(usize),
    InvalidSampleRate(u32),
    InvalidRange,
    InvalidLayers(usize),
    InvalidWindow(usize),
    PlanMismatch { plan: usize, fft_size: usize },
}

impl std::fmt::Display for DspError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidFftSize(n) => write!(
                f,
                "fft size {n} must be even, at least the largest window and at most {MAX_FFT_SIZE}"
            ),
            Self::InvalidLayers(n) => write!(f, "layer count {n} must be in 1..={MAX_LAYERS}"),
            Self::InvalidWindow(n) => write!(
                f,
                "window {n} must be at least 16 and its largest layer at most {MAX_WINDOW}"
            ),
            Self::InvalidHop(n) => write!(f, "hop {n} must be in 1..=fft_size"),
            Self::InvalidBarCount(n) => write!(f, "bar count {n} must be in 1..={MAX_BARS}"),
            Self::InvalidSampleRate(r) => write!(f, "sample rate {r} is too low"),
            Self::InvalidRange => write!(f, "db_ceiling must be above db_floor"),
            Self::PlanMismatch { plan, fft_size } => {
                write!(
                    f,
                    "FFT plan of length {plan} handed to a {fft_size}-point analyzer"
                )
            }
        }
    }
}

impl std::error::Error for DspError {}

/// Caches FFT plans by length. Owned by the engine; `get` may allocate and must not be called on
/// the analysis thread's hot path.
pub struct PlanCache {
    planner: RealFftPlanner<f32>,
}

impl std::fmt::Debug for PlanCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PlanCache")
    }
}

impl Default for PlanCache {
    fn default() -> Self {
        Self {
            planner: RealFftPlanner::new(),
        }
    }
}

impl PlanCache {
    pub fn get(&mut self, len: usize) -> Arc<dyn RealToComplex<f32>> {
        self.planner.plan_fft_forward(len)
    }
}

/// What one hop produced. Borrowed from the analyzer; copy out what you keep.
#[derive(Debug, Clone, Copy)]
pub struct FrameData<'a> {
    /// Smoothed, normalized 0..1 bar levels.
    pub bars: &'a [f32],
    /// Peak-hold markers, normalized 0..1.
    pub peaks: &'a [f32],
    /// Unsmoothed bar levels in dB (after weighting), for tests and diagnostics.
    pub bar_db: &'a [f32],
    /// Normalized per-bin levels if `emit_bins`.
    pub bins: Option<&'a [f32]>,
    pub silent: bool,
    pub clipped: bool,
    /// The dB value mapped to the top of the display this hop (auto-gain aware).
    pub ceiling_db: f32,
}

pub struct Analyzer {
    cfg: SpectrumConfig,
    fft: Arc<dyn RealToComplex<f32>>,
    layers: Vec<Layer>,
    /// Sum of the layers' calibrated magnitudes for this hop.
    magnitude: Vec<f32>,
    history: Vec<f32>,
    write_pos: usize,
    fft_in: Vec<f32>,
    spectrum: Vec<Complex32>,
    scratch: Vec<Complex32>,
    bin_power: Vec<f32>,
    bin_level: Vec<f32>,
    bands: BandMap,
    bar_power: Vec<f32>,
    bar_db: Vec<f32>,
    target: Vec<f32>,
    bars: Vec<f32>,
    peaks: Vec<f32>,
    peak_held: Vec<f32>,
    dc: DcBlocker,
    ballistics: Ballistics,
    peak_hold: PeakHold,
    gain_env_db: f32,
    clipped: bool,
}

/// One FFT layer: its window and the amplitude scale `2 / sum(w)` (a full-scale sine at a bin
/// centre reads amplitude 1 in every layer).
struct Layer {
    window: Vec<f32>,
    amp_scale: f32,
}

impl std::fmt::Debug for Analyzer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Analyzer")
            .field("cfg", &self.cfg)
            .finish_non_exhaustive()
    }
}

impl Analyzer {
    /// Builds every buffer, the window and the band map. Allocates; call off the hot path.
    pub fn new(cfg: SpectrumConfig, fft: Arc<dyn RealToComplex<f32>>) -> Result<Self, DspError> {
        cfg.validate()?;
        if fft.len() != cfg.fft_size {
            return Err(DspError::PlanMismatch {
                plan: fft.len(),
                fft_size: cfg.fft_size,
            });
        }
        let n = cfg.fft_size;
        let layers: Vec<Layer> = (0..cfg.layers)
            .map(|k| {
                let mut window = vec![0.0; cfg.window_samples << k];
                cfg.window.fill(&mut window);
                let amp_scale = (2.0 / window::sum(&window)) as f32;
                Layer { window, amp_scale }
            })
            .collect();
        let mut fft_in = fft.make_input_vec();
        let mut spectrum = fft.make_output_vec();
        let mut scratch = fft.make_scratch_vec();
        let enbw = effective_enbw(
            &layers,
            fft.as_ref(),
            &mut fft_in,
            &mut spectrum,
            &mut scratch,
        );
        let bands = BandMap::build(BandSpec {
            layout: cfg.layout,
            bar_count: cfg.bar_count,
            min_hz: cfg.min_hz as f64,
            max_hz: cfg.max_hz as f64,
            sample_rate: cfg.sample_rate,
            fft_size: n,
            enbw_bins: enbw,
            level_ref: cfg.level_ref,
            weighting: cfg.weighting,
        });
        let bars = cfg.bar_count;
        let bins = cfg.bin_count();
        let emit_bins = cfg.emit_bins && bins <= av_protocol::frame::MAX_BINS;
        Ok(Self {
            history: vec![0.0; cfg.largest_window()],
            layers,
            magnitude: vec![0.0; bins],
            write_pos: 0,
            fft_in,
            spectrum,
            scratch,
            bin_power: vec![0.0; bins],
            bin_level: vec![0.0; if emit_bins { bins } else { 0 }],
            bands,
            bar_power: vec![0.0; bars],
            bar_db: vec![0.0; bars],
            target: vec![0.0; bars],
            bars: vec![0.0; bars],
            peaks: vec![0.0; bars],
            peak_held: vec![0.0; bars],
            dc: DcBlocker::new(cfg.sample_rate),
            ballistics: Ballistics {
                attack_ms: cfg.attack_ms,
                release_ms: cfg.release_ms,
            },
            peak_hold: PeakHold {
                hold_ms: cfg.peak_hold_ms,
                fall_per_s: cfg.peak_fall_per_s,
            },
            gain_env_db: cfg.db_floor + AUTO_GAIN_MIN_RANGE_DB,
            clipped: false,
            fft,
            cfg,
        })
    }

    pub fn config(&self) -> &SpectrumConfig {
        &self.cfg
    }

    /// Calibrated power per FFT bin from the last hop (diagnostics and tests).
    pub fn bin_power(&self) -> &[f32] {
        &self.bin_power
    }

    pub fn band_centers(&self) -> &[f32] {
        &self.bands.centers
    }

    /// Clears the sample history and filter state after a gap in the input (overrun, device
    /// change). Ballistics keep their state so the display decays instead of jumping.
    pub fn reset_continuity(&mut self) {
        self.history.fill(0.0);
        self.write_pos = 0;
        self.dc.reset();
    }

    /// Feeds one hop of mono samples (normally `hop_samples` long) and analyses the newest
    /// samples with every layer. `dt` is the wall-clock time this hop represents, normally
    /// `hop / sample_rate`.
    pub fn process(&mut self, hop: &[f32], dt: f32) -> FrameData<'_> {
        let cap = self.history.len();
        let hop = &hop[hop.len().saturating_sub(cap)..];
        self.clipped = false;
        for &x in hop {
            self.clipped |= x.abs() >= 1.0;
            self.history[self.write_pos] = self.dc.process(x);
            self.write_pos = (self.write_pos + 1) % cap;
        }

        // Silence is judged on the shortest layer's window: the newest audio.
        let mut peak = 0.0f32;
        for_newest(
            &self.history,
            self.write_pos,
            self.cfg.window_samples,
            |_, x| {
                peak = peak.max(x.abs());
            },
        );
        let silent = peak < SILENCE_THRESHOLD;

        self.magnitude.fill(0.0);
        for layer in &self.layers {
            let len = layer.window.len();
            let (fft_in, window) = (&mut self.fft_in, &layer.window);
            for_newest(&self.history, self.write_pos, len, |i, x| {
                fft_in[i] = x * window[i]
            });
            fft_in[len..].fill(0.0);
            if self
                .fft
                .process_with_scratch(&mut self.fft_in, &mut self.spectrum, &mut self.scratch)
                .is_err()
            {
                // Lengths are fixed at construction, so this cannot happen; contribute nothing.
                continue;
            }
            for (m, c) in self.magnitude.iter_mut().zip(&self.spectrum) {
                *m += c.norm() * layer.amp_scale;
            }
        }
        let inv_layers = 1.0 / self.layers.len() as f32;
        let last = self.bin_power.len() - 1;
        for (k, (p, &m)) in self.bin_power.iter_mut().zip(&self.magnitude).enumerate() {
            // Amplitude a = average calibrated magnitude; a sine's power is a^2 / 2. DC and
            // Nyquist have no mirrored half.
            let a = m * inv_layers;
            let one_sided = if k == 0 || k == last { 0.5 } else { 1.0 };
            *p = 0.5 * a * a * one_sided;
        }

        self.bands.apply(&self.bin_power, &mut self.bar_power);
        let mut loudest = f32::NEG_INFINITY;
        for ((db, &p), &w) in self
            .bar_db
            .iter_mut()
            .zip(&self.bar_power)
            .zip(&self.bands.weighting_db)
        {
            *db = power_db(p) + w;
            loudest = loudest.max(*db);
        }

        let floor = self.cfg.db_floor;
        let ceiling = if self.cfg.auto_gain {
            if !silent {
                if loudest > self.gain_env_db {
                    self.gain_env_db = loudest;
                } else {
                    let k = crate::ballistics::coefficient(dt, AUTO_GAIN_RELEASE_MS);
                    self.gain_env_db += (loudest - self.gain_env_db) * k;
                }
            }
            (self.gain_env_db + AUTO_GAIN_HEADROOM_DB).clamp(
                floor + AUTO_GAIN_MIN_RANGE_DB,
                self.cfg.db_ceiling.max(floor + AUTO_GAIN_MIN_RANGE_DB),
            )
        } else {
            self.cfg.db_ceiling
        };
        let span = ceiling - floor;

        for (t, &db) in self.target.iter_mut().zip(&self.bar_db) {
            *t = if silent {
                0.0
            } else {
                ((db - floor) / span).clamp(0.0, 1.0)
            };
        }
        self.ballistics.apply(&mut self.bars, &self.target, dt);
        self.peak_hold
            .apply(&mut self.peaks, &mut self.peak_held, &self.bars, dt);

        let bins = if !self.bin_level.is_empty() {
            for (l, &p) in self.bin_level.iter_mut().zip(&self.bin_power) {
                *l = if silent {
                    0.0
                } else {
                    ((power_db(p) - floor) / span).clamp(0.0, 1.0)
                };
            }
            Some(&self.bin_level[..])
        } else {
            None
        };

        FrameData {
            bars: &self.bars,
            peaks: &self.peaks,
            bar_db: &self.bar_db,
            bins,
            silent,
            clipped: self.clipped,
            ceiling_db: ceiling,
        }
    }
}

/// Calls `f(i, sample)` for the newest `len` samples of a ring, oldest first (`i` from 0).
#[inline]
fn for_newest(ring: &[f32], write_pos: usize, len: usize, mut f: impl FnMut(usize, f32)) {
    let cap = ring.len();
    let start = (write_pos + cap - len) % cap;
    let first = (cap - start).min(len);
    for (i, &x) in ring[start..start + first].iter().enumerate() {
        f(i, x);
    }
    for (i, &x) in ring[..len - first].iter().enumerate() {
        f(first + i, x);
    }
}

/// Exact effective noise bandwidth (in bins of `fft_size`) of the layer-averaged response:
/// average the layers' window magnitude responses normalized to 1 at DC, then sum the squares
/// over the whole (two-sided) spectrum. One unpadded layer gives the classic
/// `N * sum(w^2) / sum(w)^2` (Parseval). Runs once per configuration.
fn effective_enbw(
    layers: &[Layer],
    fft: &dyn RealToComplex<f32>,
    fft_in: &mut [f32],
    spectrum: &mut [Complex32],
    scratch: &mut [Complex32],
) -> f64 {
    let bins = spectrum.len();
    let mut avg = vec![0.0f64; bins];
    for layer in layers {
        let len = layer.window.len();
        fft_in[..len].copy_from_slice(&layer.window);
        fft_in[len..].fill(0.0);
        if fft.process_with_scratch(fft_in, spectrum, scratch).is_err() {
            continue;
        }
        let dc = spectrum[0].norm() as f64;
        for (a, c) in avg.iter_mut().zip(spectrum.iter()) {
            *a += c.norm() as f64 / dc / layers.len() as f64;
        }
    }
    let last = bins - 1;
    avg.iter()
        .enumerate()
        .map(|(k, &r)| {
            if k == 0 || k == last {
                r * r
            } else {
                2.0 * r * r
            }
        })
        .sum()
}

/// dB relative to a full-scale sine (power 1/2), floored far below any display range.
#[inline]
fn power_db(p: f32) -> f32 {
    10.0 * (2.0 * p + 1e-20).log10()
}
