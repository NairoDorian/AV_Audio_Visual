//! The analyzer (pre-plan section 5.3): hop in, display spectrum out.
//!
//! Stage order per hop, the FFT CHOP's (`AnalysisPipeline::runChannel`), with the layered
//! "Super FFT" in front:
//!
//! DC blocker -> shelf EQ (new samples only, stateful) -> sliding history -> layers (each
//! windows the newest `window * 2^(k-1)` samples, zero-pads to the one common FFT size,
//! transforms, and reports `|X|` for the bins the warp reads, with the calibration
//! `2 / sum(w_k)`, the magnitude normalization and the `1 / L` average folded into its window,
//! FFT CHOP v2.13) -> per-bin sum over the layers that are not silent -> spectral features (layer
//! 1, the most reactive) ->
//! warp onto the output axis (scale, warp blend, interpolation, peak/RMS aggregation) ->
//! equal-loudness weighting -> reference (frame peak, dBFS or AGC) -> loudness (dB over the dB
//! range, or linear) onto 0..1 -> attack/release ballistics -> peak search.
//!
//! Layers (ADR-020, `docs/super-fft.md`): with one layer this is the CHOP's zero-padded FFT.
//! With more, each layer doubles the window and its magnitude is divided by its window sum,
//! which is the `1 / 2^(k-1)` window-length normalization up to one global constant, so every
//! layer reads the same level for the same tone and the per-bin average is calibrated.
//!
//! Every buffer is allocated in `new`; `process` never allocates, locks, logs or plans. FFT plans
//! come from a [`PlanCache`] the engine owns and builds off the analysis thread.
//!
//! Sources: the FFT CHOP (primary source of truth for this analyzer, re-implemented) and its
//! Rust port in ZER0 `src-tauri/src/live_fft/dsp.rs` (MIT, same author), from which the warp,
//! weighting, dB, ballistics and feature stages are ported.

use std::sync::Arc;

use av_protocol::config::{
    AnalysisConfig, BallisticsMode, DbReference, KaiserBetaMode, LayerRefresh, LoudnessMode,
    MAX_FFT_SIZE, MAX_LAYERS, MAX_OUTPUT_BINS, MAX_WINDOW, MIN_WINDOW, MagnitudeNorm,
    OutputBinsMode, Scale, WarpAggregation, WarpInterp, Weighting, WindowLengthMode,
};
use realfft::{RealFftPlanner, RealToComplex};

use crate::dc::DcBlocker;
use crate::eq::{EqConfig, ShelfEq};
use crate::features::SpectralFeatures;
use crate::layers::{LayerPlan, LayerRunner, LayerUnit, SequentialLayers, newest};
use crate::warp::{Axis, Warp, max_of};
use crate::window::{WindowKind, kaiser_beta_for_sidelobe_db};
use crate::{ballistics, db, features};

/// Peak level below which a window counts as silent (-100 dBFS).
pub const SILENCE_THRESHOLD: f32 = 1e-5;

/// Everything the analyzer needs: the persisted settings and the values resolved from them at
/// the source rate. Changing anything means building a new analyzer (a generation bump at the
/// next hop boundary), never mutating this one.
#[derive(Debug, Clone, PartialEq)]
pub struct SpectrumConfig {
    pub sample_rate: u32,
    pub settings: AnalysisConfig,
    /// Window of the first (shortest) layer, samples.
    pub window_samples: usize,
    pub layers: usize,
    /// The transform length of every layer.
    pub fft_size: usize,
    pub hop_samples: usize,
}

impl SpectrumConfig {
    /// Resolves `settings` at `sample_rate`: the window (samples or milliseconds), the layer
    /// count that keeps the longest window within [`MAX_WINDOW`], and the FFT size (the
    /// zero-pad length grown to the longest window, or the window itself without padding).
    pub fn new(settings: AnalysisConfig, sample_rate: u32) -> Self {
        let base = match settings.window_length_mode {
            WindowLengthMode::Samples => settings.window_samples as usize,
            WindowLengthMode::Milliseconds => {
                (settings.window_ms as f64 * sample_rate as f64 / 1000.0).round() as usize
            }
        }
        .clamp(MIN_WINDOW as usize, MAX_WINDOW as usize);
        let mut layers = (settings.layers as usize).clamp(1, MAX_LAYERS as usize);
        while layers > 1 && base << (layers - 1) > MAX_WINDOW as usize {
            layers -= 1;
        }
        let largest = base << (layers - 1);
        let fft_size = if settings.zero_padding {
            (settings.fft_size as usize)
                .next_power_of_two()
                .max(largest.next_power_of_two())
                .min(MAX_FFT_SIZE as usize)
        } else {
            largest + (largest & 1)
        };
        let hop_samples = (settings.hop_samples as usize).clamp(1, fft_size);
        Self {
            sample_rate,
            settings,
            window_samples: base,
            layers,
            fft_size,
            hop_samples,
        }
    }

    /// The same settings at another rate (the engine knows the source rate, the Studio does not).
    pub fn at_rate(self, sample_rate: u32) -> Self {
        Self::new(self.settings, sample_rate)
    }

    /// Settings for tests and benches: no EQ, no ballistics, dBFS reference.
    fn plain(settings: AnalysisConfig) -> AnalysisConfig {
        let mut s = settings;
        s.eq.enabled = false;
        s.ballistics = false;
        s.db_reference = DbReference::Dbfs;
        s
    }

    /// A classic single-window analysis (tests, benches).
    pub fn single(window_samples: usize, fft_size: usize, hop_samples: usize) -> Self {
        let s = AnalysisConfig {
            layers: 1,
            window_samples: window_samples as u32,
            fft_size: fft_size as u32,
            hop_samples: hop_samples as u32,
            ..Self::plain(AnalysisConfig::default())
        };
        Self::new(s, 48_000)
    }

    /// A layered analysis (1,024 base window, hop 512) at one common FFT size (tests, benches).
    pub fn layered(layers: usize, fft_size: usize) -> Self {
        let s = AnalysisConfig {
            layers: layers as u32,
            window_samples: 1024,
            fft_size: fft_size as u32,
            hop_samples: 512,
            ..Self::plain(AnalysisConfig::default())
        };
        Self::new(s, 48_000)
    }

    /// Bins of the transform, `N/2 + 1`.
    pub fn bin_count(&self) -> usize {
        self.fft_size / 2 + 1
    }

    pub fn nyquist(&self) -> f64 {
        self.sample_rate as f64 / 2.0
    }

    /// The window of the longest layer.
    pub fn largest_window(&self) -> usize {
        self.window_samples << (self.layers.clamp(1, MAX_LAYERS as usize) - 1)
    }

    /// Window length of layer `k` (0-based).
    pub fn layer_window(&self, k: usize) -> usize {
        self.window_samples << k
    }

    /// Output bins this configuration produces.
    pub fn output_bins(&self) -> usize {
        let s = &self.settings;
        if s.raw_bins || s.output_bins_mode == OutputBinsMode::Auto {
            self.bin_count()
        } else {
            s.output_bins as usize
        }
    }

    /// The Kaiser beta in use (manual, or the one that puts the sidelobes at the dB floor).
    pub fn kaiser_beta(&self) -> f32 {
        match self.settings.kaiser_beta_mode {
            KaiserBetaMode::Manual => self.settings.kaiser_beta,
            KaiserBetaMode::Auto => {
                kaiser_beta_for_sidelobe_db(self.settings.db_range as f64) as f32
            }
        }
    }

    pub fn window_kind(&self) -> WindowKind {
        WindowKind::from_shape(self.settings.window, self.kaiser_beta())
    }

    /// The output axis. Raw RFFT Bins is the identity grid: linear, unblended, DC..Nyquist.
    pub fn axis(&self) -> Axis {
        let s = &self.settings;
        let nyquist = self.nyquist();
        if s.raw_bins {
            return Axis {
                scale: Scale::Linear,
                fmax: nyquist,
                n_out: self.bin_count(),
                warp_blend: 0.0,
                log_floor_hz: s.log_floor_hz as f64,
                interp: WarpInterp::Linear,
                aggregation: WarpAggregation::Off,
            };
        }
        let fmax = (s.display_max_hz as f64).min(nyquist);
        Axis {
            scale: s.scale,
            fmax,
            n_out: self.output_bins(),
            warp_blend: s.warp_blend.clamp(0.0, 1.0) as f64,
            log_floor_hz: (s.log_floor_hz as f64).min(fmax / 2.0),
            interp: s.warp_interp,
            aggregation: s.warp_aggregation,
        }
    }

    pub fn eq(&self) -> EqConfig {
        let e = &self.settings.eq;
        EqConfig {
            enabled: e.enabled,
            high_shelf: e.high_shelf,
            high_gain_db: e.high_gain_db,
            high_cutoff_hz: e.high_cutoff_hz,
            low_shelf: e.low_shelf,
            low_gain_db: e.low_gain_db,
            low_cutoff_hz: e.low_cutoff_hz,
            q: e.q,
            amount: e.amount,
        }
    }

    /// Applied after calibration: 1 (full scale) or `window / 2` (coherent gain), so a sine
    /// reads the first layer's coherent gain in every layer (the CHOP's `1 / 2^(k-1)`).
    pub fn norm_scale(&self) -> f32 {
        match self.settings.magnitude_norm {
            MagnitudeNorm::FullScale => 1.0,
            MagnitudeNorm::CoherentGain => self.window_samples as f32 * 0.5,
        }
    }

    /// The layer units of this config, each computing `bins_used` bins, with the normalization
    /// and the `1 / L` average folded into the windows (allocates; off the hot path). Hand them
    /// to any [`LayerRunner`].
    pub fn build_units(
        &self,
        fft: &Arc<dyn RealToComplex<f32>>,
        bins_used: usize,
    ) -> Vec<LayerUnit> {
        let kind = self.window_kind();
        let gain = self.norm_scale() / self.layers as f32;
        (0..self.layers)
            .map(|k| LayerUnit::new(self.layer_window(k), kind, fft.clone(), bins_used, gain))
            .collect()
    }

    /// Hop-Scaled refresh (FFT CHOP v2.13 `layerRefreshDivisor` / `layerRefreshPhase`): layer
    /// `k` (0-based) is recomputed on hops where `hop % divisor == phase`.
    pub fn refresh(&self, k: usize) -> (u64, u64) {
        if self.settings.layer_refresh != LayerRefresh::HopScaled || k < 3 {
            return (1, 0);
        }
        let divisor = 1u64 << (k - 2).min(3);
        let phase = match k {
            3 => 0,
            4 => 1,
            _ => 3,
        };
        (divisor, phase)
    }

    pub fn validate(&self) -> Result<(), DspError> {
        if self.layers == 0 || self.layers > MAX_LAYERS as usize {
            return Err(DspError::InvalidLayers(self.layers));
        }
        if self.window_samples < MIN_WINDOW as usize || self.largest_window() > MAX_WINDOW as usize
        {
            return Err(DspError::InvalidWindow(self.window_samples));
        }
        if self.fft_size < self.largest_window()
            || self.fft_size > MAX_FFT_SIZE as usize
            || !self.fft_size.is_multiple_of(2)
        {
            return Err(DspError::InvalidFftSize(self.fft_size));
        }
        if self.hop_samples == 0 || self.hop_samples > self.fft_size {
            return Err(DspError::InvalidHop(self.hop_samples));
        }
        let bins = self.output_bins();
        if bins == 0 || bins > MAX_OUTPUT_BINS as usize {
            return Err(DspError::InvalidOutputBins(bins));
        }
        if self.sample_rate < 8_000 {
            return Err(DspError::InvalidSampleRate(self.sample_rate));
        }
        Ok(())
    }
}

impl Default for SpectrumConfig {
    fn default() -> Self {
        Self::new(AnalysisConfig::default(), 48_000)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DspError {
    InvalidFftSize(usize),
    InvalidHop(usize),
    InvalidOutputBins(usize),
    InvalidSampleRate(u32),
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
                "window {n} must be at least {MIN_WINDOW} and its largest layer at most {MAX_WINDOW}"
            ),
            Self::InvalidHop(n) => write!(f, "hop {n} must be in 1..=fft_size"),
            Self::InvalidOutputBins(n) => {
                write!(f, "output bins {n} must be in 1..={MAX_OUTPUT_BINS}")
            }
            Self::InvalidSampleRate(r) => write!(f, "sample rate {r} is too low"),
            Self::PlanMismatch { plan, fft_size } => write!(
                f,
                "FFT plan of length {plan} handed to a {fft_size}-point analyzer"
            ),
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

/// Raw input samples that are not finite are replaced by this (a NaN would poison the filter
/// state and every later frame).
const NON_FINITE_SAMPLE: f32 = 0.0;

/// What one hop produced. Borrowed from the analyzer; copy out what you keep.
#[derive(Debug, Clone, Copy)]
pub struct FrameData<'a> {
    /// Display values, 0..1, one per output bin.
    pub bars: &'a [f32],
    pub silent: bool,
    pub clipped: bool,
    /// Largest absolute raw input sample of this hop (before DC blocker and EQ), 0..1+.
    pub input_peak: f32,
    /// Frequency of the loudest output bin.
    pub peak_hz: f32,
    /// The reference full height meant this hop, in the magnitude's own scale.
    pub reference: f32,
    /// Present when spectral features are enabled.
    pub features: Option<SpectralFeatures>,
    /// Layers that were recomputed this hop (bit k = layer k).
    pub computed: u32,
}

pub struct Analyzer {
    cfg: SpectrumConfig,
    runner: Box<dyn LayerRunner>,
    /// Layer-averaged, normalized linear magnitude of the bins the warp reads.
    magnitude: Vec<f32>,
    history: Vec<f32>,
    write_pos: usize,
    warp: Warp,
    weights: Vec<f32>,
    out: Vec<f32>,
    envelope: Vec<f32>,
    feature_prev: Vec<f32>,
    have_prev: bool,
    dc: DcBlocker,
    eq: ShelfEq,
    /// What a full-scale sine reads after normalization (the dBFS reference).
    dbfs_ref: f32,
    /// Newest raw input samples in a row below the silence threshold: layer k is silent when
    /// this reaches its window.
    quiet_run: usize,
    /// Layers holding a magnitude from an earlier hop (bit k), for Hop-Scaled reuse.
    valid: u32,
    /// Hops analysed, for the refresh phases.
    hop_index: u64,
    /// `(divisor, phase)` per layer.
    refresh: [(u64, u64); MAX_LAYERS as usize],
    agc_peak: f32,
    clipped: bool,
    input_peak: f32,
}

impl std::fmt::Debug for Analyzer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Analyzer")
            .field("cfg", &self.cfg)
            .finish_non_exhaustive()
    }
}

impl Analyzer {
    /// Builds every buffer, the layers (run sequentially) and the warp tables. Allocates; call
    /// off the hot path.
    pub fn new(cfg: SpectrumConfig, fft: Arc<dyn RealToComplex<f32>>) -> Result<Self, DspError> {
        Self::with_runner(cfg, fft, |units| Box::new(SequentialLayers::new(units)))
    }

    /// As [`Analyzer::new`], with the layer units handed to `make_runner` (the engine's parallel
    /// runner puts each on its own thread).
    pub fn with_runner(
        cfg: SpectrumConfig,
        fft: Arc<dyn RealToComplex<f32>>,
        make_runner: impl FnOnce(Vec<LayerUnit>) -> Box<dyn LayerRunner>,
    ) -> Result<Self, DspError> {
        cfg.validate()?;
        if fft.len() != cfg.fft_size {
            return Err(DspError::PlanMismatch {
                plan: fft.len(),
                fft_size: cfg.fft_size,
            });
        }
        db::warm();
        let s = &cfg.settings;
        let nlin = cfg.bin_count();
        let warp = Warp::new(cfg.axis(), cfg.nyquist(), nlin);
        let bins_used = if s.spectral_features {
            nlin
        } else {
            (warp.max_linear_index() + 1).min(nlin)
        };
        let units = cfg.build_units(&fft, bins_used);
        let mut refresh = [(1, 0); MAX_LAYERS as usize];
        for (k, r) in refresh.iter_mut().enumerate() {
            *r = cfg.refresh(k);
        }
        let weights = if s.weighting == Weighting::Off {
            Vec::new()
        } else {
            crate::weighting::curve(s.weighting, warp.target_hz())
        };
        let n_out = warp.output_bins();
        let norm_scale = cfg.norm_scale();
        Ok(Self {
            runner: make_runner(units),
            magnitude: vec![0.0; bins_used],
            history: vec![0.0; cfg.largest_window()],
            write_pos: 0,
            weights,
            out: vec![0.0; n_out],
            envelope: vec![0.0; n_out],
            feature_prev: vec![0.0; if s.spectral_features { bins_used } else { 0 }],
            have_prev: false,
            dc: DcBlocker::new(cfg.sample_rate),
            eq: ShelfEq::new(&cfg.eq(), cfg.sample_rate),
            dbfs_ref: norm_scale,
            quiet_run: 0,
            valid: 0,
            hop_index: 0,
            refresh,
            agc_peak: 0.0,
            clipped: false,
            input_peak: 0.0,
            warp,
            cfg,
        })
    }

    pub fn config(&self) -> &SpectrumConfig {
        &self.cfg
    }

    /// Threads the layers run on (1 = the analysis thread only).
    pub fn threads(&self) -> usize {
        self.runner.threads()
    }

    /// Carries the audio context of the analyzer this one replaces (a setting changed): the
    /// newest samples of its history, its silence run and DC state, and, when the output keeps
    /// its shape, the ballistics envelope and the AGC level. A setting change then neither
    /// restarts from silence nor dips the display (FFT CHOP v2.13: "a window change keeps the
    /// history"). Allocation-free.
    pub fn adopt(&mut self, old: &Analyzer) {
        let n = self.history.len().min(old.history.len());
        let (o_old, o_new) = newest(&old.history, old.write_pos, n);
        let cap = self.history.len();
        let start = (cap - n) % cap;
        for (i, &x) in o_old.iter().chain(o_new).enumerate() {
            self.history[(start + i) % cap] = x;
        }
        self.write_pos = 0;
        self.quiet_run = old.quiet_run;
        self.dc = old.dc.clone();
        if self.cfg.eq() == old.cfg.eq() && self.cfg.sample_rate == old.cfg.sample_rate {
            self.eq = old.eq.clone();
        }
        let same_shape = self.envelope.len() == old.envelope.len()
            && self.cfg.settings.loudness == old.cfg.settings.loudness;
        if same_shape {
            self.envelope.copy_from_slice(&old.envelope);
        }
        if self.cfg.settings.magnitude_norm == old.cfg.settings.magnitude_norm
            && self.dbfs_ref == old.dbfs_ref
        {
            self.agc_peak = old.agc_peak;
        }
    }

    /// The FFT CHOP's Reset: clears the filter, ballistics, AGC and feature state and recomputes
    /// every layer; the sample history (the audio itself) is kept.
    pub fn reset_state(&mut self) {
        self.dc.reset();
        self.eq.reset();
        self.envelope.fill(0.0);
        self.agc_peak = 0.0;
        self.have_prev = false;
        self.valid = 0;
    }

    /// Frequency of every output bin.
    pub fn target_hz(&self) -> &[f64] {
        self.warp.target_hz()
    }

    /// The warp tables (diagnostics and tests).
    pub fn warp(&self) -> &Warp {
        &self.warp
    }

    /// Layer-averaged, normalized linear magnitude from the last hop, for the bins the warp
    /// reads (all bins with spectral features or Raw RFFT Bins).
    pub fn linear_magnitude(&self) -> &[f32] {
        &self.magnitude
    }

    /// Clears the sample history and filter state after a gap in the input (overrun, device
    /// change). Ballistics keep their state so the display decays instead of jumping.
    pub fn reset_continuity(&mut self) {
        self.history.fill(0.0);
        self.write_pos = 0;
        self.dc.reset();
        self.eq.reset();
        self.have_prev = false;
        self.quiet_run = 0;
        self.valid = 0;
    }

    /// Feeds one hop of mono samples (normally `hop_samples` long) and analyses the newest
    /// samples with every layer. `dt` is the wall-clock time this hop represents, normally
    /// `hop / sample_rate`.
    pub fn process(&mut self, hop: &[f32], dt: f32) -> FrameData<'_> {
        let cap = self.history.len();
        let hop = &hop[hop.len().saturating_sub(cap)..];
        self.clipped = false;
        self.input_peak = 0.0;
        for &x in hop {
            let x = if x.is_finite() { x } else { NON_FINITE_SAMPLE };
            self.clipped |= x.abs() >= 1.0;
            self.input_peak = self.input_peak.max(x.abs());
            let y = self.eq.process(self.dc.process(x));
            self.history[self.write_pos] = y;
            self.write_pos = (self.write_pos + 1) % cap;
            // Judged on the raw input, as the CHOP judges its input ring: the DC blocker and EQ
            // tails after an abrupt stop are sub-audible decays, not signal.
            self.quiet_run = if x.abs() < SILENCE_THRESHOLD {
                self.quiet_run.saturating_add(1)
            } else {
                0
            };
        }
        let s = &self.cfg.settings;
        let dt_ms = if dt.is_finite() && dt > 0.0 {
            (dt as f64 * 1000.0).min(5000.0)
        } else {
            self.cfg.hop_samples as f64 * 1000.0 / self.cfg.sample_rate as f64
        };

        // Per-layer silence (FFT CHOP v2.13): a layer whose whole window is below the silence
        // threshold contributes exactly 0 and is not computed; the frame is silent only when
        // every layer is. Hop-Scaled refresh recomputes the long layers on their phase only.
        let mut include = 0u32;
        let mut compute = 0u32;
        for k in 0..self.cfg.layers {
            if self.quiet_run >= self.cfg.layer_window(k) {
                continue;
            }
            include |= 1 << k;
            let (divisor, phase) = self.refresh[k];
            let due = self.valid & (1 << k) == 0 || self.hop_index % divisor == phase;
            if due {
                compute |= 1 << k;
            }
        }
        self.valid = (self.valid | compute) & include;
        self.hop_index = self.hop_index.wrapping_add(1);
        let silent = include == 0;

        let mut feats = None;
        if silent {
            // The magnitude of a zero window is zero through every stage: skip them.
            self.out.fill(0.0);
            self.magnitude.fill(0.0);
            self.have_prev = false;
            if s.spectral_features {
                feats = Some(SpectralFeatures::default());
            }
        } else {
            let plan = LayerPlan { include, compute };
            self.runner
                .run(&self.history, self.write_pos, plan, &mut self.magnitude);
            // Full scale: DC and Nyquist have no mirrored half.
            if s.magnitude_norm == MagnitudeNorm::FullScale {
                self.magnitude[0] *= 0.5;
                if self.magnitude.len() == self.cfg.bin_count() {
                    let last = self.magnitude.len() - 1;
                    self.magnitude[last] *= 0.5;
                }
            }
            if s.spectral_features {
                if include & 1 != 0 {
                    // Layer 1 (the most reactive), corrected for its 1/L share.
                    let (older, newer) =
                        newest(&self.history, self.write_pos, self.cfg.window_samples);
                    feats = Some(features::compute(
                        self.runner.first_magnitude(),
                        self.cfg.sample_rate as f64 / self.cfg.fft_size as f64,
                        [older, newer],
                        self.cfg.layers as f32,
                        &mut self.feature_prev,
                        self.have_prev,
                    ));
                    self.have_prev = true;
                } else {
                    feats = Some(SpectralFeatures::default());
                    self.have_prev = false;
                }
            }
            self.warp.apply(&self.magnitude, &mut self.out);
            for (v, &w) in self.out.iter_mut().zip(&self.weights) {
                *v *= w;
            }
        }

        let frame_peak = if silent { 0.0 } else { max_of(&self.out) };
        let mut reference = match s.db_reference {
            DbReference::Dbfs => self.dbfs_ref,
            DbReference::FramePeak => frame_peak,
            DbReference::Agc => {
                let decay = ballistics::coef_from_ms(s.agc_release_ms as f64, dt_ms);
                self.agc_peak = frame_peak.max(self.agc_peak * decay);
                self.agc_peak
            }
        };
        if !reference.is_finite() || reference <= 0.0 {
            reference = self.dbfs_ref;
        }
        match s.loudness {
            LoudnessMode::Db => db::to_display(&mut self.out, reference, s.db_range),
            LoudnessMode::Off => db::to_display_linear(&mut self.out, reference),
        }

        if s.ballistics {
            let (attack, release) = match s.ballistics_mode {
                BallisticsMode::Milliseconds => (
                    ballistics::coef_from_ms(s.attack_ms as f64, dt_ms),
                    ballistics::coef_from_ms(s.release_ms as f64, dt_ms),
                ),
                BallisticsMode::Coefficient => (s.attack, s.release),
            };
            ballistics::apply(attack, release, &mut self.out, &mut self.envelope);
        }

        let (mut best, mut idx) = (f32::NEG_INFINITY, 0);
        for (i, &v) in self.out.iter().enumerate() {
            if v > best {
                best = v;
                idx = i;
            }
        }
        let peak_hz = self.warp.target_hz().get(idx).copied().unwrap_or(0.0) as f32;

        FrameData {
            bars: &self.out,
            silent,
            clipped: self.clipped,
            input_peak: self.input_peak,
            peak_hz,
            reference,
            features: feats,
            computed: compute,
        }
    }
}
