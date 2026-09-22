// Test for extra heartbeats processing functionality
use assert_cmd::Command;
use chronova_cli::queue::{Queue, QueueOps};
use tempfile::{NamedTempFile, TempDir};

/// Run `chronova-cli` with the given args and stdin under an isolated `$HOME`,
/// then return every pending heartbeat left in that home's queue.db.
fn run_and_read_queue(args: &[&str], stdin: &str) -> Vec<chronova_cli::heartbeat::Heartbeat> {
    let home = TempDir::new().unwrap();

    Command::cargo_bin("chronova-cli")
        .unwrap()
        .args(args)
        .env("HOME", home.path())
        .write_stdin(stdin)
        .assert()
        .success();

    let queue = Queue::with_path(home.path().join(".chronova").join("queue.db")).unwrap();
    queue.get_pending(Some(100), None).unwrap()
}

#[test]
fn test_extra_heartbeats_with_missing_id() {
    // Create a temporary config file
    let config_content = r#"
[settings]
api_key = test-key-123
"#;

    let config_file = NamedTempFile::new().unwrap();
    std::fs::write(&config_file, config_content).unwrap();

    // Create sample JSON data that simulates external heartbeats (without id field)
    let heartbeat_data = r#"[
        {
            "entity": "/path/to/file.rs",
            "type": "file",
            "time": 1764432679.433,
            "project": "test-project",
            "branch": "main",
            "language": "Rust",
            "is_write": false,
            "lines": 100,
            "lineno": 10,
            "cursorpos": 5,
            "user_agent": "vscode/1.106.3 vscode-wakatime/25.5.0",
            "category": "coding",
            "machine": "test-machine",
            "dependencies": [],
            "editor": {
                "name": "vscode",
                "version": "1.106.3"
            },
            "operating_system": {
                "name": "linux",
                "title": "Linux",
                "version": null
            }
        }
    ]"#;

    let mut cmd = Command::cargo_bin("chronova-cli").unwrap();

    // Test that the command succeeds with external heartbeat data (missing id)
    cmd.arg("--config")
        .arg(config_file.path())
        .arg("--extra-heartbeats")
        .write_stdin(heartbeat_data)
        .assert()
        .success();
}

#[test]
fn test_extra_heartbeats_with_id() {
    // Create a temporary config file
    let config_content = r#"
[settings]
api_key = test-key-123
"#;

    let config_file = NamedTempFile::new().unwrap();
    std::fs::write(&config_file, config_content).unwrap();

    // Create sample JSON data with id field (should work with both parsers)
    let heartbeat_data = r#"[
        {
            "id": "test-id-123",
            "entity": "/path/to/file.rs",
            "type": "file",
            "time": 1764432679.433,
            "project": "test-project",
            "branch": "main",
            "language": "Rust",
            "is_write": false,
            "lines": 100,
            "lineno": 10,
            "cursorpos": 5,
            "user_agent": "vscode/1.106.3 vscode-wakatime/25.5.0",
            "category": "coding",
            "machine": "test-machine",
            "dependencies": [],
            "editor": {
                "name": "vscode",
                "version": "1.106.3"
            },
            "operating_system": {
                "name": "linux",
                "title": "Linux",
                "version": null
            }
        }
    ]"#;

    let mut cmd = Command::cargo_bin("chronova-cli").unwrap();

    // Test that the command succeeds with heartbeat data including id
    cmd.arg("--config")
        .arg(config_file.path())
        .arg("--extra-heartbeats")
        .write_stdin(heartbeat_data)
        .assert()
        .success();
}

#[test]
fn test_extra_heartbeats_invalid_json() {
    // Create a temporary config file
    let config_content = r#"
[settings]
api_key = test-key-123
"#;

    let config_file = NamedTempFile::new().unwrap();
    std::fs::write(&config_file, config_content).unwrap();

    // Create invalid JSON data
    let invalid_data = r#"{
        "entity": "/path/to/file.rs",
        "type": "file",
        "time": 1764432679.433,
        "project": "test-project"
    "#; // Missing closing brace and bracket

    let mut cmd = Command::cargo_bin("chronova-cli").unwrap();

    // Test that the command fails with invalid JSON
    cmd.arg("--config")
        .arg(config_file.path())
        .arg("--extra-heartbeats")
        .write_stdin(invalid_data)
        .assert()
        .failure();
}

#[test]
fn test_extra_heartbeats_keeps_primary_entity() {
    let stdin_batch = r#"[
        {"id": "a1", "entity": "extra-one.rs", "type": "file", "time": 1700000000.0, "is_write": false, "dependencies": []},
        {"id": "a2", "entity": "extra-two.rs", "type": "file", "time": 1700000001.0, "is_write": false, "dependencies": []}
    ]"#;

    let heartbeats = run_and_read_queue(&["--entity", "foo.rs", "--extra-heartbeats"], stdin_batch);

    assert_eq!(heartbeats.len(), 3);
    let entities: Vec<&str> = heartbeats.iter().map(|h| h.entity.as_str()).collect();
    assert!(entities.contains(&"foo.rs"));
    assert!(entities.contains(&"extra-one.rs"));
    assert!(entities.contains(&"extra-two.rs"));
}

#[test]
fn test_extra_heartbeats_without_entity_is_unchanged() {
    let stdin_batch = r#"[
        {"id": "a1", "entity": "extra-one.rs", "type": "file", "time": 1700000000.0, "is_write": false, "dependencies": []},
        {"id": "a2", "entity": "extra-two.rs", "type": "file", "time": 1700000001.0, "is_write": false, "dependencies": []}
    ]"#;

    let heartbeats = run_and_read_queue(&["--extra-heartbeats"], stdin_batch);

    assert_eq!(heartbeats.len(), 2);
    let entities: Vec<&str> = heartbeats.iter().map(|h| h.entity.as_str()).collect();
    assert!(entities.contains(&"extra-one.rs"));
    assert!(entities.contains(&"extra-two.rs"));
}

#[test]
fn test_extra_heartbeats_empty_json_array_keeps_primary() {
    let heartbeats = run_and_read_queue(&["--entity", "foo.rs", "--extra-heartbeats"], "[]");

    assert_eq!(heartbeats.len(), 1);
    assert_eq!(heartbeats[0].entity, "foo.rs");
}

#[test]
fn test_extra_heartbeats_empty_stdin_keeps_primary() {
    let heartbeats = run_and_read_queue(&["--entity", "foo.rs", "--extra-heartbeats"], "");

    assert_eq!(heartbeats.len(), 1);
    assert_eq!(heartbeats[0].entity, "foo.rs");
}

#[test]
fn test_extra_heartbeats_relaxed_parsing_without_id_or_type() {
    let stdin_batch = r#"[{"entity": "relaxed.rs", "time": 1700000002.0}]"#;

    let heartbeats = run_and_read_queue(&["--extra-heartbeats"], stdin_batch);

    assert_eq!(heartbeats.len(), 1);
    assert_eq!(heartbeats[0].entity, "relaxed.rs");
    assert_eq!(heartbeats[0].entity_type, "file");
}
