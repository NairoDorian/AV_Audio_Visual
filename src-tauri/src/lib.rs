//! AV_AudioVisual composition root (`av-app`).
//!
//! The only crate that sees every other crate. It wires the startup order, the Studio window,
//! the tray and the commands. Product logic lives in the `crates/` members.

mod cli;
mod commands;
mod identity;
mod logging;
mod panic;
mod paths;
mod preflight;
mod startup;

pub use startup::run;
