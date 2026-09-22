//! Refused credentials must stop the flush instead of being retried per heartbeat.
//!
//! The cascade offers the key as Bearer, then Basic, then X-API-Key. If a batch
//! of 50 is refused under all three and the flush falls back to sending them one
//! at a time, each one runs that cascade again: 150 requests that cannot
//! succeed, repeated on every retry pass.
//!
//! The run happens under a throwaway `$HOME`, so every path the CLI derives from
//! `dirs::home_dir()` — queue, config, log — lands in a temp directory instead of
//! the developer's real state. `$HOME` is process-global, so this is deliberately
//! the only test in this binary.
//!
//! Unix only, and deliberately so: `$HOME` isolates this run on unix, where
//! `dirs::home_dir()` reads `env::var_os("HOME")` first, but **not on Windows**,
//! where `dirs-7.0.0/src/win.rs:5` resolves `known_folder_profile()` and ignores
//! the variable entirely. On Windows the binary under test would read and write
//! the real `%USERPROFILE%\.chronova\queue.db`, shared with every other test in
//! the job, and assertions about queue contents would answer for whatever else
//! had run first. Do not ungate this without first giving the queue path an
//! override that does not go through `dirs`.
#![cfg(unix)]

use chronova_cli::config::Config;
use chronova_cli::heartbeat::{Heartbeat, HeartbeatManager, HeartbeatManagerExt};
use chronova_cli::queue::{Queue, QueueOps};
use chronova_cli::sync::SyncStatus;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

mod support;
use support::{queued_heartbeat, HEARTBEAT_PATH};

#[tokio::test]
async fn a_refused_key_is_reported_and_the_batch_is_requeued_untouched() {
    let home = tempfile::tempdir().expect("temp home");
    std::env::set_var("HOME", home.path());

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(HEARTBEAT_PATH))
        .respond_with(ResponseTemplate::new(401))
        .expect(3)
        .mount(&server)
        .await;

    let queued: Vec<Heartbeat> = (1..=3)
        .map(|n| queued_heartbeat(&format!("hb-{}", n)))
        .collect();
    let ids: Vec<String> = queued.iter().map(|hb| hb.id.clone()).collect();

    let queue = Queue::new().expect("queue under the throwaway home");
    queue.add_batch(queued).expect("heartbeats queued");

    let config = Config {
        api_key: Some("rejected-key".to_string()),
        api_url: Some(server.uri()),
        ..Config::default()
    };
    let manager = HeartbeatManager::new_with_queue(config, queue).expect("manager builds");

    let error = manager
        .manual_sync()
        .await
        .expect_err("a refused key is reported, not swallowed");
    assert!(
        error.to_string().contains("Authentication error"),
        "the error names the real cause, got: {}",
        error
    );

    let queue = Queue::new().expect("queue reopens");
    let pending: Vec<String> = queue
        .get_pending(None, Some(SyncStatus::Pending))
        .expect("queue readable")
        .into_iter()
        .map(|hb| hb.id)
        .collect();
    assert_eq!(
        pending, ids,
        "the batch is back on the queue, pending and in order"
    );

    for id in &ids {
        assert_eq!(
            queue.get_retry_count(id).expect("retry count readable"),
            0,
            "a bad key must not spend the heartbeat's attempts; the key is what has to change"
        );
    }

    assert_eq!(
        server.received_requests().await.unwrap().len(),
        3,
        "one cascade for the batch and nothing more: falling back to per-heartbeat \
         sends would make this 3 + 3 x 3"
    );
}
