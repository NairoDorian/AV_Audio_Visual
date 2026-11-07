//! Data directories (startup step 2).
//!
//! Resolved before Tauri exists so the session log can start before any window. Portable mode:
//! a `Data/` directory next to the executable wins (ZER0 portable layout); otherwise the OS
//! per-user data directory joined with the bundle identifier.

use std::path::{Path, PathBuf};

/// Where the app keeps its config, scenes and logs.
#[derive(Debug, Clone)]
pub struct DataDirs {
    pub root: PathBuf,
    pub logs: PathBuf,
    pub portable: bool,
}

impl DataDirs {
    pub fn resolve(identifier: &str) -> Self {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf));
        if let Some(portable) = exe_dir.map(|d| d.join("Data")).filter(|d| d.is_dir()) {
            return Self::at(portable, true);
        }
        Self::at(os_data_dir().join(identifier), false)
    }

    fn at(root: PathBuf, portable: bool) -> Self {
        Self {
            logs: root.join("logs"),
            root,
            portable,
        }
    }
}

fn os_data_dir() -> PathBuf {
    let env = |k: &str| {
        std::env::var_os(k)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    };
    let home = || env("HOME").unwrap_or_else(std::env::temp_dir);
    if cfg!(target_os = "windows") {
        env("APPDATA").unwrap_or_else(std::env::temp_dir)
    } else if cfg!(target_os = "macos") {
        home().join("Library").join("Application Support")
    } else {
        env("XDG_DATA_HOME").unwrap_or_else(|| home().join(".local").join("share"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logs_live_under_the_root() {
        let d = DataDirs::at(PathBuf::from("x"), false);
        assert_eq!(d.logs, PathBuf::from("x").join("logs"));
    }

    #[test]
    fn resolved_root_ends_with_the_identifier_when_not_portable() {
        let d = DataDirs::resolve("com.example.test");
        assert!(d.portable || d.root.ends_with("com.example.test"));
    }
}
