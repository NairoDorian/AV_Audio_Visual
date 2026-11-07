//! Session logging (startup step 3).
//!
//! One session log with daily rotation, plus the console. `RUST_LOG` filters the console only;
//! the durable file always captures everything from our crates in development (AGENTS.md
//! section 5). Never log per audio callback or per frame.

use std::path::Path;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

/// Keeps the non-blocking file writer alive; dropping it flushes the log.
#[must_use = "dropping the guard stops and flushes the session log"]
#[derive(Debug)]
pub struct LogGuard {
    _file: Option<WorkerGuard>,
}

/// File capture: every severity from our crates (`av_*`) in debug, Info in release.
const FILE_FILTER_DEBUG: &str = "info,av_=trace";
const FILE_FILTER_RELEASE: &str = "info";
/// Console default when `RUST_LOG` is unset.
const CONSOLE_DEFAULT_DEBUG: &str = "info,av_=debug";
const CONSOLE_DEFAULT_RELEASE: &str = "info";

pub fn init(log_dir: &Path) -> LogGuard {
    let debug = cfg!(debug_assertions);
    let console_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(if debug {
            CONSOLE_DEFAULT_DEBUG
        } else {
            CONSOLE_DEFAULT_RELEASE
        })
    });
    let console = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_filter(console_filter);

    let (file_layer, guard) = match open_file(log_dir) {
        Ok(appender) => {
            let (writer, guard) = tracing_appender::non_blocking(appender);
            let filter = EnvFilter::new(if debug {
                FILE_FILTER_DEBUG
            } else {
                FILE_FILTER_RELEASE
            });
            let layer = tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_thread_names(true)
                .with_writer(writer)
                .with_filter(filter);
            (Some(layer), Some(guard))
        }
        Err(err) => {
            eprintln!(
                "session log disabled: cannot open {}: {err}",
                log_dir.display()
            );
            (None, None)
        }
    };

    // `try_init` so a second call (tests) does not panic.
    let _ = tracing_subscriber::registry()
        .with(console)
        .with(file_layer)
        .try_init();
    LogGuard { _file: guard }
}

fn open_file(log_dir: &Path) -> Result<RollingFileAppender, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(log_dir)?;
    Ok(RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .max_log_files(7)
        .filename_prefix("session")
        .filename_suffix("log")
        .build(log_dir)?)
}
