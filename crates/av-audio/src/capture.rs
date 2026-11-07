//! The capture contract (pre-plan 5.2 and 5.7). Backends accept the endpoint's mix format and
//! push interleaved f32 into an [`AudioSink`]; rate and channel normalization happen in the
//! analysis worker, never in the callback.

use crate::ring::AudioSink;

/// What a backend delivers into the ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureFormat {
    pub sample_rate: u32,
    pub channels: u16,
}

/// Typed, user-visible capture errors. Each maps to a Studio card with a fix action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureError {
    PermissionDenied,
    DeviceNotFound,
    UnsupportedFormat(String),
    DeviceLost,
    BackendUnavailable(String),
    Timeout,
    /// File sources: unreadable, unknown container or codec.
    Decode(String),
}

impl std::fmt::Display for CaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PermissionDenied => f.write_str("permission to capture audio was denied"),
            Self::DeviceNotFound => f.write_str("the audio device was not found"),
            Self::UnsupportedFormat(why) => write!(f, "unsupported audio format: {why}"),
            Self::DeviceLost => f.write_str("the audio device was disconnected"),
            Self::BackendUnavailable(why) => write!(f, "audio backend unavailable: {why}"),
            Self::Timeout => f.write_str("the audio device stopped responding"),
            Self::Decode(why) => write!(f, "cannot read the audio file: {why}"),
        }
    }
}

impl std::error::Error for CaptureError {}

/// Coarse state for diagnostics and the Studio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CaptureState {
    #[default]
    Stopped,
    Running,
    /// The source ran out (file end without looping).
    Finished,
    Failed,
}

/// One capture source. `start` hands the backend the producer end of the ring; the backend
/// owns its thread or callback from then until `stop`.
pub trait CaptureBackend: Send {
    fn format(&self) -> CaptureFormat;
    fn start(&mut self, sink: AudioSink) -> Result<(), CaptureError>;
    fn stop(&mut self);
    fn state(&self) -> CaptureState;
}
