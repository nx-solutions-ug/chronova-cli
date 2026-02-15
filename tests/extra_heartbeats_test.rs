// Test for extra heartbeats processing functionality
use assert_cmd::Command;
use tempfile::NamedTempFile;

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
