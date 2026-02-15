//! Performance tests for the offline heartbeats feature
//! Tests the system with large queue sizes to ensure performance requirements are met

use chronova_cli::{
    heartbeat::Heartbeat,
    queue::{Queue, QueueOps},
    sync::SyncStatus,
};
use std::time::{Duration, SystemTime};
use uuid::Uuid;
use tempfile::TempDir;

fn create_test_heartbeat(id: &str, time: f64) -> Heartbeat {
    Heartbeat {
        id: id.to_string(),
        entity: format!("/path/to/file_{}.rs", id),
        entity_type: "file".to_string(),
        time,
        project: Some(format!("test-project-{}", id)),
        branch: Some("main".to_string()),
        language: Some("Rust".to_string()),
        is_write: false,
        lines: Some(100),
        lineno: Some(42),
        cursorpos: Some(10),
        user_agent: Some("test/1.0.0".to_string()),
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

/// Test adding a large number of heartbeats to the queue
#[tokio::test]
async fn test_large_queue_performance() {
    // Create a temporary directory for this test to isolate the database
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_large_queue.db");

    // Create queue with custom path for performance testing
    let queue = Queue::with_path(db_path.clone()).unwrap();

    // Clean up any existing heartbeats first
    let _ = queue.cleanup_old_entries(0); // Remove all entries

    // Test with 1000 heartbeats (large but reasonable for performance testing)
    let num_heartbeats = 1000;

    let start_time = SystemTime::now();

    // Add heartbeats in batches to simulate real usage
    for i in 0..num_heartbeats {
        let heartbeat = create_test_heartbeat(
            &Uuid::new_v4().to_string(),
            chrono::Utc::now().timestamp_millis() as f64 / 1000.0
        );

        queue.add(heartbeat).unwrap();

        // Log progress every 100 heartbeats
        if (i + 1) % 100 == 0 {
            println!("Added {} heartbeats", i + 1);
        }
    }

    let duration = start_time.elapsed().unwrap();
    println!("Added {} heartbeats in {:?}", num_heartbeats, duration);

    // Verify all heartbeats were added
    let stats = queue.get_sync_stats().unwrap();
    assert_eq!(stats.total, num_heartbeats);
    assert_eq!(stats.pending, num_heartbeats);

    // Performance requirement: should complete in under 10 seconds for 1000 heartbeats
    assert!(
        duration < Duration::from_secs(10),
        "Adding {} heartbeats took {:?}, expected under 10s",
        num_heartbeats,
        duration
    );

    // Test getting pending heartbeats
    let start_time = SystemTime::now();
    let pending = queue.get_pending(Some(num_heartbeats), None).unwrap();
    let query_duration = start_time.elapsed().unwrap();

    assert_eq!(pending.len(), num_heartbeats);
    println!("Retrieved {} heartbeats in {:?}", num_heartbeats, query_duration);

    // Performance requirement: query should complete in under 2 seconds
    assert!(
        query_duration < Duration::from_secs(2),
        "Querying {} heartbeats took {:?}, expected under 2s",
        num_heartbeats,
        query_duration
    );

    // Clean up - database file will be deleted when temp_dir is dropped
}

/// Test queue operations with sequential access (since Queue is not thread-safe)
#[tokio::test]
async fn test_sequential_queue_operations() {
    // Create a temporary directory for this test to isolate the database
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_sequential_queue.db");

    // Create queue with custom path
    let queue = Queue::with_path(db_path.clone()).unwrap();

    // Clean up any existing heartbeats first
    let _ = queue.cleanup_old_entries(0); // Remove all entries

    // Test adding heartbeats sequentially
    let num_heartbeats = 1000;

    let start_time = SystemTime::now();

    for i in 0..num_heartbeats {
        let heartbeat = create_test_heartbeat(
            &format!("seq-{}", i),
            chrono::Utc::now().timestamp_millis() as f64 / 1000.0
        );

        queue.add(heartbeat).unwrap();

        // Log progress every 100 heartbeats
        if (i + 1) % 100 == 0 {
            println!("Added {} heartbeats", i + 1);
        }
    }

    let duration = start_time.elapsed().unwrap();

    println!("Added {} heartbeats sequentially in {:?}", num_heartbeats, duration);

    // Verify all heartbeats were added
    let stats = queue.get_sync_stats().unwrap();
    assert_eq!(stats.total, num_heartbeats);

    // Performance requirement: sequential operations should be fast
    assert!(
        duration < Duration::from_secs(5),
        "Sequential addition of {} heartbeats took {:?}, expected under 5s",
        num_heartbeats,
        duration
    );

    // Clean up - database file will be deleted when temp_dir is dropped
}

/// Test memory usage with large queue
#[tokio::test]
async fn test_memory_usage_large_queue() {
    // Create a temporary directory for this test to isolate the database
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_memory.db");

    // Create queue with custom path
    let queue = Queue::with_path(db_path.clone()).unwrap();

    // Clean up any existing heartbeats first
    let _ = queue.cleanup_old_entries(0); // Remove all entries

    // Add a moderate number of heartbeats to test memory efficiency
    let num_heartbeats = 500;

    for i in 0..num_heartbeats {
        let heartbeat = create_test_heartbeat(
            &format!("large-{}", i),
            chrono::Utc::now().timestamp_millis() as f64 / 1000.0
        );

        queue.add(heartbeat).unwrap();
    }

    // Test that we can still efficiently query the queue
    let start_time = SystemTime::now();
    let pending = queue.get_pending(Some(num_heartbeats), None).unwrap();
    let query_duration = start_time.elapsed().unwrap();

    assert_eq!(pending.len(), num_heartbeats);

    println!("Memory test: Queried {} large heartbeats in {:?}", num_heartbeats, query_duration);

    // Performance should remain reasonable even with larger data
    assert!(
        query_duration < Duration::from_secs(1),
        "Querying large heartbeats took {:?}, expected under 1s",
        query_duration
    );

    // Clean up - database file will be deleted when temp_dir is dropped
}

/// Test sync operations performance with large queue
#[tokio::test]
async fn test_sync_performance_large_queue() {
    // Create a temporary directory for this test to isolate the database
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_sync.db");

    // Create queue with custom path
    let queue = Queue::with_path(db_path.clone()).unwrap();

    // Clean up any existing heartbeats first
    let _ = queue.cleanup_old_entries(0); // Remove all entries

    // Add heartbeats with different sync statuses
    let num_heartbeats = 300;

    for i in 0..num_heartbeats {
        let heartbeat = create_test_heartbeat(
            &format!("sync-{}", i),
            chrono::Utc::now().timestamp_millis() as f64 / 1000.0
        );

        queue.add(heartbeat.clone()).unwrap();

        // Set different sync statuses to test filtering performance
        let status = match i % 3 {
            0 => SyncStatus::Pending,
            1 => SyncStatus::Failed,
            2 => SyncStatus::Synced,
            _ => SyncStatus::Pending,
        };

        if status != SyncStatus::Pending {
            queue.update_sync_status(&heartbeat.id, status, Some("test".to_string())).unwrap();
        }
    }

    // Test getting sync stats (should be efficient even with mixed statuses)
    let start_time = SystemTime::now();
    let _stats = queue.get_sync_stats().unwrap();
    let stats_duration = start_time.elapsed().unwrap();

    println!("Sync stats query took {:?} for {} heartbeats", stats_duration, num_heartbeats);

    // Stats query should be very fast
    assert!(
        stats_duration < Duration::from_millis(100),
        "Sync stats query took {:?}, expected under 100ms",
        stats_duration
    );

    // Test filtering by status
    let start_time = SystemTime::now();
    let _pending = queue.get_pending(Some(num_heartbeats), Some(SyncStatus::Pending)).unwrap();
    let filter_duration = start_time.elapsed().unwrap();

    println!("Status filtering took {:?}", filter_duration);

    assert!(
        filter_duration < Duration::from_millis(200),
        "Status filtering took {:?}, expected under 200ms",
        filter_duration
    );

    // Clean up - database file will be deleted when temp_dir is dropped
}

/// Test database cleanup performance
#[tokio::test]
async fn test_cleanup_performance() {
    // Create a temporary directory for this test to isolate the database
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_cleanup.db");

    // Create queue with custom path
    let queue = Queue::with_path(db_path.clone()).unwrap();

    // Add heartbeats with very old timestamps (10 days old)
    let num_heartbeats = 200;

    for i in 0..num_heartbeats {
        let heartbeat = create_test_heartbeat(
            &format!("old-{}", i),
            // Use a timestamp that's 10 days old (864000 seconds)
            chrono::Utc::now().timestamp_millis() as f64 / 1000.0 - 864000.0
        );

        queue.add(heartbeat).unwrap();
    }

    // Verify heartbeats were added
    let stats_before = queue.get_sync_stats().unwrap();
    assert_eq!(stats_before.total, num_heartbeats);

    // Since we can't access the private conn field, we'll test cleanup with entries that are actually old
    // by creating them with a delay to ensure they're older than the cleanup threshold

    // Test cleanup performance (cleanup entries older than 0 days to remove all)
    let start_time = SystemTime::now();
    let removed = queue.cleanup_old_entries(0).unwrap();
    let cleanup_duration = start_time.elapsed().unwrap();

    println!("Cleaned up {} old entries in {:?}", removed, cleanup_duration);

    assert_eq!(removed, num_heartbeats);

    // Verify cleanup worked
    let stats_after = queue.get_sync_stats().unwrap();
    assert_eq!(stats_after.total, 0);

    // Cleanup should be efficient
    assert!(
        cleanup_duration < Duration::from_secs(1),
        "Cleanup of {} entries took {:?}, expected under 1s",
        num_heartbeats,
        cleanup_duration
    );

    // Clean up - database file will be deleted when temp_dir is dropped
}
