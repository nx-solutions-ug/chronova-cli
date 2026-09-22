//! One offline invocation must not spend the queue's whole retry budget.
//!
//! A heartbeat written while the network is down has to survive until the
//! network comes back. If a single `--entity` call re-sends it once per attempt
//! and marks it `PermanentFailure`, nothing ever promotes it again: the
//! retry-eligibility pass filters on `Failed`, and the drain query asks for
//! `pending`. The heartbeat is then stranded on disk forever.
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
use chronova_cli::heartbeat::{HeartbeatManager, HeartbeatManagerExt};
use chronova_cli::queue::{Queue, QueueOps};
use chronova_cli::sync::SyncStatus;

mod support;
use support::queued_heartbeat;

const UNREACHABLE_API: &str = "http://127.0.0.1:1/api/v1";

#[tokio::test]
async fn an_offline_flush_leaves_the_heartbeat_drainable() {
    let home = tempfile::tempdir().expect("temp home");
    std::env::set_var("HOME", home.path());

    let queue = Queue::new().expect("queue under the throwaway home");
    queue
        .add(queued_heartbeat("hb-offline"))
        .expect("heartbeat queued");

    let config = Config {
        api_key: Some("test-key".to_string()),
        api_url: Some(UNREACHABLE_API.to_string()),
        timeout: Some(1),
        ..Config::default()
    };
    let manager = HeartbeatManager::new_with_queue(config, queue).expect("manager builds");

    let result = manager
        .manual_sync()
        .await
        .expect("an offline flush is not an error");
    assert_eq!(result.synced_count, 0, "the server is unreachable");

    let queue = Queue::new().expect("queue reopens");
    assert_eq!(
        queue.get_retry_count("hb-offline").expect("retry readable"),
        0,
        "being offline is not the heartbeat's fault and must not spend its attempts"
    );

    let pending: Vec<String> = queue
        .get_pending(None, Some(SyncStatus::Pending))
        .expect("queue readable")
        .into_iter()
        .map(|hb| hb.id)
        .collect();
    assert_eq!(
        pending,
        vec!["hb-offline".to_string()],
        "still pending, so --sync-offline-activity can drain it later"
    );

    assert!(
        queue
            .get_pending(None, Some(SyncStatus::PermanentFailure))
            .expect("queue readable")
            .is_empty(),
        "one offline invocation must not give up on a heartbeat"
    );

    let drained = manager
        .manual_sync()
        .await
        .expect("a later drain still finds it");
    assert_eq!(
        drained.failed_count, 1,
        "the heartbeat is still there to be attempted again"
    );
}
