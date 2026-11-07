//! `Scene` format v1: visual intent only. A scene never carries the audio device, monitor,
//! paths, hotkeys or GPU (AGENTS.md invariant 20); those live in `AppConfig`. Share codes
//! (Phase 3) wrap this same document with a version.

use serde::{Deserialize, Serialize};

use crate::pipeline::{Limit, Versioned, limit, slugify};

pub const FORMAT_VERSION: u32 = 1;
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

/// Parameters of the concentric style. Present in every scene so switching style keeps them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ring {
    /// Display bins the bars are peak-pooled to; the ring has twice as many points.
    pub bins: u32,
    /// Values at or below the floor draw nothing (applied before the gain).
    pub floor: f32,
    /// Fixed display multiplier after the floor; never a dynamic normalization.
    pub gain: f32,
    /// Ring radius as a fraction of the half-size of the view.
    pub radius: f32,
    /// Radial bars (true) or two closed loops (false).
    pub bars: bool,
    /// Draw the inner branch.
    pub inner: bool,
}

impl Default for Ring {
    fn default() -> Self {
        Self {
            bins: 96,
            floor: 0.35,
            gain: 1.0,
            radius: 0.5,
            bars: true,
            inner: true,
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
];

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
    }
}
