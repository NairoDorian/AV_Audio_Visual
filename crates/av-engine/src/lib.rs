//! Analysis engine for AV_AudioVisual: one analysis worker per active source publishing into a
//! preallocated frame slot ring. See `docs/phase-1a-blueprint.md` section 5.
//!
//! Conceptual source: ZER0's callback -> `rtrb` -> worker split, latest-frame bus and
//! generation counters (`references/studies/zer0-audio-capture-and-fft.md`); no code copied.

pub mod engine;
pub mod pipeline;
pub mod preview;
pub mod slot_ring;
pub mod status;

pub use engine::{Engine, EngineError};
pub use slot_ring::{FrameCopy, FrameReader, ReadError, SlotWriter, slot_ring};
pub use status::{StatusSnapshot, monotonic_us};
