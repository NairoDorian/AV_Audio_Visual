//! Native hosts (blueprint A10 and section 10): monitors with stable ids, DPI, the taskbar, and
//! the webview-less windows of the three placements. Hands raw window handles to the renderer;
//! knows nothing about wgpu (crate graph, blueprint 3.1).
//!
//! [`geometry`] is pure and OS-independent (anchors, taskbar zones). The hosts are Windows-only
//! for now; elsewhere [`Hosts::start`] reports that the platform is not supported yet.

pub mod geometry;

#[cfg(windows)]
mod win;

#[cfg(windows)]
pub use win::{HostEvent, HostKind, HostWindow, Hosts, emergency_hide_all};

#[cfg(not(windows))]
mod unsupported {
    use crate::geometry::{Monitor, Rect, Taskbar};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum HostKind {
        Overlay {
            click_through: bool,
            capture_visible: bool,
        },
        Taskbar,
        Background,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum HostEvent {
        DisplayChanged,
        ExplorerRestarted,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct HostWindow {
        pub hwnd: isize,
        pub kind: HostKind,
    }

    impl HostWindow {
        pub fn raw_window_handle(&self) -> Option<raw_window_handle::RawWindowHandle> {
            None
        }
        pub fn raw_display_handle(&self) -> raw_window_handle::RawDisplayHandle {
            raw_window_handle::RawDisplayHandle::Windows(
                raw_window_handle::WindowsDisplayHandle::new(),
            )
        }
    }

    #[derive(Debug)]
    pub struct Hosts {
        pub events: std::sync::mpsc::Receiver<HostEvent>,
    }

    impl Hosts {
        pub fn start() -> Result<Self, String> {
            Err("desktop surfaces are Windows-only for now".into())
        }
        pub fn create(&self, _: HostKind, _: Rect) -> Result<HostWindow, String> {
            Err("unsupported".into())
        }
        pub fn place(&self, _: &HostWindow, _: Rect) {}
        pub fn show(&self, _: &HostWindow) {}
        pub fn destroy(&self, _: &HostWindow) {}
        pub fn monitors(&self) -> Vec<Monitor> {
            Vec::new()
        }
        pub fn taskbar(&self) -> Option<Taskbar> {
            None
        }
    }

    pub fn emergency_hide_all() {}
}

#[cfg(not(windows))]
pub use unsupported::{HostEvent, HostKind, HostWindow, Hosts, emergency_hide_all};
