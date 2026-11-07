//! The startup order (`docs/pre-plan.md` section 5.1). The order is a safety property: the panic
//! hook exists before any window, and the preflight runs before any webview.
//!
//! A failure before step 8 may print and exit. A failure after a full-desktop window exists must
//! use the panic/cleanup path.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

use crate::audio::{AudioService, SourceRequest};
use crate::cli::CliArgs;
use crate::config_store::{self, ConfigStore};
use crate::paths::DataDirs;
use crate::{commands, identity, logging, panic, preflight};

/// Label of the Studio window, the only webview (ADR-013). `capabilities/studio.json` grants
/// permissions to this label alone.
pub const STUDIO_LABEL: &str = "studio";

/// The ten steps, in order. `ORDER` is the documented sequence; `enter` enforces it at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    PanicHook = 1,
    DataDirs,
    SessionLog,
    Cli,
    Preflight,
    Config,
    RegisterApp,
    StudioAndTray,
    HostsHidden,
    Pipeline,
}

#[cfg_attr(not(test), allow(dead_code))]
pub const ORDER: [Step; 10] = [
    Step::PanicHook,
    Step::DataDirs,
    Step::SessionLog,
    Step::Cli,
    Step::Preflight,
    Step::Config,
    Step::RegisterApp,
    Step::StudioAndTray,
    Step::HostsHidden,
    Step::Pipeline,
];

/// The last step entered; steps must be entered in `ORDER`.
static LAST_STEP: AtomicU8 = AtomicU8::new(0);

fn enter(step: Step) {
    let previous = LAST_STEP.swap(step as u8, Ordering::SeqCst);
    assert_eq!(
        previous + 1,
        step as u8,
        "startup step {step:?} entered out of order"
    );
}

fn reached(step: Step) -> bool {
    LAST_STEP.load(Ordering::SeqCst) >= step as u8
}

pub fn run() {
    // 1. Panic hook, before anything that can fail.
    enter(Step::PanicHook);
    panic::install();

    // 2. Data directories. The identifier comes from tauri.conf.json at compile time.
    enter(Step::DataDirs);
    let context = tauri::generate_context!();
    let dirs = DataDirs::resolve(&context.config().identifier);

    // 3. Session log.
    enter(Step::SessionLog);
    let _log_guard = logging::init(&dirs.logs);
    tracing::info!(
        target: "av_app::startup",
        version = env!("CARGO_PKG_VERSION"),
        portable = dirs.portable,
        data = %dirs.root.display(),
        "starting"
    );

    // 4. CLI.
    enter(Step::Cli);
    let cli = CliArgs::parse(std::env::args());
    if !cli.unknown.is_empty() {
        tracing::warn!(target: "av_app::startup", args = ?cli.unknown, "ignoring unknown arguments");
    }

    // 5. Development preflight: no webview unless the dev server serves the Studio.
    enter(Step::Preflight);
    if let Err(why) = preflight::check(cfg!(debug_assertions)) {
        tracing::error!(target: "av_app::startup", "{why}");
        eprintln!("{why}");
        std::process::exit(2);
    }

    // 6. Config: migrate -> self-heal -> sanitize -> normalize, backed up if anything was lost.
    enter(Step::Config);
    let store = match ConfigStore::open(&dirs.root) {
        Ok((store, report)) => {
            config_store::log_report(&report);
            Arc::new(store)
        }
        Err(err) => {
            let why = format!("cannot start the config saver: {err}");
            tracing::error!(target: "av_app::startup", "{why}");
            eprintln!("{why}");
            std::process::exit(1);
        }
    };

    let audio = Arc::new(AudioService::default());
    let (store_for_setup, audio_for_setup) = (store.clone(), audio.clone());

    let app = tauri::Builder::default()
        .runtime(tauri_runtime_wry::Wry::default())
        .manage(store.clone())
        .manage(audio.clone())
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::get_config,
            commands::patch_config,
            commands::start_source,
            commands::stop_source,
            commands::get_audio_status,
            commands::get_frame,
            commands::list_scenes,
            commands::get_active_scene
        ])
        .setup(move |app| {
            // 7. Let the panic hook reach the app.
            enter(Step::RegisterApp);
            panic::register_app(app.handle().clone());

            // 8. Studio window (tray arrives with the v0.1 shell work).
            enter(Step::StudioAndTray);
            create_studio(app, !cli.hidden)?;

            // 9-10. Visualizer hosts and the capture/analysis/render pipeline: Phase 1B.
            enter(Step::HostsHidden);
            enter(Step::Pipeline);
            if cli.demo {
                let analysis = store_for_setup.get().analysis;
                if let Err(e) = audio_for_setup.start(SourceRequest::Demo, &analysis) {
                    tracing::warn!(target: "av_app::startup", "demo source failed: {e}");
                }
            }
            tracing::info!(target: "av_app::startup", "startup complete");
            Ok(())
        })
        .build(context);

    let app = match app {
        Ok(app) => app,
        Err(err) => {
            tracing::error!(target: "av_app::startup", "tauri failed to start: {err}");
            store.flush();
            std::process::exit(1);
        }
    };
    app.run(move |_handle, event| {
        if let tauri::RunEvent::Exit = event {
            // Shutdown order (blueprint 4.3): stop capture and analysis, then flush config.
            audio.stop();
            store.flush();
            tracing::info!(target: "av_app::startup", "exit");
        }
    });
}

fn create_studio(app: &mut tauri::App, visible: bool) -> tauri::Result<()> {
    assert!(
        panic::is_installed(),
        "the panic hook must exist before any window"
    );
    assert!(reached(Step::Preflight), "no webview before the preflight");
    if app.get_webview_window(STUDIO_LABEL).is_some() {
        return Ok(());
    }
    WebviewWindowBuilder::new(app, STUDIO_LABEL, WebviewUrl::App("index.html".into()))
        .title(identity::STUDIO_TITLE)
        .inner_size(1100.0, 720.0)
        .min_inner_size(820.0, 520.0)
        .visible(visible)
        .build()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn position(step: Step) -> usize {
        ORDER.iter().position(|s| *s == step).unwrap()
    }

    #[test]
    fn steps_are_numbered_in_order() {
        for (i, step) in ORDER.iter().enumerate() {
            assert_eq!(*step as usize, i + 1);
        }
    }

    #[test]
    fn the_panic_hook_precedes_every_window() {
        assert_eq!(ORDER[0], Step::PanicHook);
        assert!(position(Step::PanicHook) < position(Step::StudioAndTray));
        assert!(position(Step::PanicHook) < position(Step::HostsHidden));
    }

    #[test]
    fn preflight_precedes_the_studio_and_config_precedes_hosts() {
        assert!(position(Step::Preflight) < position(Step::StudioAndTray));
        assert!(position(Step::Config) < position(Step::HostsHidden));
        assert!(position(Step::HostsHidden) < position(Step::Pipeline));
    }
}
