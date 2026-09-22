use assert_cmd::Command;
use chronova_cli::heartbeat::Heartbeat;
use chronova_cli::queue::{Queue, QueueOps};
use predicates::prelude::*;
use std::fs;

/// Isolated under a throwaway `$HOME` (see AGENTS.md's "State on Disk" /
/// "Testing" sections) so this never opens the developer's real
/// `~/.chronova/queue.db`, and pointed at an unroutable `--api-url` so
/// nothing this test triggers can leave the machine.
#[test]
fn test_offline_count_command() {
    let home = tempfile::tempdir().unwrap();
    let mut cmd = Command::cargo_bin("chronova-cli").unwrap();

    cmd.env("HOME", home.path())
        .arg("--api-url")
        .arg("http://127.0.0.1:1")
        .arg("--offline-count")
        .assert()
        .success()
        .stdout(predicate::str::contains("Offline heartbeats queue status:"))
        .stdout(predicate::str::contains("Total:"))
        .stdout(predicate::str::contains("Pending:"))
        .stdout(predicate::str::contains("Syncing:"))
        .stdout(predicate::str::contains("Synced:"))
        .stdout(predicate::str::contains("Failed:"))
        .stdout(predicate::str::contains("Permanent failures:"));
}

/// Regression test: `--offline-count` must report a non-zero count when the
/// queue already holds a heartbeat, i.e. construction must not have wiped it
/// first. Uses its own `$HOME` so it neither reads nor mutates the real
/// `~/.chronova/queue.db`; Unix-only because `dirs::home_dir()` does not
/// honour a `$HOME` override on Windows, where this would instead hit the
/// runner's real queue.
#[cfg(unix)]
#[test]
fn test_offline_count_reports_queued_heartbeats() {
    let home = tempfile::tempdir().unwrap();
    let chronova_dir = home.path().join(".chronova");
    fs::create_dir_all(&chronova_dir).unwrap();
    let db_path = chronova_dir.join("queue.db");

    let queue = Queue::with_path(db_path).expect("failed to seed test queue");
    queue
        .add(Heartbeat {
            id: "offline-count-test".to_string(),
            entity: "/tmp/example.rs".to_string(),
            entity_type: "file".to_string(),
            time: 1.0,
            project: Some("p".to_string()),
            branch: None,
            language: Some("Rust".to_string()),
            is_write: false,
            lines: None,
            lineno: None,
            cursorpos: None,
            user_agent: Some("test/1.0".to_string()),
            category: Some("coding".to_string()),
            machine: Some("m".to_string()),
            editor: None,
            operating_system: None,
            commit_hash: None,
            commit_author: None,
            commit_message: None,
            repository_url: None,
            dependencies: Vec::new(),
            ai: Default::default(),
        })
        .expect("failed to seed queue with a heartbeat");
    drop(queue);

    let mut cmd = Command::cargo_bin("chronova-cli").unwrap();

    cmd.env("HOME", home.path())
        .arg("--api-url")
        .arg("http://127.0.0.1:1")
        .arg("--offline-count")
        .assert()
        .success()
        .stdout(predicate::str::contains("Total: 1\n"));
}

/// Isolated under a throwaway `$HOME` and an unroutable `--api-url`: unlike
/// `--offline-count`, `--sync-offline-activity` actually attempts to send
/// whatever is queued, so without isolation this would fire real requests
/// (using the developer's real `~/.chronova.cfg` API key, against the real
/// `~/.chronova/queue.db`) every time `cargo test` runs.
#[test]
fn test_sync_offline_activity_command() {
    let home = tempfile::tempdir().unwrap();
    let mut cmd = Command::cargo_bin("chronova-cli").unwrap();

    cmd.env("HOME", home.path())
        .arg("--api-url")
        .arg("http://127.0.0.1:1")
        .arg("--sync-offline-activity")
        .arg("10")
        .assert()
        .success()
        .stdout(predicate::str::contains("Syncing offline heartbeats..."))
        .stdout(predicate::str::contains("Sync completed:"))
        .stdout(predicate::str::contains("Heartbeats synced:"))
        .stdout(predicate::str::contains("Heartbeats failed:"));
}

/// Same isolation as `test_sync_offline_activity_command`, and for the same
/// reason: `--force-sync` still goes through `--sync-offline-activity`'s
/// send path.
#[test]
fn test_force_sync_option() {
    let home = tempfile::tempdir().unwrap();
    let mut cmd = Command::cargo_bin("chronova-cli").unwrap();

    cmd.env("HOME", home.path())
        .arg("--api-url")
        .arg("http://127.0.0.1:1")
        .arg("--sync-offline-activity")
        .arg("10")
        .arg("--force-sync")
        .assert()
        .success()
        .stdout(predicate::str::contains("Syncing offline heartbeats..."))
        .stdout(predicate::str::contains("Sync completed:"))
        .stdout(predicate::str::contains("Heartbeats synced:"))
        .stdout(predicate::str::contains("Heartbeats failed:"))
        .stdout(predicate::str::contains("Forced sync: true"));
}

#[test]
fn test_cli_help_includes_offline_commands() {
    let mut cmd = Command::cargo_bin("chronova-cli").unwrap();

    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--sync-offline-activity"))
        .stdout(predicate::str::contains("--offline-count"))
        .stdout(predicate::str::contains("--force-sync"));
}

#[test]
fn test_offline_commands_with_config_file() {
    // Create a temporary config file
    let config_content = r#"
[settings]
api_key = test-key-123
"#;

    let config_file = tempfile::NamedTempFile::new().unwrap();
    fs::write(&config_file, config_content).unwrap();

    let home = tempfile::tempdir().unwrap();
    let mut cmd = Command::cargo_bin("chronova-cli").unwrap();

    cmd.env("HOME", home.path())
        .arg("--config")
        .arg(config_file.path())
        .arg("--api-url")
        .arg("http://127.0.0.1:1")
        .arg("--offline-count")
        .assert()
        .success()
        .stdout(predicate::str::contains("Offline heartbeats queue status:"));
}

#[test]
fn test_offline_commands_with_verbose_logging() {
    let home = tempfile::tempdir().unwrap();
    let mut cmd = Command::cargo_bin("chronova-cli").unwrap();

    cmd.env("HOME", home.path())
        .arg("--api-url")
        .arg("http://127.0.0.1:1")
        .arg("--offline-count")
        .arg("--verbose")
        .assert()
        .success()
        .stdout(predicate::str::contains("Offline heartbeats queue status:"));
}
