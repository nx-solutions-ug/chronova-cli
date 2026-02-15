use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

#[test]
fn test_offline_count_command() {
    let mut cmd = Command::cargo_bin("chronova-cli").unwrap();
    
    cmd.arg("--offline-count")
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

#[test]
fn test_sync_offline_activity_command() {
    let mut cmd = Command::cargo_bin("chronova-cli").unwrap();
    
    cmd.arg("--sync-offline-activity")
       .arg("10")
       .assert()
       .success()
       .stdout(predicate::str::contains("Syncing offline heartbeats..."))
       .stdout(predicate::str::contains("Sync completed:"))
       .stdout(predicate::str::contains("Heartbeats synced:"))
       .stdout(predicate::str::contains("Heartbeats failed:"));
}

#[test]
fn test_force_sync_option() {
    let mut cmd = Command::cargo_bin("chronova-cli").unwrap();
    
    cmd.arg("--sync-offline-activity")
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
    
    let mut cmd = Command::cargo_bin("chronova-cli").unwrap();
    
    cmd.arg("--config")
       .arg(config_file.path())
       .arg("--offline-count")
       .assert()
       .success()
       .stdout(predicate::str::contains("Offline heartbeats queue status:"));
}

#[test]
fn test_offline_commands_with_verbose_logging() {
    let mut cmd = Command::cargo_bin("chronova-cli").unwrap();
    
    cmd.arg("--offline-count")
       .arg("--verbose")
       .assert()
       .success()
       .stdout(predicate::str::contains("Offline heartbeats queue status:"));
}