//! Audio capture for AV_AudioVisual: backends, the real-time ring producer and the device model.
//!
//! The capture callback is real-time: no allocation, locks, logging, FFT or blocking sends.
//! See `docs/pre-plan.md` section 5.2 (capture contract).
