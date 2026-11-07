//! Pure DSP for AV_AudioVisual.
//!
//! No threads, clocks, I/O or logging: every function is testable against golden vectors.
//! See `docs/pre-plan.md` section 5.3 (analyzer contract) and section 6 (numeric gates).
//!
//! The analysis chain is the FFT CHOP's (TouchDesigner custom operator by the product owner;
//! re-implemented, its source carries no licence) as ported to Rust in ZER0 `live_fft/dsp.rs`
//! (MIT, same author): windows, perceptual warp, weighting, dB, ballistics, spectral features.
//! The layered "Super FFT" (ADR-020) runs in front of it. The zero-allocation gate and
//! off-thread plan building come from the FFT plugin study (`references/studies/`).

pub mod analyzer;
pub mod ballistics;
pub mod db;
pub mod dc;
pub mod eq;
pub mod features;
pub mod layers;
pub mod warp;
pub mod weighting;
pub mod window;

pub use analyzer::{Analyzer, DspError, FrameData, PlanCache, SpectrumConfig};
pub use eq::EqConfig;
pub use features::SpectralFeatures;
pub use layers::{LayerRunner, LayerUnit, SequentialLayers};
pub use warp::{Axis, Warp};
pub use window::WindowKind;
