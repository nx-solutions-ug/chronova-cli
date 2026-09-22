//! A rate-limited flush must stop and leave the queue exactly as it found it.
//!
//! The run happens under a throwaway `$HOME`, so every path the CLI derives
//! from `dirs::home_dir()` — queue, config, log — lands in a temp directory
//! instead of the developer's real state. `$HOME` is process-global, so this is
//! deliberately the only test in this binary; `flush_stops_when_credentials_are_refused`
//! covers the other "stop sending" answer in a process of its own.
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
async fn a_rate_limited_batch_is_requeued_untouched_and_the_flush_stops() {
    let home = tempfile::tempdir().expect("temp home");
    std::env::set_var("HOME", home.path());

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(HEARTBEAT_PATH))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "30"))
        .expect(1)
        .mount(&server)
        .await;

    let queued: Vec<Heartbeat> = (1..=3)
        .map(|n| queued_heartbeat(&format!("hb-{}", n)))
        .collect();
    let ids: Vec<String> = queued.iter().map(|hb| hb.id.clone()).collect();

    let queue = Queue::new().expect("queue under the throwaway home");
    queue.add_batch(queued).expect("heartbeats queued");

    let config = Config {
        api_key: Some("test-key".to_string()),
        api_url: Some(server.uri()),
        ..Config::default()
    };
    let manager = HeartbeatManager::new_with_queue(config, queue).expect("manager builds");

    let result = manager
        .manual_sync()
        .await
        .expect("the flush itself is fine");

    assert_eq!(result.synced_count, 0, "nothing reached the server");
    assert_eq!(
        result.failed_count, 3,
        "all three count as not synced, which is what ai_sync rolls back on"
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
            "a rate limit must not spend one of the heartbeat's attempts"
        );
    }

    assert_eq!(
        server.received_requests().await.unwrap().len(),
        1,
        "a 429 must not be answered with more requests: the old code slept and \
         re-sent the batch, or fell back to one request per heartbeat"
    );
}
