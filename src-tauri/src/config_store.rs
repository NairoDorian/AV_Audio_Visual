//! Config persistence (startup step 6). The pure pipeline lives in `av_protocol::pipeline`; this
//! module owns the file:
//!
//! - **Load:** read `config.json`, run migrate -> heal -> sanitize -> normalize. If anything was
//!   lost or cannot be understood (corrupt, healed fields, a newer schema), copy the original to
//!   `config.json.bak-<unix seconds>` before the normalized form replaces it.
//! - **Save:** atomic (temp file + fsync + rename, never an in-place edit), debounced by 400 ms
//!   on one saver thread, flushed on exit. The saver parks on its channel when idle.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use av_protocol::AppConfig;
use av_protocol::pipeline::{self, Report};

pub const FILE_NAME: &str = "config.json";
pub const DEBOUNCE: Duration = Duration::from_millis(400);

/// Writes `text` to `path` atomically: a sibling temp file is written and synced, then renamed
/// over the target (`MoveFileExW(REPLACE_EXISTING)` on Windows, `rename(2)` elsewhere).
pub fn atomic_write(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

enum Msg {
    Save(String),
    Flush(Sender<()>),
}

/// The debounced writer thread.
struct Saver {
    tx: Option<Sender<Msg>>,
    thread: Option<JoinHandle<()>>,
}

impl Saver {
    fn spawn(path: PathBuf, debounce: Duration) -> std::io::Result<Self> {
        let (tx, rx) = mpsc::channel::<Msg>();
        let thread = std::thread::Builder::new()
            .name("av-config-saver".into())
            .spawn(move || {
                let mut pending: Option<String> = None;
                let write = |text: &str| {
                    if let Err(err) = atomic_write(&path, text) {
                        tracing::error!(target: "av_app::config", "saving {} failed: {err}", path.display());
                    }
                };
                loop {
                    // Idle: park until a message arrives. Pending: wait out the debounce.
                    let msg = match pending {
                        None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
                        Some(_) => rx.recv_timeout(debounce),
                    };
                    match msg {
                        Ok(Msg::Save(text)) => pending = Some(text),
                        Ok(Msg::Flush(ack)) => {
                            if let Some(text) = pending.take() {
                                write(&text);
                            }
                            let _ = ack.send(());
                        }
                        Err(RecvTimeoutError::Timeout) => {
                            if let Some(text) = pending.take() {
                                write(&text);
                            }
                        }
                        Err(RecvTimeoutError::Disconnected) => {
                            if let Some(text) = pending.take() {
                                write(&text);
                            }
                            return;
                        }
                    }
                }
            })?;
        Ok(Self {
            tx: Some(tx),
            thread: Some(thread),
        })
    }

    fn save(&self, text: String) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Msg::Save(text));
        }
    }

    fn flush(&self) {
        if let Some(tx) = &self.tx {
            let (ack_tx, ack_rx) = mpsc::channel();
            if tx.send(Msg::Flush(ack_tx)).is_ok() {
                let _ = ack_rx.recv_timeout(Duration::from_secs(5));
            }
        }
    }
}

impl Drop for Saver {
    fn drop(&mut self) {
        // Closing the channel makes the thread write anything pending and exit.
        self.tx.take();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// The live config plus its persistence.
pub struct ConfigStore {
    path: PathBuf,
    current: Mutex<AppConfig>,
    saver: Saver,
}

impl std::fmt::Debug for ConfigStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfigStore")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl ConfigStore {
    /// Loads (or creates) `<dir>/config.json`. Never fails on bad content; fails only if the
    /// saver thread cannot start.
    pub fn open(dir: &Path) -> std::io::Result<(Self, Report)> {
        Self::open_with(dir, DEBOUNCE)
    }

    fn open_with(dir: &Path, debounce: Duration) -> std::io::Result<(Self, Report)> {
        let path = dir.join(FILE_NAME);
        let (config, report, rewrite) = match std::fs::read_to_string(&path) {
            Ok(text) => {
                let loaded = pipeline::load::<AppConfig>(&text);
                if loaded.report.needs_backup() {
                    backup(&path);
                }
                let canonical = pipeline::to_text(&loaded.value);
                let rewrite = canonical != text.replace("\r\n", "\n");
                (loaded.value, loaded.report, rewrite)
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                (AppConfig::default(), Report::default(), true)
            }
            Err(err) => {
                tracing::error!(target: "av_app::config", "cannot read {}: {err}; using defaults", path.display());
                (
                    AppConfig::default(),
                    Report {
                        corrupt: true,
                        ..Report::default()
                    },
                    false,
                )
            }
        };
        if rewrite && let Err(err) = atomic_write(&path, &pipeline::to_text(&config)) {
            tracing::error!(target: "av_app::config", "cannot write {}: {err}", path.display());
        }
        let store = Self {
            saver: Saver::spawn(path.clone(), debounce)?,
            path,
            current: Mutex::new(config),
        };
        Ok((store, report))
    }

    fn lock(&self) -> MutexGuard<'_, AppConfig> {
        // A poisoned lock still holds a sanitized config; keep serving it.
        self.current.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn get(&self) -> AppConfig {
        self.lock().clone()
    }

    /// Applies a partial update and schedules a debounced save. Returns the applied (sanitized)
    /// value and what the pipeline changed, for the Studio's desired/applied diff.
    pub fn patch(&self, patch: &serde_json::Value) -> (AppConfig, Report) {
        let mut current = self.lock();
        let applied = pipeline::patch(&*current, patch);
        if applied.value != *current {
            *current = applied.value.clone();
            self.saver.save(pipeline::to_text(&applied.value));
        }
        (applied.value, applied.report)
    }

    /// Writes any pending save now. Called on exit.
    pub fn flush(&self) {
        self.saver.flush();
    }

    #[cfg(test)]
    fn path(&self) -> &Path {
        &self.path
    }
}

fn backup(path: &Path) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let mut name = path.as_os_str().to_owned();
    name.push(format!(".bak-{secs}"));
    match std::fs::copy(path, &name) {
        Ok(_) => {
            tracing::warn!(target: "av_app::config", "backed up the previous config to {}", PathBuf::from(&name).display())
        }
        Err(err) => tracing::error!(target: "av_app::config", "config backup failed: {err}"),
    }
}

/// One log line per load, never per field.
pub fn log_report(report: &Report) {
    if !report.changed() {
        tracing::info!(target: "av_app::config", "config loaded");
        return;
    }
    tracing::warn!(
        target: "av_app::config",
        corrupt = report.corrupt,
        migrated_from = ?report.migrated_from,
        newer_version = ?report.newer_version,
        filled = report.filled.len(),
        healed = ?report.healed,
        unknown = ?report.unknown,
        clamped = ?report.clamped,
        normalized = report.normalized.len(),
        "config loaded with repairs"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "av-config-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn backups(dir: &Path) -> usize {
        std::fs::read_dir(dir)
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .contains(".bak-")
            })
            .count()
    }

    #[test]
    fn atomic_write_leaves_no_temp_file() {
        let d = temp_dir("atomic");
        let p = d.join("x.json");
        atomic_write(&p, "one").unwrap();
        atomic_write(&p, "two").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "two");
        assert!(!d.join("x.json.tmp").exists());
    }

    #[test]
    fn a_missing_file_is_created_with_defaults() {
        let d = temp_dir("missing");
        let (store, report) = ConfigStore::open(&d).unwrap();
        assert!(!report.changed());
        assert_eq!(store.get(), AppConfig::default());
        let text = std::fs::read_to_string(store.path()).unwrap();
        assert_eq!(
            pipeline::load::<AppConfig>(&text).value,
            AppConfig::default()
        );
    }

    #[test]
    fn a_corrupt_file_is_backed_up_and_replaced() {
        let d = temp_dir("corrupt");
        std::fs::write(d.join(FILE_NAME), "{ not json").unwrap();
        let (store, report) = ConfigStore::open(&d).unwrap();
        assert!(report.corrupt);
        assert_eq!(backups(&d), 1);
        assert_eq!(store.get(), AppConfig::default());
        let text = std::fs::read_to_string(d.join(FILE_NAME)).unwrap();
        assert!(!pipeline::load::<AppConfig>(&text).report.corrupt);
    }

    #[test]
    fn a_partial_file_is_completed_without_a_backup() {
        let d = temp_dir("partial");
        std::fs::write(d.join(FILE_NAME), r#"{"analysis":{"barCount":64}}"#).unwrap();
        let (store, _) = ConfigStore::open(&d).unwrap();
        assert_eq!(store.get().analysis.bar_count, 64);
        assert_eq!(backups(&d), 0);
        let text = std::fs::read_to_string(d.join(FILE_NAME)).unwrap();
        assert!(
            text.contains("\"releaseMs\""),
            "normalized form written back"
        );
    }

    #[test]
    fn patches_are_debounced_then_flushed() {
        let d = temp_dir("debounce");
        let (store, _) = ConfigStore::open_with(&d, Duration::from_secs(60)).unwrap();
        for n in [10, 20, 30, 40] {
            store.patch(&json!({ "analysis": { "barCount": n } }));
        }
        // Within the (long) debounce nothing is written yet.
        let before = std::fs::read_to_string(d.join(FILE_NAME)).unwrap();
        assert_eq!(
            pipeline::load::<AppConfig>(&before)
                .value
                .analysis
                .bar_count,
            48
        );
        store.flush();
        let after = std::fs::read_to_string(d.join(FILE_NAME)).unwrap();
        assert_eq!(
            pipeline::load::<AppConfig>(&after).value.analysis.bar_count,
            40
        );
    }

    #[test]
    fn the_debounce_fires_on_its_own() {
        let d = temp_dir("timer");
        let (store, _) = ConfigStore::open_with(&d, Duration::from_millis(30)).unwrap();
        store.patch(&json!({ "performance": { "fpsCap": 30 } }));
        std::thread::sleep(Duration::from_millis(300));
        let text = std::fs::read_to_string(d.join(FILE_NAME)).unwrap();
        assert_eq!(
            pipeline::load::<AppConfig>(&text).value.performance.fps_cap,
            30
        );
    }

    #[test]
    fn dropping_the_store_writes_pending_changes() {
        let d = temp_dir("drop");
        {
            let (store, _) = ConfigStore::open_with(&d, Duration::from_secs(60)).unwrap();
            store.patch(&json!({ "scene": "neon" }));
        }
        let text = std::fs::read_to_string(d.join(FILE_NAME)).unwrap();
        assert_eq!(pipeline::load::<AppConfig>(&text).value.scene, "neon");
    }

    #[test]
    fn an_unchanged_patch_schedules_nothing() {
        let d = temp_dir("noop");
        let (store, _) = ConfigStore::open_with(&d, Duration::from_secs(60)).unwrap();
        let (applied, report) = store.patch(&json!({ "analysis": { "barCount": 48 } }));
        assert_eq!(applied, AppConfig::default());
        assert!(!report.changed());
    }
}
