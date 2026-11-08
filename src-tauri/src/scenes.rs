//! Scenes on disk: `scenes/<id>.json` under the data root. A saved scene with a built-in id is
//! that built-in, edited; "reset" deletes the file and the built-in returns. Every file goes
//! through the scene pipeline on read (migrate -> self-heal -> sanitize -> normalize), so a
//! hand-edited or older file cannot carry an out-of-range value to a renderer. Scenes carry no
//! machine-specific setting (AGENTS.md invariant 20), so the files can be shared as they are.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use av_protocol::pipeline::{load, slugify, to_text};
use av_protocol::scene::{Scene, builtin_scenes};

use crate::config_store::atomic_write;

pub const DIR_NAME: &str = "scenes";
pub const MAX_SCENES: usize = 256;

#[derive(Debug)]
pub struct SceneStore {
    dir: PathBuf,
    write: Mutex<()>,
}

impl SceneStore {
    pub fn new(root: &Path) -> Self {
        Self {
            dir: root.join(DIR_NAME),
            write: Mutex::new(()),
        }
    }

    fn path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    fn user_scenes(&self) -> Vec<Scene> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .filter_map(|p| std::fs::read_to_string(&p).ok())
            .map(|text| load::<Scene>(&text))
            .filter(|l| !l.report.corrupt)
            .map(|l| l.value)
            .take(MAX_SCENES)
            .collect()
    }

    /// Built-ins (with any saved edit applied) first, then user-only scenes by name.
    pub fn list(&self) -> Vec<Scene> {
        let mut user = self.user_scenes();
        let mut out: Vec<Scene> = builtin_scenes()
            .into_iter()
            .map(|b| match user.iter().position(|u| u.id == b.id) {
                Some(i) => user.swap_remove(i),
                None => b,
            })
            .collect();
        user.sort_by_key(|s| s.name.to_lowercase());
        out.extend(user);
        out
    }

    /// The scene `id`, or the default scene when it does not exist.
    pub fn get(&self, id: &str) -> Scene {
        let all = self.list();
        all.iter()
            .find(|s| s.id == id)
            .cloned()
            .unwrap_or_else(|| all.into_iter().next().unwrap_or_default())
    }

    /// True when `id` is a built-in with a saved edit.
    pub fn is_modified(&self, id: &str) -> bool {
        builtin_scenes().iter().any(|b| b.id == id) && self.path(id).exists()
    }

    /// Saves `scene` (sanitized first); same id overwrites.
    pub fn save(&self, scene: &Scene) -> Result<Scene, String> {
        let text = to_text(scene);
        let clean = load::<Scene>(&text).value;
        let _guard = self.write.lock().unwrap_or_else(|e| e.into_inner());
        let exists = self.path(&clean.id).exists();
        if !exists && self.user_scenes().len() >= MAX_SCENES {
            return Err(format!("at most {MAX_SCENES} scenes"));
        }
        atomic_write(&self.path(&clean.id), &to_text(&clean)).map_err(|e| e.to_string())?;
        Ok(clean)
    }

    /// Deletes a saved scene (a built-in returns to its original look).
    pub fn reset(&self, id: &str) -> Result<(), String> {
        let slug = slugify(id, "");
        if slug != id || slug.is_empty() {
            return Err("not a scene id".into());
        }
        let _guard = self.write.lock().unwrap_or_else(|e| e.into_inner());
        match std::fs::remove_file(self.path(id)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("av-scenes-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn an_edited_builtin_overrides_and_resets() {
        let root = temp_root("edit");
        let store = SceneStore::new(&root);
        let mut ring = store.get("concentric");
        assert!(!store.is_modified("concentric"));
        ring.ring.bins = 300;
        ring.colors.outer_line = Some("#ffd400".into());
        store.save(&ring).unwrap();
        assert!(store.is_modified("concentric"));
        let back = store.get("concentric");
        assert_eq!(back.ring.bins, 300);
        assert_eq!(back.colors.outer_line.as_deref(), Some("#ffd400"));
        assert_eq!(
            store.list().len(),
            2,
            "no duplicate entry for an edited built-in"
        );
        store.reset("concentric").unwrap();
        assert_eq!(store.get("concentric").ring.bins, 96);
        assert!(store.reset("../config").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_saved_scene_is_sanitized() {
        let root = temp_root("clean");
        let store = SceneStore::new(&root);
        let mut s = store.get("bars");
        s.bar_count = 1_000_000;
        let saved = store.save(&s).unwrap();
        assert_eq!(saved.bar_count, 4096);
        let _ = std::fs::remove_dir_all(&root);
    }
}
