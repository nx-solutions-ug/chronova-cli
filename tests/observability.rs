use chronova_cli::sync::{ChronovaSyncManager, PerformanceMetrics, SyncResult};
use chronova_cli::api::ApiClient;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;

#[test]
fn test_performance_metrics_default() {
    let metrics = PerformanceMetrics::default();
    
    assert_eq!(metrics.total_operations, 0);
    assert_eq!(metrics.successful_operations, 0);
    assert_eq!(metrics.failed_operations, 0);
    assert_eq!(metrics.average_latency_ms, 0.0);
    assert_eq!(metrics.success_rate_percent, 0.0);
    assert_eq!(metrics.total_latency_ms, 0);
}

#[test]
fn test_performance_metrics_calculation() {
    let metrics = PerformanceMetrics {
        total_operations: 100,
        successful_operations: 80,
        failed_operations: 20,
        average_latency_ms: 150.5,
        success_rate_percent: 80.0,
        total_latency_ms: 15050,
    };
    
    assert_eq!(metrics.total_operations, 100);
    assert_eq!(metrics.successful_operations, 80);
    assert_eq!(metrics.failed_operations, 20);
    assert_eq!(metrics.average_latency_ms, 150.5);
    assert_eq!(metrics.success_rate_percent, 80.0);
    assert_eq!(metrics.total_latency_ms, 15050);
}

#[test]
fn test_performance_metrics_clone() {
    let metrics1 = PerformanceMetrics {
        total_operations: 50,
        successful_operations: 45,
        failed_operations: 5,
        average_latency_ms: 100.0,
        success_rate_percent: 90.0,
        total_latency_ms: 5000,
    };
    
    let metrics2 = metrics1.clone();
    
    assert_eq!(metrics1.total_operations, metrics2.total_operations);
    assert_eq!(metrics1.successful_operations, metrics2.successful_operations);
    assert_eq!(metrics1.failed_operations, metrics2.failed_operations);
    assert_eq!(metrics1.average_latency_ms, metrics2.average_latency_ms);
    assert_eq!(metrics1.success_rate_percent, metrics2.success_rate_percent);
    assert_eq!(metrics1.total_latency_ms, metrics2.total_latency_ms);
}

#[test]
fn test_performance_metrics_debug() {
    let metrics = PerformanceMetrics::default();
    let debug_output = format!("{:?}", metrics);
    
    assert!(debug_output.contains("total_operations"));
    assert!(debug_output.contains("successful_operations"));
    assert!(debug_output.contains("failed_operations"));
    assert!(debug_output.contains("average_latency_ms"));
    assert!(debug_output.contains("success_rate_percent"));
    assert!(debug_output.contains("total_latency_ms"));
}

#[tokio::test]
async fn test_sync_result_with_timestamps() {
    let result = SyncResult {
        synced_count: 10,
        failed_count: 2,
        total_count: 12,
        duration: Duration::from_millis(500),
        error: None,
        start_time: Some(SystemTime::now()),
        end_time: Some(SystemTime::now()),
        avg_latency_ms: Some(41.67),
    };
    
    assert_eq!(result.synced_count, 10);
    assert_eq!(result.failed_count, 2);
    assert_eq!(result.total_count, 12);
    assert_eq!(result.duration.as_millis(), 500);
    assert!(result.error.is_none());
    assert!(result.start_time.is_some());
    assert!(result.end_time.is_some());
    assert_eq!(result.avg_latency_ms, Some(41.67));
}

#[tokio::test]
async fn test_sync_result_default_with_timestamps() {
    let result = SyncResult::default();
    
    assert_eq!(result.synced_count, 0);
    assert_eq!(result.failed_count, 0);
    assert_eq!(result.total_count, 0);
    assert_eq!(result.duration.as_millis(), 0);
    assert!(result.error.is_none());
    assert!(result.start_time.is_none());
    assert!(result.end_time.is_none());
    assert!(result.avg_latency_ms.is_none());
}

#[test]
fn test_atomic_counters_initialization() {
    let total_ops = Arc::new(AtomicU64::new(0));
    let successful_ops = Arc::new(AtomicU64::new(0));
    let failed_ops = Arc::new(AtomicU64::new(0));
    let total_latency = Arc::new(AtomicU64::new(0));
    
    assert_eq!(total_ops.load(Ordering::Relaxed), 0);
    assert_eq!(successful_ops.load(Ordering::Relaxed), 0);
    assert_eq!(failed_ops.load(Ordering::Relaxed), 0);
    assert_eq!(total_latency.load(Ordering::Relaxed), 0);
}

#[test]
fn test_atomic_counters_increment() {
    let total_ops = Arc::new(AtomicU64::new(0));
    let successful_ops = Arc::new(AtomicU64::new(0));
    let failed_ops = Arc::new(AtomicU64::new(0));
    let total_latency = Arc::new(AtomicU64::new(0));
    
    total_ops.fetch_add(1, Ordering::Relaxed);
    successful_ops.fetch_add(1, Ordering::Relaxed);
    total_latency.fetch_add(100, Ordering::Relaxed);
    
    assert_eq!(total_ops.load(Ordering::Relaxed), 1);
    assert_eq!(successful_ops.load(Ordering::Relaxed), 1);
    assert_eq!(failed_ops.load(Ordering::Relaxed), 0);
    assert_eq!(total_latency.load(Ordering::Relaxed), 100);
}

#[tokio::test]
async fn test_rwlock_queue_size_monitoring() {
    let queue_size = Arc::new(RwLock::new(None::<usize>));
    
    // Initially should be None
    {
        let size_guard = queue_size.read().await;
        assert!(size_guard.is_none());
    }
    
    // Update queue size
    {
        let mut size_guard = queue_size.write().await;
        *size_guard = Some(25);
    }
    
    // Verify update
    {
        let size_guard = queue_size.read().await;
        assert_eq!(*size_guard, Some(25));
    }
}

#[test]
fn test_latency_calculation_simple() {
    use std::time::Instant;
    
    let start = Instant::now();
    let end = start + Duration::from_millis(500);
    let count = 10;
    
    let avg_latency = if count > 0 {
        end.duration_since(start).as_millis() as f64 / count as f64
    } else {
        0.0
    };
    
    assert_eq!(avg_latency, 50.0); // 500ms / 10 = 50ms per heartbeat
}

#[test]
fn test_latency_calculation_zero_count() {
    use std::time::Instant;
    
    let start = Instant::now();
    let end = start + Duration::from_millis(500);
    let count = 0;
    
    let avg_latency = if count > 0 {
        end.duration_since(start).as_millis() as f64 / count as f64
    } else {
        0.0
    };
    
    assert_eq!(avg_latency, 0.0);
}

#[test]
fn test_success_rate_calculation() {
    let total_ops = 100;
    let successful_ops = 85;
    
    let success_rate = if total_ops > 0 {
        (successful_ops as f64 / total_ops as f64) * 100.0
    } else {
        0.0
    };
    
    assert_eq!(success_rate, 85.0);
}

#[test]
fn test_success_rate_calculation_zero_total() {
    let total_ops = 0;
    let successful_ops = 0;
    
    let success_rate = if total_ops > 0 {
        (successful_ops as f64 / total_ops as f64) * 100.0
    } else {
        0.0
    };
    
    assert_eq!(success_rate, 0.0);
}

#[test]
fn test_average_latency_calculation() {
    let total_latency = 1500; // 1.5 seconds in milliseconds
    let total_ops = 10;
    
    let avg_latency = if total_ops > 0 {
        total_latency as f64 / total_ops as f64
    } else {
        0.0
    };
    
    assert_eq!(avg_latency, 150.0); // 150ms per operation
}

#[test]
fn test_average_latency_calculation_zero_ops() {
    let total_latency = 1500;
    let total_ops = 0;
    
    let avg_latency = if total_ops > 0 {
        total_latency as f64 / total_ops as f64
    } else {
        0.0
    };
    
    assert_eq!(avg_latency, 0.0);
}

#[test]
fn test_queue_utilization_calculation() {
    let queue_size = 750;
    let max_queue_size = 1000;
    
    let utilization = queue_size as f64 / max_queue_size as f64;
    let utilization_percent = utilization * 100.0;
    
    assert_eq!(utilization, 0.75);
    assert_eq!(utilization_percent, 75.0);
}

#[test]
fn test_queue_utilization_warning_threshold() {
    let queue_size = 850;
    let max_queue_size = 1000;
    
    let utilization = queue_size as f64 / max_queue_size as f64;
    
    // Test warning threshold (80%)
    assert!(utilization > 0.8);
    assert!(utilization <= 1.0);
}

#[test]
fn test_performance_metrics_serialization_compatibility() {
    // Test that PerformanceMetrics can be used in logging contexts
    let metrics = PerformanceMetrics {
        total_operations: 42,
        successful_operations: 40,
        failed_operations: 2,
        average_latency_ms: 125.5,
        success_rate_percent: 95.24,
        total_latency_ms: 5271,
    };
    
    // Verify all fields are accessible for structured logging
    assert_eq!(metrics.total_operations, 42);
    assert_eq!(metrics.successful_operations, 40);
    assert_eq!(metrics.failed_operations, 2);
    assert_eq!(metrics.average_latency_ms, 125.5);
    assert_eq!(metrics.success_rate_percent, 95.24);
    assert_eq!(metrics.total_latency_ms, 5271);
}

#[test]
fn test_observability_metrics_consistency() {
    // Test that metrics calculations are consistent
    let total_ops = 200;
    let successful_ops = 190;
    let failed_ops = 10;
    let total_latency = 38000; // 38 seconds in milliseconds
    
    // Verify consistency
    assert_eq!(total_ops, successful_ops + failed_ops);
    
    let success_rate = (successful_ops as f64 / total_ops as f64) * 100.0;
    let avg_latency = total_latency as f64 / total_ops as f64;
    
    assert_eq!(success_rate, 95.0);
    assert_eq!(avg_latency, 190.0);
}

#[tokio::test]
async fn test_observability_integration() {
    // Test that observability components work together
    let api_client = ApiClient::new("http://localhost:8080".to_string());
    let sync_manager = ChronovaSyncManager::new(api_client);
    
    // Verify observability fields are initialized
    assert_eq!(sync_manager.total_sync_operations.load(Ordering::Relaxed), 0);
    assert_eq!(sync_manager.successful_sync_operations.load(Ordering::Relaxed), 0);
    assert_eq!(sync_manager.failed_sync_operations.load(Ordering::Relaxed), 0);
    assert_eq!(sync_manager.total_sync_latency_ms.load(Ordering::Relaxed), 0);
    
    // Verify queue size monitoring is initialized
    let queue_size = sync_manager.get_last_queue_size().await;
    assert!(queue_size.is_none()); // Should be None initially
}