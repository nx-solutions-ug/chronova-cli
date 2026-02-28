use chronova_cli::queue::{Queue, QueueOps};
use chronova_cli::sync::SyncStatus;

#[tokio::test]
async fn test_retry_mechanism_integration() {
    // Create a test queue directly to test the retry logic
    let queue = Queue::new().unwrap();

    // Clear any existing heartbeats
    let _ = queue.cleanup_old_entries(0);

    // Create a test heartbeat
    let heartbeat = create_test_heartbeat("test-retry-1");
    queue.add(heartbeat.clone()).unwrap();

    // Simulate 2 failed attempts
    for i in 0..2 {
        // Mark as failed with incrementing retry count
        queue
            .update_sync_status(
                &heartbeat.id,
                SyncStatus::Failed,
                Some(format!("Attempt {} failed", i + 1)),
            )
            .unwrap();
        queue.increment_retry(&heartbeat.id).unwrap();
    }

    // Verify initial state: failed with 2 retries
    let retry_count = queue.get_retry_count(&heartbeat.id).unwrap();
    assert_eq!(retry_count, 2);

    // Test retry-eligible logic: heartbeats with retry_count < 3 should be retry-eligible
    // In the actual implementation, prepare_retry_eligible_failures() would update these to pending
    let failed_heartbeats = queue
        .get_pending(Some(1000), Some(SyncStatus::Failed))
        .unwrap();
    assert_eq!(failed_heartbeats.len(), 1);

    let heartbeat_to_retry = &failed_heartbeats[0];
    let retry_count = queue.get_retry_count(&heartbeat_to_retry.id).unwrap();

    // This heartbeat should be retry-eligible (retry_count < 3)
    assert!(retry_count < 3);

    // Simulate what prepare_retry_eligible_failures does: update to pending for retry
    queue
        .update_sync_status(
            &heartbeat_to_retry.id,
            SyncStatus::Pending,
            Some(format!("Retry eligible (attempt {})", retry_count)),
        )
        .unwrap();

    // Verify the heartbeat is now pending for retry
    let pending = queue.get_pending(None, Some(SyncStatus::Pending)).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, heartbeat.id);

    // Simulate another failed attempt (this will be attempt 3)
    queue
        .update_sync_status(
            &heartbeat.id,
            SyncStatus::Syncing,
            Some("Attempting sync".to_string()),
        )
        .unwrap();

    // Simulate failure and increment to retry_count = 3
    queue.increment_retry(&heartbeat.id).unwrap();
    queue
        .update_sync_status(
            &heartbeat.id,
            SyncStatus::Failed,
            Some("Attempt 3 failed".to_string()),
        )
        .unwrap();

    // Verify retry count is now 3
    let retry_count = queue.get_retry_count(&heartbeat.id).unwrap();
    assert_eq!(retry_count, 3);

    // Test retry-eligible logic again: heartbeat with retry_count >= 3 should NOT be retry-eligible
    let failed_heartbeats = queue
        .get_pending(Some(1000), Some(SyncStatus::Failed))
        .unwrap();
    assert_eq!(failed_heartbeats.len(), 1);

    let heartbeat_not_retry = &failed_heartbeats[0];
    let retry_count = queue.get_retry_count(&heartbeat_not_retry.id).unwrap();

    // This heartbeat should NOT be retry-eligible (retry_count >= 3)
    assert!(retry_count >= 3);

    // Simulate what would happen in process_queue: mark as permanently failed
    queue
        .update_sync_status(
            &heartbeat_not_retry.id,
            SyncStatus::PermanentFailure,
            Some(format!("Permanent failure after {} attempts", retry_count)),
        )
        .unwrap();

    // Verify the heartbeat is now permanently failed
    let permanent_failures = queue
        .get_pending(None, Some(SyncStatus::PermanentFailure))
        .unwrap();
    assert_eq!(permanent_failures.len(), 1);
    assert_eq!(permanent_failures[0].id, heartbeat.id);

    // Verify stats reflect the permanent failure
    let stats = queue.get_sync_stats().unwrap();
    assert_eq!(stats.permanent_failures, 1);
    assert_eq!(stats.failed, 0);
}

#[tokio::test]
async fn test_retry_mechanism_successful_retry() {
    let queue = Queue::new().unwrap();

    // Clear any existing heartbeats
    let _ = queue.cleanup_old_entries(0);

    // Create a test heartbeat
    let heartbeat = create_test_heartbeat("test-retry-2");
    queue.add(heartbeat.clone()).unwrap();

    // Simulate 1 failed attempt
    queue
        .update_sync_status(
            &heartbeat.id,
            SyncStatus::Failed,
            Some("Attempt 1 failed".to_string()),
        )
        .unwrap();
    queue.increment_retry(&heartbeat.id).unwrap();

    // Verify initial state
    let retry_count = queue.get_retry_count(&heartbeat.id).unwrap();
    assert_eq!(retry_count, 1);

    // Mark for retry (simulate prepare_retry_eligible_failures)
    queue
        .update_sync_status(
            &heartbeat.id,
            SyncStatus::Pending,
            Some("Retry eligible".to_string()),
        )
        .unwrap();

    // Simulate successful sync on retry attempt
    queue
        .update_sync_status(
            &heartbeat.id,
            SyncStatus::Syncing,
            Some("Attempting sync".to_string()),
        )
        .unwrap();

    // Simulate success - remove from queue
    queue.remove(&heartbeat.id).unwrap();

    // Verify heartbeat is removed from queue
    let count = queue.count().unwrap();
    assert_eq!(count, 0);

    let stats = queue.get_sync_stats().unwrap();
    assert_eq!(stats.total, 0);
}

fn create_test_heartbeat(id: &str) -> chronova_cli::heartbeat::Heartbeat {
    chronova_cli::heartbeat::Heartbeat {
        id: id.to_string(),
        entity: format!("/path/to/file_{}.rs", id),
        entity_type: "file".to_string(),
        time: chrono::Utc::now().timestamp_millis() as f64 / 1000.0,
        project: Some("test-project".to_string()),
        branch: Some("main".to_string()),
        language: Some("Rust".to_string()),
        is_write: false,
        lines: Some(100),
        lineno: Some(10),
        cursorpos: Some(5),
        user_agent: Some("test/1.0".to_string()),
        category: Some("coding".to_string()),
        machine: Some("test-machine".to_string()),
        editor: Some(chronova_cli::heartbeat::EditorInfo {
            name: "test-editor".to_string(),
            version: Some("1.0".to_string()),
        }),
        operating_system: Some(chronova_cli::heartbeat::OsInfo {
            name: "test-os".to_string(),
            title: Some("Test OS".to_string()),
            version: Some("1.0".to_string()),
        }),
        commit_hash: None,
        commit_author: None,
        commit_message: None,
        repository_url: None,
        dependencies: Vec::new(),
    }
}
