//! Panic hook (startup step 1, completed at step 7).
//!
//! Installed before anything else so a panic at any later step is logged and, once hosts exist,
//! hides every visualizer surface. `panic = "unwind"` (ADR-001) lets Drop guards run as well; the
//! hook is the second line of defence against a transparent always-on-top window outliving its
//! renderer.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

static INSTALLED: AtomicBool = AtomicBool::new(false);
static APP: OnceLock<tauri::AppHandle> = OnceLock::new();

/// Installs the hook once. Chains the previous hook so the default message still prints.
pub fn install() {
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let name = thread.name().unwrap_or("<unnamed>");
        tracing::error!(target: "av_app::panic", thread = name, "panic: {info}");
        hide_all_surfaces();
        previous(info);
    }));
}

pub fn is_installed() -> bool {
    INSTALLED.load(Ordering::SeqCst)
}

/// Step 7: give the hook the app handle so it can reach the host manager.
pub fn register_app(handle: tauri::AppHandle) {
    let _ = APP.set(handle);
}

/// No visualizer hosts exist yet (Phase 1B). When they do, this sends `HideAll` to the host
/// pump and destroys native host windows directly if the pump is dead.
fn hide_all_surfaces() {
    if APP.get().is_some() {
        tracing::warn!(target: "av_app::panic", "no visualizer surfaces registered yet");
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn install_is_idempotent() {
        super::install();
        super::install();
        assert!(super::is_installed());
    }
}
