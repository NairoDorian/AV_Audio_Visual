//! Pure DSP for AV_AudioVisual.
//!
//! No threads, clocks, I/O or logging: every function is testable against golden vectors.
//! See `docs/pre-plan.md` section 5.3 (analyzer contract) and section 6 (numeric gates).
//!
//! Conceptual sources (no code copied): per-bar measured bands and the third-octave level
//! reference from the Tourne'Table and Neiz studies; the zero-allocation gate and off-thread
//! plan building from the FFT plugin study; time-based ballistics from the cross-project
//! synthesis (`references/studies/`).

pub mod analyzer;
pub mod ballistics;
pub mod bands;
pub mod dc;
pub mod eq;
pub mod layers;
pub mod weighting;
pub mod window;

pub use analyzer::{Analyzer, DspError, FrameData, PlanCache, SpectrumConfig};
pub use bands::LevelRef;
pub use eq::EqConfig;
pub use layers::{LayerRunner, LayerUnit, SequentialLayers};
pub use weighting::Weighting;
pub use window::WindowKind;
