use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::NamedTempFile;

#[test]
fn test_wakatime_help_compatibility() {
    let mut cmd = Command::cargo_bin("chronova-cli").unwrap();
    cmd.arg("--help");
    cmd.assert().success().stdout(predicate::str::contains(
        "A high-performance, drop-in replacement for wakatime-cli",
    ));
}

#[test]
fn test_wakatime_version_compatibility() {
    let mut cmd = Command::cargo_bin("chronova-cli").unwrap();
    cmd.arg("--version");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("chronova-cli"));
}

#[test]
fn test_wakatime_entity_argument() {
    let mut cmd = Command::cargo_bin("chronova-cli").unwrap();
    cmd.arg("--entity").arg("/tmp/test.rs").arg("--verbose");
    // With offline heartbeats support, this should succeed and queue the heartbeat
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Heartbeat added to queue"));
}

#[test]
fn test_wakatime_config_file_parsing() {
    // Create a temporary config file
    let config_file = NamedTempFile::new().unwrap();
    let config_content = r#"
[settings]
api_key = test_key_123
api_url = https://test.example.com/api
debug = true
hide_file_names = true
exclude =
    *.tmp
    *.log
"#;
    fs::write(config_file.path(), config_content).unwrap();

    let mut cmd = Command::cargo_bin("chronova-cli").unwrap();
    cmd.arg("--config")
        .arg(config_file.path())
        .arg("--entity")
        .arg("/tmp/test.rs")
        .arg("--verbose");

    // With offline heartbeats support, this should succeed and queue the heartbeat
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Heartbeat added to queue"));
}

#[test]
fn test_wakatime_plugin_argument() {
    let mut cmd = Command::cargo_bin("chronova-cli").unwrap();
    cmd.arg("--entity")
        .arg("/tmp/test.rs")
        .arg("--plugin")
        .arg("vscode/1.88.0 vscode-wakatime/24.0.0")
        .arg("--verbose");

    // With offline heartbeats support, this should succeed and queue the heartbeat
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Heartbeat added to queue"));
}

#[tokio::test]
async fn test_wakatime_today_flag() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // Start mock server and mount a simple statusbar/today endpoint
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/users/current/statusbar/today"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"text":"4 mins","has_team_features":false}"#),
        )
        .mount(&mock_server)
        .await;

    // Create a temporary config that points api_url to the mock server and includes an api_key
    let config_file = NamedTempFile::new().unwrap();
    let config_content = format!(
        r#"[settings]
api_key = test_key_123
api_url = {}
"#,
        mock_server.uri()
    );
    fs::write(config_file.path(), config_content).unwrap();

    let mut cmd = Command::cargo_bin("chronova-cli").unwrap();
    cmd.arg("--today")
        .arg("--verbose")
        .arg("--config")
        .arg(config_file.path());

    // This should succeed and show a concise time string (e.g., "4 mins", "1 hour 2 mins")
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("min").or(predicate::str::contains("hour")));
}

#[tokio::test]
async fn test_key_flag_overrides_config_file_key_on_the_wire() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // Start a mock server and accept any heartbeat post; we inspect the
    // captured request's Authorization header afterwards rather than
    // constraining the mock itself, so a mismatch fails the assertion
    // below instead of silently falling back to Basic auth.
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/users/current/heartbeats"))
        .respond_with(ResponseTemplate::new(201).set_body_string(r#"{"data":{}}"#))
        .mount(&mock_server)
        .await;

    // Throwaway $HOME so this run never touches a real queue/log/config
    // (see AGENTS.md "State on Disk" / Testing).
    let home_dir = tempfile::tempdir().unwrap();
    let config_path = home_dir.path().join(".chronova.cfg");
    let config_content = format!(
        "[settings]\napi_key = file_key\napi_url = {}\n",
        mock_server.uri()
    );
    fs::write(&config_path, config_content).unwrap();

    let entity_file = NamedTempFile::new().unwrap();

    let mut cmd = Command::cargo_bin("chronova-cli").unwrap();
    cmd.env("HOME", home_dir.path())
        .arg("--key")
        .arg("cli_key")
        .arg("--entity")
        .arg(entity_file.path())
        .arg("--config")
        .arg(&config_path)
        .arg("--verbose");

    cmd.assert().success();

    let requests = mock_server.received_requests().await.unwrap();
    let heartbeat_request = requests
        .iter()
        .find(|r| r.url.path() == "/users/current/heartbeats")
        .expect("expected a heartbeat request to reach the mock server");

    let auth_header = heartbeat_request
        .headers
        .get("Authorization")
        .expect("expected an Authorization header")
        .to_str()
        .unwrap();

    // Proves resolution (--key wired), precedence (over the config file's
    // file_key) and delivery (the header the wire actually saw) in one shot.
    assert_eq!(auth_header, "Bearer cli_key");
}

#[test]
fn test_wakatime_write_flag() {
    let mut cmd = Command::cargo_bin("chronova-cli").unwrap();
    cmd.arg("--entity")
        .arg("/tmp/test.rs")
        .arg("--write")
        .arg("--verbose");

    // With offline heartbeats support, this should succeed and queue the heartbeat
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Heartbeat added to queue"));
}
