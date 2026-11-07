//! Analysis presets: a named, versioned copy of every analysis setting.
//!
//! There is one built-in preset, `default` (the [`AnalysisConfig`] defaults); every other preset
//! is saved by the user from the current settings and stored as `presets/<id>.json`. Applying a
//! preset copies its settings into the config; nothing keeps tracking it, so the Studio can
//! change any parameter afterwards (presets are never mandatory). A preset is portable: like a
//! scene it carries no device, monitor, path or hotkey (AGENTS.md invariant 20).
//!
//! Loading reuses the config pipeline: migrate -> self-heal -> sanitize (the analysis rows of
//! the config limit table) -> normalize (the same cross-field rules as the config).

use serde::{Deserialize, Serialize};

use crate::config::{AnalysisConfig, analysis_limits, normalize_analysis};
use crate::pipeline::{Limit, Versioned, limit, slugify};

pub const PRESET_VERSION: u32 = 1;
/// The id of the built-in preset; a user preset may not take it.
pub const DEFAULT_ID: &str = "default";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub preset_version: u32,
    /// Stable slug, also the file name.
    pub id: String,
    pub name: String,
    pub analysis: AnalysisConfig,
}

impl Default for Preset {
    fn default() -> Self {
        Self {
            preset_version: PRESET_VERSION,
            id: DEFAULT_ID.into(),
            name: "Default".into(),
            analysis: AnalysisConfig::default(),
        }
    }
}

impl Preset {
    /// A user preset named `name` holding `analysis`. The id is the slug of the name; the
    /// built-in id is suffixed so a user preset never shadows it.
    pub fn new(name: &str, analysis: AnalysisConfig) -> Self {
        let mut id = slugify(name, "preset");
        if id == DEFAULT_ID {
            id.push_str("-1");
        }
        let mut p = Self {
            preset_version: PRESET_VERSION,
            id,
            name: name.to_string(),
            analysis,
        };
        p.normalize(&mut Vec::new());
        p
    }

    pub fn is_builtin(&self) -> bool {
        self.id == DEFAULT_ID
    }
}

/// The built-in presets (one, by design, until more are curated).
pub fn builtin_presets() -> Vec<Preset> {
    vec![Preset::default()]
}

pub static LIMITS: [Limit<Preset>; 22] = analysis_limits!(Preset, analysis);

impl Versioned for Preset {
    const VERSION_KEY: &'static str = "presetVersion";
    const CURRENT: u32 = PRESET_VERSION;

    fn limits() -> &'static [Limit<Self>] {
        &LIMITS
    }

    fn normalize(&mut self, notes: &mut Vec<String>) {
        let slug = slugify(&self.id, "preset");
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
        normalize_analysis(&mut self.analysis, notes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::{load, to_text};

    #[test]
    fn round_trips_and_sanitizes_like_the_config() {
        let mut p = Preset::new("My Bass Preset!", AnalysisConfig::default());
        assert_eq!(p.id, "my-bass-preset");
        p.analysis.db_range = 1_000.0;
        let back = load::<Preset>(&to_text(&p));
        assert_eq!(back.value.analysis.db_range, 160.0);
        assert_eq!(back.report.clamped, vec!["analysis.dbRange".to_string()]);
    }

    #[test]
    fn a_user_preset_never_takes_the_builtin_id() {
        assert_eq!(
            Preset::new("Default", AnalysisConfig::default()).id,
            "default-1"
        );
        assert!(Preset::default().is_builtin());
    }

    #[test]
    fn missing_fields_are_filled_from_the_defaults() {
        let p =
            load::<Preset>(r#"{"presetVersion":1,"id":"x","name":"X","analysis":{"layers":2}}"#);
        assert_eq!(p.value.analysis.layers, 2);
        assert_eq!(p.value.analysis.fft_size, 32_768);
    }
}
