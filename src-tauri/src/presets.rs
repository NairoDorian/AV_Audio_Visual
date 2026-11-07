//! User analysis presets on disk: `presets/<id>.json` under the data root, one file per preset,
//! written atomically. The built-in `default` preset is never written; it is
//! [`av_protocol::preset::Preset::default`]. Every file goes through the preset pipeline on read
//! (migrate -> self-heal -> sanitize -> normalize), so a hand-edited or older file cannot carry
//! an out-of-range value into the engine. A file that does not parse is skipped, not deleted.
//!
//! Preset names are user content: they are never logged in release builds.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use av_protocol::config::AnalysisConfig;
use av_protocol::pipeline::{load, to_text};
use av_protocol::preset::{DEFAULT_ID, Preset, builtin_presets};
use serde::Serialize;

use crate::config_store::atomic_write;

pub const DIR_NAME: &str = "presets";
/// More than anyone curates by hand; keeps a runaway loop from filling the disk.
pub const MAX_PRESETS: usize = 256;

/// What the Studio lists.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetInfo {
    pub id: String,
    pub name: String,
    pub builtin: bool,
}

#[derive(Debug)]
pub struct PresetStore {
    dir: PathBuf,
    /// Serializes writers (two saves of one name must not interleave their renames).
    write: Mutex<()>,
}

impl PresetStore {
    pub fn new(root: &Path) -> Self {
        Self {
            dir: root.join(DIR_NAME),
            write: Mutex::new(()),
        }
    }

    fn path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    fn user_presets(&self) -> Vec<Preset> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut out: Vec<Preset> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .filter_map(|p| std::fs::read_to_string(&p).ok())
            .map(|text| load::<Preset>(&text))
            .filter(|l| !l.report.corrupt)
            .map(|l| l.value)
            .filter(|p| p.id != DEFAULT_ID)
            .take(MAX_PRESETS)
            .collect();
        out.sort_by_key(|p| p.name.to_lowercase());
        out
    }

    /// Built-in first, then user presets by name.
    pub fn list(&self) -> Vec<PresetInfo> {
        builtin_presets()
            .into_iter()
            .chain(self.user_presets())
            .map(|p| PresetInfo {
                builtin: p.is_builtin(),
                id: p.id,
                name: p.name,
            })
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<Preset> {
        if id == DEFAULT_ID {
            return Some(Preset::default());
        }
        self.user_presets().into_iter().find(|p| p.id == id)
    }

    /// Saves `analysis` under `name` (same name, same id: overwrites).
    pub fn save(&self, name: &str, analysis: AnalysisConfig) -> Result<PresetInfo, String> {
        if name.trim().is_empty() {
            return Err("a preset needs a name".into());
        }
        let preset = Preset::new(name.trim(), analysis);
        let _guard = self.write.lock().unwrap_or_else(|e| e.into_inner());
        let exists = self.path(&preset.id).exists();
        if !exists && self.user_presets().len() >= MAX_PRESETS {
            return Err(format!("at most {MAX_PRESETS} presets"));
        }
        atomic_write(&self.path(&preset.id), &to_text(&preset)).map_err(|e| e.to_string())?;
        tracing::info!(target: "av_app::presets", overwrite = exists, "preset saved");
        Ok(PresetInfo {
            id: preset.id,
            name: preset.name,
            builtin: false,
        })
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        if id == DEFAULT_ID {
            return Err("the default preset cannot be deleted".into());
        }
        let slug = av_protocol::pipeline::slugify(id, "");
        if slug != id || slug.is_empty() {
            return Err("not a preset id".into());
        }
        let _guard = self.write.lock().unwrap_or_else(|e| e.into_inner());
        std::fs::remove_file(self.path(id)).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("av-presets-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn save_list_get_and_delete() {
        let root = temp_root("crud");
        let store = PresetStore::new(&root);
        assert_eq!(store.list().len(), 1, "only the built-in default");
        let analysis = AnalysisConfig {
            layers: 2,
            ..AnalysisConfig::default()
        };
        let info = store.save("Bass Heavy", analysis.clone()).unwrap();
        assert_eq!(info.id, "bass-heavy");
        let listed = store.list();
        assert_eq!(listed.len(), 2);
        assert!(listed[0].builtin && !listed[1].builtin);
        assert_eq!(store.get("bass-heavy").unwrap().analysis, analysis);
        // Saving the same name overwrites.
        store.save("bass heavy", AnalysisConfig::default()).unwrap();
        assert_eq!(store.list().len(), 2);
        assert_eq!(store.get("bass-heavy").unwrap().analysis.layers, 4);
        store.delete("bass-heavy").unwrap();
        assert_eq!(store.list().len(), 1);
        assert!(store.delete(DEFAULT_ID).is_err());
        assert!(store.delete("../config").is_err());
        assert!(store.save("  ", AnalysisConfig::default()).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_hand_edited_file_is_sanitized_and_garbage_is_skipped() {
        let root = temp_root("heal");
        let store = PresetStore::new(&root);
        std::fs::create_dir_all(root.join(DIR_NAME)).unwrap();
        std::fs::write(
            root.join(DIR_NAME).join("wild.json"),
            r#"{"presetVersion":1,"id":"wild","name":"Wild","analysis":{"dbRange":9999}}"#,
        )
        .unwrap();
        std::fs::write(root.join(DIR_NAME).join("junk.json"), "not json").unwrap();
        let p = store.get("wild").unwrap();
        assert_eq!(p.analysis.db_range, 160.0);
        assert_eq!(store.list().len(), 2);
        let _ = std::fs::remove_dir_all(&root);
    }
}
