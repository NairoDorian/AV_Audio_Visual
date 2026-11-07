//! Low-rate status (blueprint 5.2): an `arc-swap` snapshot refreshed at most every 250 ms,
//! separate from the frame path. Consumers load it; nobody is pushed a 60 Hz event.

use std::sync::OnceLock;
use std::time::Instant;

/// Which clock produced frame timestamps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClockSource {
    /// `origin + samples / rate`: exact for files, arrival-anchored for live sources.
    #[default]
    SampleCount,
    /// The capture device's position clock mapped to the monotonic clock.
    Device,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StatusSnapshot {
    pub running: bool,
    pub source_rate: u32,
    pub channels: u16,
    pub fft_size: u32,
    pub hop_samples: u32,
    /// Output bins per frame.
    pub bar_count: u32,
    pub layers: u32,
    /// Window of the first layer, samples.
    pub window_samples: u32,
    /// Threads the layers run on (1 = the analysis thread only).
    pub threads: u32,
    /// Frequency of the loudest output bin in the newest frame.
    pub peak_hz: f32,
    /// Spectral features of the newest frame, when enabled (centroid Hz, rolloff Hz, flatness,
    /// flux, RMS dBFS, bass dB, mid dB, high dB).
    pub features: Option<[f32; 8]>,
    pub generation: u32,
    pub clock: ClockSource,
    /// Frames published since the engine started.
    pub frames: u64,
    /// Analysis cost over the last status window.
    pub dsp_us_avg: f32,
    pub dsp_us_max: f32,
    pub overruns: u64,
    pub dropped_samples: u64,
    /// The newest frame was silent.
    pub silent: bool,
    /// Hops of synthetic silence published while the source was starved.
    pub starved_hops: u64,
    /// Peak raw input over the last status window, dBFS (-inf for digital silence).
    pub input_peak_dbfs: f32,
    pub updated_us: u64,
}

/// Microseconds on the process monotonic clock (the time base for frame timestamps).
pub fn monotonic_us() -> u64 {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    EPOCH.get_or_init(Instant::now).elapsed().as_micros() as u64
}
