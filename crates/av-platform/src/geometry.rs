//! Placement geometry, pure and OS-independent (blueprint 10.1): anchors and taskbar zones
//! resolved against physical-pixel rectangles. The hosts call these on every display, DPI,
//! work-area or taskbar change; nothing is stored in pixels.

use av_protocol::config::TaskbarZone;
use av_protocol::placement::{Anchor, AnchorFrame, AnchorPoint};

/// A rectangle in physical pixels (virtual-desktop coordinates).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    pub const fn right(&self) -> i32 {
        self.x + self.w
    }

    pub const fn bottom(&self) -> i32 {
        self.y + self.h
    }

    pub fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }

    /// Shrinks by `m` pixels on every side (never below zero size).
    pub fn inset(&self, m: i32) -> Self {
        Self {
            x: self.x + m,
            y: self.y + m,
            w: (self.w - 2 * m).max(0),
            h: (self.h - 2 * m).max(0),
        }
    }

    /// The intersection with `o` (empty when they do not overlap).
    pub fn clip(&self, o: &Rect) -> Self {
        let x = self.x.max(o.x);
        let y = self.y.max(o.y);
        let r = self.right().min(o.right());
        let b = self.bottom().min(o.bottom());
        Self::new(x, y, (r - x).max(0), (b - y).max(0))
    }
}

/// A monitor as the hosts see it.
#[derive(Debug, Clone, PartialEq)]
pub struct Monitor {
    /// Stable device identity (the adapter output name, e.g. `\\.\DISPLAY1`), not an index.
    pub id: String,
    pub rect: Rect,
    pub work: Rect,
    /// Physical pixels per DIP (1.0 at 96 DPI).
    pub scale: f32,
    pub primary: bool,
}

/// The monitor `id` names, or the primary one (or the first) when it is gone or `None`.
pub fn pick_monitor<'a>(monitors: &'a [Monitor], id: Option<&str>) -> Option<&'a Monitor> {
    id.and_then(|id| monitors.iter().find(|m| m.id == id))
        .or_else(|| monitors.iter().find(|m| m.primary))
        .or_else(|| monitors.first())
}

/// Which screen edge the taskbar is docked to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Left,
    Top,
    Right,
    Bottom,
}

impl Edge {
    pub const fn horizontal(self) -> bool {
        matches!(self, Self::Top | Self::Bottom)
    }
}

/// The taskbar as reported by the shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Taskbar {
    pub rect: Rect,
    pub edge: Edge,
    pub auto_hide: bool,
}

/// The strip over a zone of the taskbar: `length` and `offset` are fractions of the bar's long
/// side, `margin` (physical pixels) keeps the strip off the bar's edges.
pub fn taskbar_zone(
    bar: &Taskbar,
    zone: TaskbarZone,
    length: f32,
    offset: f32,
    margin: i32,
) -> Rect {
    let length = length.clamp(0.0, 1.0);
    let start = match zone {
        TaskbarZone::Start => 0.0,
        TaskbarZone::Center => (1.0 - length) / 2.0,
        TaskbarZone::End => 1.0 - length,
        TaskbarZone::Custom => offset.clamp(0.0, 1.0 - length),
    };
    let r = bar.rect;
    let along = if bar.edge.horizontal() { r.w } else { r.h } as f32;
    let a0 = (start * along).round() as i32;
    let a1 = ((start + length) * along).round() as i32;
    let strip = if bar.edge.horizontal() {
        Rect::new(r.x + a0, r.y, a1 - a0, r.h)
    } else {
        Rect::new(r.x, r.y + a0, r.w, a1 - a0)
    };
    strip.inset(margin)
}

/// Resolves an overlay anchor (DIP offset and size) on a monitor.
pub fn resolve_anchor(anchor: &Anchor, monitor: &Monitor, virtual_desktop: Rect) -> Rect {
    let frame = match anchor.frame {
        AnchorFrame::WorkArea | AnchorFrame::TaskbarBand => monitor.work,
        AnchorFrame::FullMonitor => monitor.rect,
        AnchorFrame::VirtualDesktop => virtual_desktop,
    };
    let s = monitor.scale;
    let w = (anchor.size_dip[0] * s).round() as i32;
    let h = (anchor.size_dip[1] * s).round() as i32;
    let w = w.min(frame.w).max(1);
    let h = h.min(frame.h).max(1);
    let (ax, ay) = match anchor.anchor {
        AnchorPoint::TopLeft | AnchorPoint::Free => (0.0, 0.0),
        AnchorPoint::TopCenter => (0.5, 0.0),
        AnchorPoint::TopRight => (1.0, 0.0),
        AnchorPoint::CenterLeft => (0.0, 0.5),
        AnchorPoint::Center => (0.5, 0.5),
        AnchorPoint::CenterRight => (1.0, 0.5),
        AnchorPoint::BottomLeft => (0.0, 1.0),
        AnchorPoint::BottomCenter => (0.5, 1.0),
        AnchorPoint::BottomRight => (1.0, 1.0),
    };
    let x = frame.x as f32 + (frame.w - w) as f32 * ax + anchor.offset_dip[0] * s;
    let y = frame.y as f32 + (frame.h - h) as f32 * ay + anchor.offset_dip[1] * s;
    // Keep it on the frame (a stale offset must never park the overlay off screen).
    let x = (x.round() as i32).clamp(frame.x, frame.right() - w);
    let y = (y.round() as i32).clamp(frame.y, frame.bottom() - h);
    Rect::new(x, y, w, h)
}

/// The background visual: the monitor, or a centred square-ish region scaled by `scale`.
pub fn background_rect(monitor: &Monitor, scale: f32) -> Rect {
    let r = monitor.rect;
    if scale >= 0.999 {
        return r;
    }
    let w = (r.w as f32 * scale.clamp(0.1, 1.0)).round() as i32;
    let h = (r.h as f32 * scale.clamp(0.1, 1.0)).round() as i32;
    Rect::new(r.x + (r.w - w) / 2, r.y + (r.h - h) / 2, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor() -> Monitor {
        Monitor {
            id: r"\\.\DISPLAY1".into(),
            rect: Rect::new(0, 0, 2560, 1440),
            work: Rect::new(0, 0, 2560, 1392),
            scale: 1.5,
            primary: true,
        }
    }

    #[test]
    fn zones_cover_their_part_of_a_horizontal_bar() {
        let bar = Taskbar {
            rect: Rect::new(0, 1392, 2560, 48),
            edge: Edge::Bottom,
            auto_hide: false,
        };
        assert_eq!(
            taskbar_zone(&bar, TaskbarZone::Start, 0.25, 0.0, 4),
            Rect::new(4, 1396, 632, 40)
        );
        assert_eq!(
            taskbar_zone(&bar, TaskbarZone::End, 0.25, 0.0, 0),
            Rect::new(1920, 1392, 640, 48)
        );
        assert_eq!(
            taskbar_zone(&bar, TaskbarZone::Center, 0.5, 0.0, 0),
            Rect::new(640, 1392, 1280, 48)
        );
        // A custom offset that would run off the end is pulled back onto the bar.
        assert_eq!(
            taskbar_zone(&bar, TaskbarZone::Custom, 0.25, 0.9, 0).right(),
            2560
        );
    }

    #[test]
    fn zones_run_down_a_vertical_bar() {
        let bar = Taskbar {
            rect: Rect::new(0, 0, 60, 1440),
            edge: Edge::Left,
            auto_hide: false,
        };
        let z = taskbar_zone(&bar, TaskbarZone::Start, 0.5, 0.0, 0);
        assert_eq!(z, Rect::new(0, 0, 60, 720));
    }

    #[test]
    fn anchors_scale_with_dpi_and_stay_on_screen() {
        let m = monitor();
        let a = Anchor::default(); // bottom centre, 720 x 160 DIP, offset (0, -24)
        let r = resolve_anchor(&a, &m, m.rect);
        assert_eq!((r.w, r.h), (1080, 240));
        assert_eq!(r.x, (2560 - 1080) / 2);
        assert_eq!(r.bottom(), 1392 - 36);
        let far = Anchor {
            offset_dip: [10_000.0, 10_000.0],
            ..Anchor::default()
        };
        let r = resolve_anchor(&far, &m, m.rect);
        assert!(r.right() <= m.work.right() && r.bottom() <= m.work.bottom());
    }

    #[test]
    fn a_missing_monitor_falls_back_to_the_primary() {
        let ms = vec![
            Monitor {
                primary: false,
                id: "B".into(),
                ..monitor()
            },
            monitor(),
        ];
        assert_eq!(pick_monitor(&ms, Some("gone")).unwrap().id, ms[1].id);
        assert_eq!(pick_monitor(&ms, Some("B")).unwrap().id, "B");
    }

    #[test]
    fn the_background_can_be_a_centred_region() {
        let m = monitor();
        assert_eq!(background_rect(&m, 1.0), m.rect);
        assert_eq!(background_rect(&m, 0.5), Rect::new(640, 360, 1280, 720));
    }
}
