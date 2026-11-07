//! Shared types and pure functions for AV_AudioVisual.
//!
//! This crate has no internal dependencies and no threads, I/O, GPU or OS APIs: it is shared by
//! every other crate, by the fixture dumper and (through generated fixtures) by the TypeScript
//! parity tests. See `docs/phase-1a-blueprint.md` section 3.

pub mod config;
pub mod fixtures;
pub mod frame;
pub mod pipeline;
pub mod placement;
pub mod scene;
pub mod wire;

pub use config::AppConfig;
pub use frame::{FrameFlags, FrameHeader, WIRE_VERSION, WireError};
pub use pipeline::{Loaded, Report, Versioned};
pub use scene::Scene;
