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
    let log_file = get_log_file_path()?;

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

    // Handle JSON output mode - completely disable stdout logging
    if json_output {
        // When JSON output is requested, we must ensure stdout is completely clean
        // Only set up file logging and avoid any stdout contamination
        let registry = tracing_subscriber::registry().with(file_layer);

        // Set the global default subscriber
        if tracing::subscriber::set_global_default(registry).is_err() {
            // If we can't set the global default, a subscriber is already set
            // We need to ensure it doesn't log to stdout for JSON output
            // For now, we rely on the fact that no stdout layer was added
        }
        // No logging messages should be output to stdout in JSON mode
    } else {
        // Normal mode - include both file and stdout logging
        let stdout_layer = fmt::layer()
            .with_writer(io::stdout)
            .with_ansi(true)
            .with_timer(ChronoLocalTimer)
            .with_filter(env_filter);

        let registry = tracing_subscriber::registry()
            .with(file_layer)
            .with(stdout_layer);

        // Check if a subscriber is already set to avoid "SetGlobalDefaultError"
        if tracing::subscriber::set_global_default(registry).is_err() {
            // If we can't set the global default, it means one is already set
            // Don't log initialization messages to stdout to keep output clean
        } else {
            // Don't log initialization messages to stdout to keep output clean
        }
    }

    Ok(guard)
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
    use tempfile::NamedTempFile;

    #[test]
    fn test_log_file_path() {
        let path = get_log_file_path().unwrap();
        assert!(path.to_string_lossy().ends_with(".chronova.log"));
    }

    #[test]
    fn test_setup_logging() {
        // This test just ensures the function doesn't panic
        // We can't easily test the actual logging behavior without complex setup
        let temp_file = NamedTempFile::new().unwrap();
        let _guard = setup_logging(false).unwrap();

        // Log a test message
        tracing::info!("Test log message");
    }
}
