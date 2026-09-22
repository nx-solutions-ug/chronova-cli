use dirs::home_dir;
use std::fs::OpenOptions;
use std::io;
use std::path::PathBuf;
use tracing::Level;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{
    fmt::{self, format::Writer, time::FormatTime},
    prelude::*,
    EnvFilter,
};

pub fn setup_logging(verbose: bool) -> Result<WorkerGuard, io::Error> {
    setup_logging_with_output_format(verbose, false)
}

pub fn setup_logging_with_output_format(
    verbose: bool,
    json_output: bool,
) -> Result<WorkerGuard, io::Error> {
    setup_logging_with_options(verbose, json_output, None, false)
}

/// `json_output` forces file-only logging, unconditionally, for any path whose
/// stdout is machine-parsed (`--output json`/`raw-json`, and `--sync-ai-activity`
/// via the hardcoded `true` its `main.rs` call site passes). `log_to_stdout`
/// (`--log-to-stdout`) is deliberately **not** able to override that: a caller
/// mixing `--log-to-stdout` into a machine-readable invocation must not get log
/// lines interleaved into the document it's trying to parse — same failure
/// class as `--sync-ai-activity`'s plugin, just a different victim. Every other
/// (human-facing) path already includes the stdout layer unconditionally,
/// flag or no flag (see AGENTS.md's logging Landmine — out of scope here), so
/// `log_to_stdout` has no code path where it currently changes the outcome; it
/// stays a real, threaded-through parameter rather than being silently dropped,
/// so a future machine-readable output has an unambiguous switch to opt into
/// the same protection. `log_file` overrides the default `~/.chronova.log`
/// destination (`--log-file`) and applies everywhere, unconditionally.
pub fn setup_logging_with_options(
    verbose: bool,
    json_output: bool,
    log_file: Option<&str>,
    log_to_stdout: bool,
) -> Result<WorkerGuard, io::Error> {
    // Not consulted below — see the doc comment above for why machine-readable
    // output must win unconditionally regardless of this flag's value.
    let _ = log_to_stdout;

    let log_file = resolve_log_file_path(log_file)?;

    // Create log file directory if it doesn't exist
    if let Some(parent) = log_file.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let file_appender = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file)?;

    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // Set log level based on verbose flag
    let log_level = if verbose { Level::DEBUG } else { Level::INFO };

    let env_filter = EnvFilter::new(format!(
        "chronova_cli={},warn",
        log_level.as_str().to_lowercase()
    ));

    let file_layer = fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_timer(ChronoLocalTimer)
        .with_filter(env_filter.clone());

    if json_output {
        // Only set up file logging and avoid any stdout contamination.
        let registry = tracing_subscriber::registry().with(file_layer);
        let _ = tracing::subscriber::set_global_default(registry);
    } else {
        let stdout_layer = fmt::layer()
            .with_writer(io::stdout)
            .with_ansi(true)
            .with_timer(ChronoLocalTimer)
            .with_filter(env_filter);

        let registry = tracing_subscriber::registry()
            .with(file_layer)
            .with(stdout_layer);

        let _ = tracing::subscriber::set_global_default(registry);
    }

    Ok(guard)
}

fn resolve_log_file_path(log_file: Option<&str>) -> Result<PathBuf, io::Error> {
    match log_file {
        Some(path) => Ok(PathBuf::from(path)),
        None => get_log_file_path(),
    }
}

fn get_log_file_path() -> Result<PathBuf, io::Error> {
    let mut path = home_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Could not find home directory"))?;

    path.push(".chronova.log");
    Ok(path)
}

struct ChronoLocalTimer;

impl FormatTime for ChronoLocalTimer {
    fn format_time(&self, w: &mut Writer<'_>) -> std::fmt::Result {
        let now = chrono::Local::now();
        write!(w, "{}", now.format("%Y-%m-%d %H:%M:%S%.3f"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_file_path() {
        let path = get_log_file_path().unwrap();
        assert!(path.to_string_lossy().ends_with(".chronova.log"));
    }

    #[test]
    fn resolve_log_file_path_honors_override() {
        let path = resolve_log_file_path(Some("/tmp/custom-chronova.log")).unwrap();
        assert_eq!(path, PathBuf::from("/tmp/custom-chronova.log"));
    }

    #[test]
    fn resolve_log_file_path_defaults_to_chronova_log() {
        let path = resolve_log_file_path(None).unwrap();
        assert!(path.to_string_lossy().ends_with(".chronova.log"));
    }

    #[test]
    fn test_setup_logging() {
        // This test just ensures the function doesn't panic
        // We can't easily test the actual logging behavior without complex setup
        let _guard = setup_logging(false).unwrap();

        // Log a test message
        tracing::info!("Test log message");
    }
}
