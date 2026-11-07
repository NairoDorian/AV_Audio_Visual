//! Placement geometry (blueprint section 10.1). Geometry is an anchor, never absolute pixels:
//! re-resolved on display, DPI, work-area and taskbar changes. Machine-specific, so it lives in
//! `AppConfig` and is excluded from scenes and share codes.

use serde::{Deserialize, Serialize};

/// The three places the visual lives (README section 1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Placement {
    Background,
    Taskbar,
    #[default]
    Overlay,
}

/// The rectangle an anchor is resolved against.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AnchorFrame {
    #[default]
    WorkArea,
    FullMonitor,
    VirtualDesktop,
    TaskbarBand,
}

/// Where in the frame the surface is pinned.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AnchorPoint {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    Center,
    CenterRight,
    BottomLeft,
    #[default]
    BottomCenter,
    BottomRight,
    /// Offset from the frame's top-left corner.
    Free,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Orientation {
    #[default]
    Horizontal,
    Vertical,
}

/// Monitor + anchor + offset + size in DIPs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Anchor {
    /// Stable monitor device identity (not an index). `None` = the primary monitor.
    pub monitor: Option<String>,
    pub frame: AnchorFrame,
    pub anchor: AnchorPoint,
    pub offset_dip: [f32; 2],
    pub size_dip: [f32; 2],
    pub orientation: Orientation,
}

impl Default for Anchor {
    fn default() -> Self {
        Self {
            monitor: None,
            frame: AnchorFrame::WorkArea,
            anchor: AnchorPoint::BottomCenter,
            offset_dip: [0.0, -24.0],
            size_dip: [720.0, 160.0],
            orientation: Orientation::Horizontal,
        }
    }
}

/// Locked = click-through; Edit = drag, resize and hot zones.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InputMode {
    #[default]
    Locked,
    Edit,
}
