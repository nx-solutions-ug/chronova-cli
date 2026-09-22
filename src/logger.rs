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

/// `json_output` suppresses the stdout layer so parsed output (e.g. `--output
/// json`) stays clean; `log_to_stdout` (`--log-to-stdout`) adds it back even
/// then. Callers that must stay byte-silent no matter what (`--sync-ai-activity`
/// at `main.rs`) pass `log_to_stdout = false` themselves rather than forwarding
/// the CLI flag. `log_file` overrides the default `~/.chronova.log` destination
/// (`--log-file`).
pub fn setup_logging_with_options(
    verbose: bool,
    json_output: bool,
    log_file: Option<&str>,
    log_to_stdout: bool,
) -> Result<WorkerGuard, io::Error> {
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

    if json_output && !log_to_stdout {
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
