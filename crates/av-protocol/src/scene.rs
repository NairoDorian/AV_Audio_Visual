//! `Scene` format v2: visual intent only. A scene never carries the audio device, monitor,
//! paths, hotkeys or GPU (AGENTS.md invariant 20); those live in `AppConfig`. Share codes
//! (Phase 3) wrap this same document with a version.

use serde::{Deserialize, Serialize};

use serde_json::{Map, Value};

use crate::pipeline::{Limit, Migration, Versioned, limit, slugify};

/// v2 (2026-11-08): the ring's `bars`/`inner` booleans became the six components of
/// docs/super-fft.md 3.2, and every scene gained the raw-audio `scope` block.
pub const FORMAT_VERSION: u32 = 2;
pub const MAX_PALETTE_COLORS: usize = 8;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VisualStyle {
    #[default]
    Bars,
    MirroredBars,
    Line,
    Area,
    /// The concentric ring of the ZER0 circular scope (docs/super-fft.md section 3).
    Concentric,
}

/// Parameters of the concentric style (docs/super-fft.md section 3). Present in every scene so
/// switching style keeps them. With perimeter radius `R` and the mirrored signal `C` in 0..1:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ring {
    /// Display bins the output bins are peak-pooled to; the ring has twice as many points.
    pub bins: u32,
    /// Values at or below the floor draw nothing (applied before the gain).
    pub floor: f32,
    /// Fixed display multiplier after the floor; never a dynamic normalization.
    pub gain: f32,
    /// Perimeter radius as a fraction of the half-size of the view.
    pub radius: f32,
    /// Closed line at `R (1 + C)`: the spectrum going outward.
    pub outer_line: bool,
    /// Closed line at `R (1 - C)`: the spectrum going inward.
    pub inner_line: bool,
    /// Radial bars from the perimeter out to the outer line.
    pub outer_bars: bool,
    /// Radial bars from the perimeter in to the inner line.
    pub inner_bars: bool,
    /// Radial bars from the inner line to the outer line, through the perimeter.
    pub span_bars: bool,
    /// The perimeter itself, faint.
    pub base_circle: bool,
    /// Bar length as a fraction of the line's distance from the perimeter: below 1 the lines
    /// outline the bars with a small gap.
    pub bar_scale: f32,
    /// Gap between neighbouring bars, CSS pixels.
    pub bar_spacing: f32,
}

impl Default for Ring {
    fn default() -> Self {
        Self {
            bins: 96,
            floor: 0.35,
            gain: 1.0,
            radius: 0.5,
            outer_line: true,
            inner_line: true,
            outer_bars: false,
            inner_bars: false,
            span_bars: true,
            base_circle: true,
            bar_scale: 0.92,
            bar_spacing: 1.5,
        }
    }
}

/// A custom colour per drawn component. `None` uses the palette (the gradient for bars and
/// lines, the first stop for the base circle, the last stop for the scope). Colours are
/// `#rrggbb` or `#rrggbbaa`; anything else is dropped back to the palette on load.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Colors {
    /// Bars of the bar styles.
    pub bars: Option<String>,
    pub outer_line: Option<String>,
    pub inner_line: Option<String>,
    pub outer_bars: Option<String>,
    pub inner_bars: Option<String>,
    pub span_bars: Option<String>,
    pub base_circle: Option<String>,
    /// Raw audio scope traces.
    pub scope: Option<String>,
}

impl Colors {
    fn slots(&mut self) -> [(&'static str, &mut Option<String>); 8] {
        [
            ("bars", &mut self.bars),
            ("outerLine", &mut self.outer_line),
            ("innerLine", &mut self.inner_line),
            ("outerBars", &mut self.outer_bars),
            ("innerBars", &mut self.inner_bars),
            ("spanBars", &mut self.span_bars),
            ("baseCircle", &mut self.base_circle),
            ("scope", &mut self.scope),
        ]
    }
}

/// The raw-audio scope (docs/super-fft.md section 4): a linear trace in the bar styles, traces
/// inside the ring in the concentric style.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeStyle {
    pub enabled: bool,
    /// Amplitude multiplier (full scale reaches the trace's half-height at 1).
    pub gain: f32,
    /// Trace length as a fraction of the inner diameter (ring) or the width (linear).
    pub size: f32,
    /// Adds a time-reversed copy (right to left), superposed.
    pub horizontal_mirror: bool,
    /// Adds an amplitude-inverted copy of every trace.
    pub vertical_mirror: bool,
    /// Rotational copies inside the ring: copy `i` is rotated by `i * 180 / copies` degrees.
    pub copies: u32,
    /// CSS pixels.
    pub line_width: f32,
}

impl Default for ScopeStyle {
    fn default() -> Self {
        Self {
            enabled: false,
            gain: 1.0,
            size: 0.9,
            horizontal_mirror: false,
            vertical_mirror: false,
            copies: 1,
            line_width: 1.5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scene {
    pub format_version: u32,
    /// Stable slug, also the file name of a user scene.
    pub id: String,
    pub name: String,
    pub style: VisualStyle,
    /// Display bars of the bar styles, peak-pooled from the analysis output bins.
    pub bar_count: u32,
    /// Bar width as a fraction of its slot (the rest is the gap).
    pub bar_width: f32,
    /// Corner radius as a fraction of half the bar width.
    pub corner_radius: f32,
    pub opacity: f32,
    /// Gradient stops, bottom to top, `#rrggbb` or `#rrggbbaa`.
    pub palette: Vec<String>,
    pub ring: Ring,
    pub scope: ScopeStyle,
    pub colors: Colors,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            format_version: FORMAT_VERSION,
            id: "bars".into(),
            name: "Bars".into(),
            style: VisualStyle::Bars,
            bar_count: 64,
            bar_width: 0.7,
            corner_radius: 1.0,
            opacity: 1.0,
            palette: vec!["#3d6bff".into(), "#8a5cff".into(), "#ff5ca8".into()],
            ring: Ring::default(),
            scope: ScopeStyle::default(),
            colors: Colors::default(),
        }
    }
}

/// Built-in scenes, generated from Rust (README section 4.8). User scenes arrive with scene files.
pub fn builtin_scenes() -> Vec<Scene> {
    vec![
        Scene::default(),
        Scene {
            id: "concentric".into(),
            name: "Concentric".into(),
            style: VisualStyle::Concentric,
            palette: vec!["#5ce1ff".into(), "#8a5cff".into(), "#ff5ca8".into()],
            scope: ScopeStyle {
                enabled: true,
                ..ScopeStyle::default()
            },
            ..Scene::default()
        },
    ]
}

/// The built-in scene with this id, or the default scene.
pub fn builtin(id: &str) -> Scene {
    builtin_scenes()
        .into_iter()
        .find(|s| s.id == id)
        .unwrap_or_default()
}

pub static LIMITS: &[Limit<Scene>] = &[
    limit!(Scene, "barCount", 4, 4_096, int, bar_count),
    limit!(Scene, "barWidth", 0.05, 1, float, bar_width),
    limit!(Scene, "cornerRadius", 0, 1, float, corner_radius),
    limit!(Scene, "opacity", 0, 1, float, opacity),
    limit!(Scene, "ring.bins", 12, 4_096, int, ring.bins),
    limit!(Scene, "ring.floor", 0, 0.9, float, ring.floor),
    limit!(Scene, "ring.gain", 0.05, 8, float, ring.gain),
    limit!(Scene, "ring.radius", 0.1, 0.5, float, ring.radius),
    limit!(Scene, "ring.barScale", 0.5, 1, float, ring.bar_scale),
    limit!(Scene, "ring.barSpacing", 0, 8, float, ring.bar_spacing),
    limit!(Scene, "scope.gain", 0.1, 16, float, scope.gain),
    limit!(Scene, "scope.size", 0.1, 1, float, scope.size),
    limit!(Scene, "scope.copies", 1, 8, int, scope.copies),
    limit!(Scene, "scope.lineWidth", 0.5, 8, float, scope.line_width),
];

/// v1 -> v2: `bars` + `inner` meant spanning bars (both), outward bars (bars only), or the two
/// loops (no bars: the outer line, plus the inner line when `inner`).
pub fn migrate_v1(doc: &mut Map<String, Value>) {
    let Some(Value::Object(ring)) = doc.get_mut("ring") else {
        return;
    };
    let bars = ring
        .remove("bars")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let inner = ring
        .remove("inner")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let set = |ring: &mut Map<String, Value>, k: &str, v: bool| {
        ring.insert(k.into(), Value::Bool(v));
    };
    set(ring, "spanBars", bars && inner);
    set(ring, "outerBars", bars && !inner);
    set(ring, "innerBars", false);
    set(ring, "outerLine", !bars);
    set(ring, "innerLine", !bars && inner);
    set(ring, "baseCircle", true);
    // v1 bars ran the full distance.
    ring.insert("barScale".into(), Value::from(1.0));
}

static MIGRATIONS: &[Migration] = &[Migration {
    from: 1,
    apply: migrate_v1,
}];

/// `#rgb`, `#rrggbb` or `#rrggbbaa`, case-insensitive; returns the canonical lower-case
/// `#rrggbb` / `#rrggbbaa` form.
pub fn normalize_color(input: &str) -> Option<String> {
    let hex = input.trim().strip_prefix('#')?;
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let hex = hex.to_ascii_lowercase();
    match hex.len() {
        3 => Some(format!(
            "#{}",
            hex.chars().flat_map(|c| [c, c]).collect::<String>()
        )),
        6 | 8 => Some(format!("#{hex}")),
        _ => None,
    }
}

impl Versioned for Scene {
    const VERSION_KEY: &'static str = "formatVersion";
    const CURRENT: u32 = FORMAT_VERSION;

    fn migrations() -> &'static [Migration] {
        MIGRATIONS
    }

    fn limits() -> &'static [Limit<Self>] {
        LIMITS
    }

    fn normalize(&mut self, notes: &mut Vec<String>) {
        let slug = slugify(&self.id, "scene");
        if slug != self.id {
            notes.push(format!("id {:?} normalized to {slug:?}", self.id));
            self.id = slug;
        }
        let trimmed: String = self.name.trim().chars().take(80).collect();
        if trimmed.is_empty() {
            self.name = self.id.clone();
            notes.push("empty name replaced by the id".into());
        } else if trimmed != self.name {
            self.name = trimmed;
            notes.push("name trimmed".into());
        }
        let before = self.palette.len();
        let mut colors: Vec<String> = self
            .palette
            .iter()
            .filter_map(|c| normalize_color(c))
            .take(MAX_PALETTE_COLORS)
            .collect();
        if colors.is_empty() {
            colors = Scene::default().palette;
            notes.push("palette had no valid colour; default palette restored".into());
        } else if colors.len() != before || colors != self.palette {
            notes.push("palette colours normalized".into());
        }
        self.palette = colors;
        for (name, slot) in self.colors.slots() {
            if let Some(c) = slot.as_deref() {
                let fixed = normalize_color(c);
                if fixed.as_deref() != Some(c) {
                    notes.push(match &fixed {
                        Some(_) => format!("colors.{name} normalized"),
                        None => format!("colors.{name} was not a colour; palette used"),
                    });
                    *slot = fixed;
                }
            }
        }
    }
}
