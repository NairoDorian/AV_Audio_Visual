//! Audio capture for AV_AudioVisual: backends, the real-time ring and the device model.
//!
//! The capture callback is real-time: no allocation, locks, logging, FFT or blocking sends.
//! See `docs/pre-plan.md` section 5.2 (capture contract). Live system-output capture (WASAPI
//! loopback) arrives after spike P0.3; the file source is the deterministic source until then.

pub mod capture;
pub mod file;
pub mod ring;

pub use capture::{CaptureBackend, CaptureError, CaptureFormat, CaptureState};
pub use file::{DecodedAudio, FileSource, decode_file};
pub use ring::{AudioSink, AudioSource, RingCounters, ring};
