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

/// End-to-end proof, through the real binary, that `--sync-offline-activity`
/// prunes the queue at a *configured* `sync_retention_days` rather than a
/// hardcoded value — the shape the fix actually runs in production
/// (`process_queue`'s retention step opens its own transient `Queue::new()`
/// inside `spawn_blocking`; this is the only test that exercises that exact
/// path rather than calling the retention function directly). Seeds two
/// entries, 10 and 40 days old, with `sync_retention_days = 30`: the 40-day
/// one must be pruned, the 10-day one must survive. The `--api-url` is
/// unroutable, so the surviving entry ends up `PermanentFailure` rather than
/// `Pending` after its send attempts are refused — `count()` is used rather
/// than `get_pending()` (which filters to `Pending` by default) so that
/// doesn't register as "removed."
#[cfg(unix)]
#[test]
fn test_sync_offline_activity_prunes_at_configured_retention_days() {
    let home = tempfile::tempdir().unwrap();
    let chronova_dir = home.path().join(".chronova");
    fs::create_dir_all(&chronova_dir).unwrap();
    let db_path = chronova_dir.join("queue.db");

    let queue = Queue::with_path(db_path.clone()).expect("failed to seed test queue");
    let within_window = Heartbeat {
        id: "ten-days-old".to_string(),
        entity: "/tmp/within-window.rs".to_string(),
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
    };
    let beyond_window = Heartbeat {
        id: "forty-days-old".to_string(),
        ..within_window.clone()
    };
    queue
        .add(within_window.clone())
        .expect("failed to seed 10-day-old heartbeat");
    queue
        .add(beyond_window.clone())
        .expect("failed to seed 40-day-old heartbeat");
    drop(queue);

    // Backdate directly via SQL: `backdate_for_test` is `pub(crate)` and
    // `#[cfg(test)]` inside the lib crate, not visible to this integration
    // test binary.
    let conn = rusqlite::Connection::open(&db_path).expect("failed to open seeded db");
    conn.execute(
        "UPDATE heartbeats SET created_at = datetime('now', '-10 days') WHERE id = ?1",
        rusqlite::params![within_window.id],
    )
    .expect("failed to backdate 10-day-old heartbeat");
    conn.execute(
        "UPDATE heartbeats SET created_at = datetime('now', '-40 days') WHERE id = ?1",
        rusqlite::params![beyond_window.id],
    )
    .expect("failed to backdate 40-day-old heartbeat");
    drop(conn);

    let config_content = "\n[settings]\nsync_retention_days = 30\n";
    let config_file = tempfile::NamedTempFile::new().unwrap();
    fs::write(&config_file, config_content).unwrap();

    let mut cmd = Command::cargo_bin("chronova-cli").unwrap();
    cmd.env("HOME", home.path())
        .arg("--config")
        .arg(config_file.path())
        .arg("--api-url")
        .arg("http://127.0.0.1:1")
        .arg("--sync-offline-activity")
        .arg("10")
        .assert()
        .success();

    let reopened = Queue::with_path(db_path).expect("failed to reopen seeded db");
    let count = reopened.count().expect("count should return Ok");
    assert_eq!(
        count, 1,
        "only the entry within the configured 30-day retention window should remain"
    );
}
