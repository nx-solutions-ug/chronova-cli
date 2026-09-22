// Integration tests for Task 6 (contract-gap-fixes): --log-file, --log-to-stdout,
// --disable-offline (6a/6b) and the [not yet implemented] --help markers (6d).
use assert_cmd::Command;
use chronova_cli::queue::{Queue, QueueOps};

// An address that refuses connections immediately, matching the convention
// already used in src/api.rs's own unit tests for forcing a network error.
const UNROUTABLE_API_URL: &str = "http://127.0.0.1:9";

#[test]
fn log_file_flag_writes_to_the_given_path_not_the_default() {
    let home = tempfile::tempdir().unwrap();
    let log_path = home.path().join("custom").join("chronova.log");

    let mut cmd = Command::cargo_bin("chronova-cli").unwrap();
    cmd.env("HOME", home.path())
        .arg("--offline-count")
        .arg("--log-file")
        .arg(&log_path)
        .assert()
        .success();

    assert!(
        log_path.exists(),
        "expected a log file at the --log-file path"
    );
    assert!(
        !home.path().join(".chronova.log").exists(),
        "the default ~/.chronova.log must not be written when --log-file overrides it"
    );
}

#[test]
fn disable_offline_drops_the_heartbeat_after_a_failed_send() {
    let home = tempfile::tempdir().unwrap();
    let entity = home.path().join("main.rs");
    std::fs::write(&entity, "// test\n").unwrap();

    let mut cmd = Command::cargo_bin("chronova-cli").unwrap();
    cmd.env("HOME", home.path())
        .arg("--entity")
        .arg(&entity)
        .arg("--disable-offline")
        .arg("--api-url")
        .arg(UNROUTABLE_API_URL)
        .assert()
        .success();

    let queue = Queue::with_path(home.path().join(".chronova").join("queue.db"))
        .expect("failed to open the queue db the run created");
    let stats = queue.get_sync_stats().expect("failed to read queue stats");
    assert_eq!(
        stats.total, 0,
        "--disable-offline must drop the heartbeat instead of queueing it on a failed send"
    );
}

#[test]
fn without_disable_offline_a_failed_send_stays_queued() {
    // Contrast case for the test above: proves the assertion has teeth by
    // showing the queue is NOT empty when --disable-offline is absent.
    let home = tempfile::tempdir().unwrap();
    let entity = home.path().join("main.rs");
    std::fs::write(&entity, "// test\n").unwrap();

    let mut cmd = Command::cargo_bin("chronova-cli").unwrap();
    cmd.env("HOME", home.path())
        .arg("--entity")
        .arg(&entity)
        .arg("--api-url")
        .arg(UNROUTABLE_API_URL)
        .assert()
        .success();

    let queue = Queue::with_path(home.path().join(".chronova").join("queue.db"))
        .expect("failed to open the queue db the run created");
    let stats = queue.get_sync_stats().expect("failed to read queue stats");
    assert_eq!(
        stats.total, 1,
        "without --disable-offline the heartbeat should remain queued after a failed send"
    );
}

#[test]
fn sync_ai_activity_stays_byte_silent_on_success_even_with_log_to_stdout() {
    // The invoking plugin logs any stdout/stderr this process produces as an
    // error (AGENTS.md, "Claude Code activity tracking"), so this path must
    // stay silent no matter what --log-to-stdout asks for.
    let home = tempfile::tempdir().unwrap();

    let mut cmd = Command::cargo_bin("chronova-cli").unwrap();
    let assert = cmd
        .env("HOME", home.path())
        .arg("--sync-ai-activity")
        .arg("--plugin")
        .arg("claude-code/1.0.0 claude-code-wakatime/4.1.0")
        .arg("--project-folder")
        .arg(home.path())
        .arg("--log-to-stdout")
        .assert()
        .success();

    let output = assert.get_output();
    assert!(
        output.stdout.is_empty(),
        "stdout must stay empty, got: {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        output.stderr.is_empty(),
        "stderr must stay empty, got: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Prove logging still happened (to the file), rather than having been
    // silently disabled altogether.
    let log_contents =
        std::fs::read_to_string(home.path().join(".chronova.log")).expect("log file must exist");
    assert!(
        log_contents.contains("ai activity sync produced"),
        "expected the ai sync summary line in the log file, got: {log_contents}"
    );
}

#[tokio::test]
async fn log_to_stdout_cannot_break_output_json_parsing() {
    // Corrected reading of the brief (coordinator sign-off): any path whose
    // stdout is machine-parsed must stay clean, not just --sync-ai-activity.
    // --output json is exactly that kind of path (main.rs prints a single
    // JSON document with `print!`), so --log-to-stdout must be silently
    // ignored here too — a caller who wants both logs and JSON uses
    // --log-file instead. If stdout is anything other than that one JSON
    // document, serde_json::from_str below fails.
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/users/current/statusbar/today"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"text":"4 mins","has_team_features":false}"#),
        )
        .mount(&mock_server)
        .await;

    let home = tempfile::tempdir().unwrap();
    let config_path = home.path().join("chronova.cfg");
    std::fs::write(
        &config_path,
        format!(
            "[settings]\napi_key = test_key_123\napi_url = {}\n",
            mock_server.uri()
        ),
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("chronova-cli").unwrap();
    let assert = cmd
        .env("HOME", home.path())
        .arg("--today")
        .arg("--verbose")
        .arg("--config")
        .arg(&config_path)
        .arg("--output")
        .arg("json")
        .arg("--log-to-stdout")
        .assert()
        .success();

    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!(
            "stdout must stay valid, parseable JSON even with --log-to-stdout: {e}, got {stdout:?}"
        )
    });
    assert_eq!(parsed["text"], "4 mins");
}

#[test]
fn log_to_stdout_stays_silent_on_the_json_forced_heartbeat_path() {
    // `--today`'s success path (tested above) happens to emit no tracing
    // calls at all, so that test alone can't prove contamination would be
    // caught. The plain heartbeat path always logs via
    // `tracing::info!("Heartbeat added to queue")` (queue.rs), which makes a
    // reliable, independently-verified regression check: if --log-to-stdout
    // ever regains the ability to override json_output, this line leaks onto
    // stdout and the assertion below fails.
    let home = tempfile::tempdir().unwrap();
    let entity = home.path().join("main.rs");
    std::fs::write(&entity, "// test\n").unwrap();

    let mut cmd = Command::cargo_bin("chronova-cli").unwrap();
    let assert = cmd
        .env("HOME", home.path())
        .arg("--entity")
        .arg(&entity)
        .arg("--api-url")
        .arg(UNROUTABLE_API_URL)
        .arg("--output")
        .arg("json")
        .arg("--log-to-stdout")
        .assert()
        .success();

    let output = assert.get_output();
    assert!(
        output.stdout.is_empty(),
        "stdout must stay empty on a json_output-forced path regardless of --log-to-stdout, got: {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn help_marks_exactly_the_thirteen_unimplemented_flags() {
    let mut cmd = Command::cargo_bin("chronova-cli").unwrap();
    let assert = cmd.arg("--help").assert().success();
    let output = assert.get_output();
    let help_text = String::from_utf8_lossy(&output.stdout);

    // clap renders each flag as a "--flag <VALUE>" header line followed by an
    // indented description line, so a flag's own line rarely contains its
    // description. Group lines into per-flag blocks instead of matching a
    // single line — otherwise this test would pass regardless of what got
    // marked, since the marker never lands on the header line.
    let mut blocks: Vec<(&str, String)> = Vec::new();
    for line in help_text.lines() {
        let trimmed = line.trim_start();
        if let Some(name) = trimmed
            .strip_prefix("--")
            .and_then(|_| trimmed.split_whitespace().next())
        {
            blocks.push((name, String::new()));
        } else if let Some((_, desc)) = blocks.last_mut() {
            desc.push(' ');
            desc.push_str(trimmed);
        }
    }

    let marked: Vec<&str> = blocks
        .iter()
        .filter(|(_, desc)| desc.contains("[not yet implemented]"))
        .map(|(name, _)| *name)
        .collect();

    let mut expected = [
        "--local-file",
        "--metrics",
        "--is-unsaved-entity",
        "--human-line-changes",
        "--ai-line-changes",
        "--print-offline-heartbeats",
        "--offline-queue-file",
        "--offline-queue-file-legacy",
        "--internal-config",
        "--include-only-with-project-file",
        "--send-diagnostics-on-errors",
        "--guess-language",
        "--file-experts",
    ];
    expected.sort_unstable();
    let mut marked_sorted = marked.clone();
    marked_sorted.sort_unstable();

    assert_eq!(
        marked_sorted, expected,
        "the set of flags marked [not yet implemented] must be exactly the brief's thirteen"
    );

    // Flags owned by sibling tasks (4 and 5) must not be marked here.
    for flag in [
        "--timeout",
        "--proxy",
        "--no-ssl-verify",
        "--ssl-certs-file",
        "--exclude",
        "--include",
        "--hide-project-names",
        "--hide-project-folder",
        "--exclude-unknown-project",
        "--hide-file-names",
        "--hide-branch-names",
    ] {
        assert!(
            blocks.iter().any(|(name, _)| *name == flag),
            "--help output should still list {flag}"
        );
        assert!(
            !marked.contains(&flag),
            "{flag} is owned by a sibling task and must not be marked here"
        );
    }
}
