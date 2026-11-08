//! AV_AudioVisual composition root (`av-app`).
//!
//! The only crate that sees every other crate. It wires the startup order, the Studio window,
//! the tray and the commands. Product logic lives in the `crates/` members.

mod audio;
mod cli;
mod commands;
mod config_store;
mod displays;
mod identity;
mod logging;
mod panic;
mod paths;
mod preflight;
mod presets;
mod scenes;
mod startup;

pub use startup::run;
