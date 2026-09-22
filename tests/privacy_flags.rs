//! End-to-end cover for the filtering and redaction flags.
//!
//! These run the real binary under a throwaway `$HOME`, because the defect
//! being fixed was in the wiring between the CLI flags and the config: a unit
//! test on the sanitizer alone would have passed while the flags still did
//! nothing.

use assert_cmd::Command;
use chronova_cli::queue::{Queue, QueueOps};
use std::fs;
use std::path::Path;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// An address that refuses instantly, so a heartbeat stays in the queue
/// instead of being sent and removed.
const UNREACHABLE_API: &str = "http://127.0.0.1:1/api/v1";

struct Run {
    home: TempDir,
    entity: String,
}

impl Run {
    /// A throwaway `$HOME` holding a config file and one source file to
    /// report activity on.
    fn new(settings: &str, entity_name: &str) -> Self {
        let home = tempfile::tempdir().expect("temp home");
        fs::write(
            home.path().join(".chronova.cfg"),
            format!("[settings]\napi_key = test_key_123\n{}\n", settings),
        )
        .expect("write config");

        let entity = home.path().join(entity_name);
        fs::write(&entity, "fn main() {}").expect("write entity");

        Self {
            home,
            entity: entity.to_string_lossy().into_owned(),
        }
    }

    fn invoke(&self, extra_args: &[&str]) {
        let mut cmd = Command::cargo_bin("chronova-cli").expect("binary built");
        cmd.env("HOME", self.home.path())
            .arg("--config")
            .arg(self.home.path().join(".chronova.cfg"))
            .arg("--entity")
            .arg(&self.entity);
        for arg in extra_args {
            cmd.arg(arg);
        }
        cmd.assert().success();
    }

    /// How many heartbeats the run left behind in the offline queue.
    ///
    /// Counts every row rather than the pending ones: a send to
    /// [`UNREACHABLE_API`] fails, so a heartbeat that was queued is still
    /// there, marked failed.
    fn log(&self) -> String {
        fs::read_to_string(self.home.path().join(".chronova.log")).unwrap_or_default()
    }

    fn queued_count(&self) -> usize {
        let db = self.home.path().join(".chronova").join("queue.db");
        if !Path::new(&db).exists() {
            return 0;
        }
        Queue::with_path(db)
            .expect("open queue")
            .count()
            .expect("count queue")
    }
}

/// Run the binary against a mock API and return the heartbeat it posted.
async fn posted_heartbeat(
    settings: &str,
    extra_args: &[&str],
    entity_name: &str,
) -> (Run, serde_json::Value) {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/users/current/heartbeats"))
        .respond_with(ResponseTemplate::new(201).set_body_string("{}"))
        .mount(&mock_server)
        .await;

    let run = Run::new(
        &format!("api_url = {}\n{}", mock_server.uri(), settings),
        entity_name,
    );
    run.invoke(extra_args);

    let requests = mock_server
        .received_requests()
        .await
        .expect("requests recorded");
    assert_eq!(requests.len(), 1, "the heartbeat is still sent");

    let body: serde_json::Value =
        serde_json::from_slice(&requests[0].body).expect("heartbeat json");
    let heartbeat = if body.is_array() {
        body[0].clone()
    } else {
        body
    };
    (run, heartbeat)
}

#[tokio::test]
async fn cli_hide_file_names_overrides_a_config_file_that_disables_it() {
    let (_run, heartbeat) = posted_heartbeat(
        "hide_file_names = false",
        &["--hide-file-names", "true"],
        "secret.rs",
    )
    .await;

    assert_eq!(
        heartbeat["entity"].as_str(),
        Some("HIDDEN.rs"),
        "the CLI flag must win over `hide_file_names = false` in the config file"
    );
}

#[tokio::test]
async fn hide_project_names_replaces_the_project() {
    let (_run, heartbeat) =
        posted_heartbeat("", &["--hide-project-names", "true"], "secret.rs").await;

    assert_eq!(heartbeat["project"].as_str(), Some("HIDDEN"));
    assert_eq!(
        heartbeat["entity"]
            .as_str()
            .map(|e| e.ends_with("secret.rs")),
        Some(true),
        "only the project is redacted by this flag"
    );
}

#[tokio::test]
async fn hide_branch_names_replaces_the_branch() {
    let (_run, heartbeat) = posted_heartbeat(
        "",
        &["--branch", "feature/secret", "--hide-branch-names", "true"],
        "secret.rs",
    )
    .await;

    assert_eq!(heartbeat["branch"].as_str(), Some("HIDDEN"));
}

#[test]
fn an_excluded_entity_is_not_queued() {
    let run = Run::new(&format!("api_url = {}", UNREACHABLE_API), "secret.rs");
    run.invoke(&["--exclude", "secret"]);

    assert_eq!(
        run.queued_count(),
        0,
        "a filtered-out heartbeat must not wait in the offline queue"
    );
}

#[test]
fn an_unfiltered_entity_is_queued() {
    let run = Run::new(&format!("api_url = {}", UNREACHABLE_API), "secret.rs");
    run.invoke(&[]);

    assert_eq!(
        run.queued_count(),
        1,
        "the control run does queue its heartbeat"
    );
}

#[test]
fn an_entity_missing_from_the_include_list_is_not_queued() {
    let run = Run::new(&format!("api_url = {}", UNREACHABLE_API), "secret.rs");
    run.invoke(&["--include", "/nowhere/"]);

    assert_eq!(run.queued_count(), 0);
}

#[test]
fn include_wins_when_both_lists_match() {
    let run = Run::new(&format!("api_url = {}", UNREACHABLE_API), "secret.rs");
    run.invoke(&["--exclude", "secret", "--include", "secret"]);

    assert_eq!(
        run.queued_count(),
        1,
        "include takes precedence over exclude when both match"
    );
}

#[test]
fn an_invalid_pattern_does_not_abort_the_run() {
    let run = Run::new(&format!("api_url = {}", UNREACHABLE_API), "secret.rs");
    // `*.rs` is a glob, not a regex, and cannot compile; the valid pattern
    // beside it still has to exclude the entity.
    run.invoke(&["--exclude", "*.rs", "--exclude", "secret"]);

    assert!(
        run.log().contains("ignoring invalid exclude pattern"),
        "the skipped pattern must be reported, not swallowed: {}",
        run.log()
    );
    assert_eq!(
        run.queued_count(),
        0,
        "the valid pattern beside the invalid one still applies"
    );
}

#[test]
fn hide_project_folder_warns_when_there_is_nothing_to_strip() {
    let run = Run::new(&format!("api_url = {}", UNREACHABLE_API), "secret.rs");
    run.invoke(&["--hide-project-folder"]);

    assert!(
        run.log().contains("hide_project_folder: no project root"),
        "a privacy flag that could not act must say so: {}",
        run.log()
    );
}

/// A git worktree resolves its *project* to the main repository, which is not
/// an ancestor of the file being edited. The folder to strip is the worktree's
/// own root, or the flag silently sends the whole path.
#[tokio::test]
async fn hide_project_folder_strips_the_worktree_root_not_the_main_repo() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/users/current/heartbeats"))
        .respond_with(ResponseTemplate::new(201).set_body_string("{}"))
        .mount(&mock_server)
        .await;

    let repos = tempfile::tempdir().expect("temp dir");
    let main_repo = repos.path().join("main");
    fs::create_dir(&main_repo).expect("main repo dir");
    let repo = git2::Repository::init(&main_repo).expect("init repo");
    fs::write(main_repo.join("README.md"), "# main\n").expect("write readme");
    let mut index = repo.index().expect("index");
    index.add_path(Path::new("README.md")).expect("add readme");
    let tree_oid = index.write_tree().expect("write tree");
    drop(index);
    let tree = repo.find_tree(tree_oid).expect("tree");
    let sig = git2::Signature::now("Test", "test@example.com").expect("signature");
    repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
        .expect("commit");
    drop(tree);

    let worktree_path = repos.path().join("feature");
    repo.worktree("feature", &worktree_path, None)
        .expect("create worktree");
    fs::create_dir_all(worktree_path.join("src")).expect("src dir");
    let entity = worktree_path.join("src").join("main.rs");
    fs::write(&entity, "fn main() {}").expect("write entity");

    let run = Run::new(&format!("api_url = {}", mock_server.uri()), "unused.rs");
    let mut cmd = Command::cargo_bin("chronova-cli").expect("binary built");
    cmd.env("HOME", run.home.path())
        .arg("--config")
        .arg(run.home.path().join(".chronova.cfg"))
        .arg("--hide-project-folder")
        .arg("--entity")
        .arg(&entity);
    cmd.assert().success();

    let requests = mock_server
        .received_requests()
        .await
        .expect("requests recorded");
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value =
        serde_json::from_slice(&requests[0].body).expect("heartbeat json");
    let heartbeat = if body.is_array() {
        body[0].clone()
    } else {
        body
    };

    assert_eq!(heartbeat["entity"].as_str(), Some("src/main.rs"));
}

#[tokio::test]
async fn hide_project_folder_makes_the_entity_relative() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/users/current/heartbeats"))
        .respond_with(ResponseTemplate::new(201).set_body_string("{}"))
        .mount(&mock_server)
        .await;

    let run = Run::new(&format!("api_url = {}", mock_server.uri()), "secret.rs");
    // A project marker in the throwaway home makes it the detected root.
    fs::write(run.home.path().join("Cargo.toml"), "[package]").expect("project marker");
    run.invoke(&["--hide-project-folder"]);

    let requests = mock_server
        .received_requests()
        .await
        .expect("requests recorded");
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value =
        serde_json::from_slice(&requests[0].body).expect("heartbeat json");
    let heartbeat = if body.is_array() {
        body[0].clone()
    } else {
        body
    };

    assert_eq!(heartbeat["entity"].as_str(), Some("secret.rs"));
}
