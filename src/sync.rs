use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, Instant};
use thiserror::Error;
use tokio::sync::RwLock;
 
use crate::api::ApiClient;
use crate::queue::QueueOps;

/// Represents the synchronization status of a heartbeat
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[derive(Default)]
pub enum SyncStatus {
    /// Heartbeat is pending synchronization
    #[default]
    Pending,
    /// Heartbeat is currently being synchronized
    Syncing,
    /// Heartbeat was successfully synchronized
    Synced,
    /// Heartbeat synchronization failed (will be retried)
    Failed,
    /// Heartbeat synchronization permanently failed (no more retries)
    PermanentFailure,
}


impl From<&str> for SyncStatus {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "pending" => Self::Pending,
            "syncing" => Self::Syncing,
            "synced" => Self::Synced,
            "failed" => Self::Failed,
            "permanent_failure" => Self::PermanentFailure,
            _ => Self::Pending, // Default to pending for unknown values
        }
    }
}

impl From<SyncStatus> for String {
    fn from(status: SyncStatus) -> Self {
        match status {
            SyncStatus::Pending => "pending".to_string(),
            SyncStatus::Syncing => "syncing".to_string(),
            SyncStatus::Synced => "synced".to_string(),
            SyncStatus::Failed => "failed".to_string(),
            SyncStatus::PermanentFailure => "permanent_failure".to_string(),
        }
    }
}

/// Represents the result of a sync operation
#[derive(Debug, Clone)]
#[derive(Default)]
pub struct SyncResult {
    /// Number of heartbeats successfully synced
    pub synced_count: usize,
    /// Number of heartbeats that failed to sync
    pub failed_count: usize,
    /// Total number of heartbeats processed
    pub total_count: usize,
    /// Duration of the sync operation
    pub duration: std::time::Duration,
    /// Error if the sync operation failed completely
    pub error: Option<SyncError>,
    /// Timestamp when the sync operation started
    pub start_time: Option<SystemTime>,
    /// Timestamp when the sync operation ended
    pub end_time: Option<SystemTime>,
    /// Average sync latency per heartbeat in milliseconds
    pub avg_latency_ms: Option<f64>,
}


/// Represents a summary of sync status
#[derive(Debug, Clone)]
#[derive(Default)]
pub struct SyncStatusSummary {
    /// Number of pending heartbeats
    pub pending: usize,
    /// Number of syncing heartbeats
    pub syncing: usize,
    /// Number of synced heartbeats
    pub synced: usize,
    /// Number of failed heartbeats
    pub failed: usize,
    /// Number of permanent failures
    pub permanent_failures: usize,
    /// Total number of heartbeats
    pub total: usize,
    /// Last sync attempt timestamp
    pub last_sync: Option<SystemTime>,
}


/// Error type for sync operations
#[derive(Error, Debug, Clone)]
pub enum SyncError {
    #[error("Network error: {0}")]
    Network(String),
    #[error("Authentication error: {0}")]
    Auth(String),
    #[error("Rate limit exceeded: {0}")]
    RateLimit(String),
    #[error("Database error: {0}")]
    Database(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Invalid configuration: {0}")]
    Config(String),
    #[error("Unknown error: {0}")]
    Unknown(String),
}

/// Configuration for retry strategy with exponential backoff and jitter
#[derive(Debug, Clone)]
pub struct RetryStrategy {
    /// Base delay in seconds for exponential backoff
    pub base_delay_seconds: u64,
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Maximum delay in seconds (cap for exponential growth)
    pub max_delay_seconds: u64,
    /// Whether to use jitter to avoid thundering herd problem
    pub use_jitter: bool,
}

impl Default for RetryStrategy {
    fn default() -> Self {
        Self {
            base_delay_seconds: 1,
            max_attempts: 5,
            max_delay_seconds: 60,
            use_jitter: true,
        }
    }
}

impl RetryStrategy {
    /// Calculate the delay for a specific retry attempt
    pub fn calculate_delay(&self, attempt: u32) -> std::time::Duration {
        if attempt == 0 {
            return std::time::Duration::from_secs(0);
        }

        // Exponential backoff: base_delay * 2^(attempt-1)
        let exponent = attempt - 1;
        let mut delay = self.base_delay_seconds * 2u64.pow(exponent);
        
        // Apply jitter if enabled (random factor between 0.5 and 1.5)
        if self.use_jitter {
            let jitter_factor = 0.5 + (rand::random::<f64>() * 1.0); // 0.5 to 1.5
            delay = (delay as f64 * jitter_factor) as u64;
        }
        
        // Cap at maximum delay
        delay = delay.min(self.max_delay_seconds);
        
        std::time::Duration::from_secs(delay)
    }
    
    /// Determine if a retry should be attempted based on the current attempt count
    pub fn should_retry(&self, attempt: u32) -> bool {
        attempt < self.max_attempts
    }
    
    /// Check if the error is retryable
    pub fn is_retryable_error(error: &SyncError) -> bool {
        match error {
            SyncError::Network(_) => true,
            SyncError::RateLimit(_) => true,
            SyncError::Database(_) => true,
            SyncError::Serialization(_) => true,
            SyncError::Unknown(_) => true,
            SyncError::Auth(_) => false, // Auth errors are not retryable
            SyncError::Config(_) => false, // Config errors are not retryable
        }
    }
}

/// Configuration for sync operations
#[derive(Debug, Clone)]
pub struct SyncConfig {
    /// Enable offline heartbeats storage
    pub enabled: bool,
    /// Maximum number of heartbeats to store offline
    pub max_queue_size: usize,
    /// Batch size for each sync network call
    pub batch_size: usize,
    /// Sync interval in seconds when online
    pub sync_interval_seconds: u64,
    /// Maximum number of retry attempts for failed syncs
    pub max_retry_attempts: u32,
    /// Base delay for exponential backoff in seconds
    pub retry_base_delay_seconds: u64,
    /// Maximum delay for exponential backoff in seconds
    pub retry_max_delay_seconds: u64,
    /// Enable jitter for retry delays
    pub retry_use_jitter: bool,
    /// Retention period for synced heartbeats in days
    pub retention_days: u32,
    /// Enable automatic background sync
    pub background_sync: bool,
}
 
impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_queue_size: 1000,
            batch_size: 50,
            sync_interval_seconds: 300, // 5 minutes
            max_retry_attempts: 5,
            retry_base_delay_seconds: 1,
            retry_max_delay_seconds: 60,
            retry_use_jitter: true,
            retention_days: 7,
            background_sync: true,
        }
    }
}

/// Trait for managing synchronization operations
#[async_trait::async_trait]
pub trait SyncManager {
    /// Sync all pending heartbeats
    async fn sync_pending(&self) -> Result<SyncResult, SyncError>;
    
    /// Sync a specific batch of heartbeats
    async fn sync_batch(&self, batch_size: usize) -> Result<SyncResult, SyncError>;
    
    /// Check if the system can connect to the API
    async fn check_connectivity(&self) -> Result<bool, SyncError>;
    
    /// Get current sync status and statistics
    async fn get_status(&self) -> Result<SyncStatusSummary, SyncError>;
    
    /// Force immediate sync regardless of connectivity status
    async fn force_sync(&self) -> Result<SyncResult, SyncError>;
}

/// Implementation of SyncManager that handles offline heartbeats synchronization
#[derive(Debug, Clone)]
pub struct ChronovaSyncManager {
    /// Configuration for sync operations
    pub config: SyncConfig,
    /// Retry strategy for failed sync attempts
    pub retry_strategy: RetryStrategy,
    /// API client for connectivity checks and sync operations
    pub api_client: ApiClient,
    /// Cached connectivity state (thread-safe)
    pub connectivity_state: Arc<AtomicBool>,
    /// Last connectivity check timestamp
    pub last_connectivity_check: Arc<RwLock<Option<SystemTime>>>,
    /// Performance metrics: total sync operations
    pub total_sync_operations: Arc<AtomicU64>,
    /// Performance metrics: successful sync operations
    pub successful_sync_operations: Arc<AtomicU64>,
    /// Performance metrics: failed sync operations
    pub failed_sync_operations: Arc<AtomicU64>,
    /// Performance metrics: total sync latency in milliseconds
    pub total_sync_latency_ms: Arc<AtomicU64>,
    /// Performance metrics: queue size monitoring
    pub last_queue_size: Arc<RwLock<Option<usize>>>,
}

impl ChronovaSyncManager {
    /// Create a new sync manager with default configuration
    pub fn new(api_client: ApiClient) -> Self {
        let config = SyncConfig::default();
        let retry_strategy = RetryStrategy {
            base_delay_seconds: config.retry_base_delay_seconds,
            max_attempts: config.max_retry_attempts,
            max_delay_seconds: config.retry_max_delay_seconds,
            use_jitter: config.retry_use_jitter,
        };
 
        Self {
            config,
            retry_strategy,
            api_client,
            connectivity_state: Arc::new(AtomicBool::new(false)), // Start as disconnected
            last_connectivity_check: Arc::new(RwLock::new(None)),
            total_sync_operations: Arc::new(AtomicU64::new(0)),
            successful_sync_operations: Arc::new(AtomicU64::new(0)),
            failed_sync_operations: Arc::new(AtomicU64::new(0)),
            total_sync_latency_ms: Arc::new(AtomicU64::new(0)),
            last_queue_size: Arc::new(RwLock::new(None)),
        }
    }
    
    /// Create a new sync manager with custom configuration
    pub fn with_config(config: SyncConfig, api_client: ApiClient) -> Self {
        let retry_strategy = RetryStrategy {
            base_delay_seconds: config.retry_base_delay_seconds,
            max_attempts: config.max_retry_attempts,
            max_delay_seconds: config.retry_max_delay_seconds,
            use_jitter: config.retry_use_jitter,
        };
 
        Self {
            config,
            retry_strategy,
            api_client,
            connectivity_state: Arc::new(AtomicBool::new(false)),
            last_connectivity_check: Arc::new(RwLock::new(None)),
            total_sync_operations: Arc::new(AtomicU64::new(0)),
            successful_sync_operations: Arc::new(AtomicU64::new(0)),
            failed_sync_operations: Arc::new(AtomicU64::new(0)),
            total_sync_latency_ms: Arc::new(AtomicU64::new(0)),
            last_queue_size: Arc::new(RwLock::new(None)),
        }
    }
    
    /// Create a new sync manager with custom configuration and retry strategy
    pub fn with_config_and_retry(config: SyncConfig, retry_strategy: RetryStrategy, api_client: ApiClient) -> Self {
        Self {
            config,
            retry_strategy,
            api_client,
            connectivity_state: Arc::new(AtomicBool::new(false)),
            last_connectivity_check: Arc::new(RwLock::new(None)),
            total_sync_operations: Arc::new(AtomicU64::new(0)),
            successful_sync_operations: Arc::new(AtomicU64::new(0)),
            failed_sync_operations: Arc::new(AtomicU64::new(0)),
            total_sync_latency_ms: Arc::new(AtomicU64::new(0)),
            last_queue_size: Arc::new(RwLock::new(None)),
        }
    }
    
    /// Start periodic connectivity monitoring
    pub async fn start_connectivity_monitoring(&self) -> Result<(), SyncError> {
        let connectivity_state = Arc::clone(&self.connectivity_state);
        let last_check = Arc::clone(&self.last_connectivity_check);
        let api_client = self.api_client.clone();
        
        tokio::spawn(async move {
            loop {
                // Check connectivity
                match api_client.check_connectivity().await {
                    Ok(is_connected) => {
                        connectivity_state.store(is_connected, Ordering::SeqCst);
                        
                        // Update last check timestamp
                        let mut last_check_guard = last_check.write().await;
                        *last_check_guard = Some(SystemTime::now());
                        
                        tracing::debug!("Connectivity monitoring: {}", if is_connected { "connected" } else { "disconnected" });
                    }
                    Err(e) => {
                        tracing::warn!("Connectivity monitoring failed: {}", e);
                        connectivity_state.store(false, Ordering::SeqCst);
                    }
                }
                
                // Wait for next check interval (default: 30 seconds)
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
        });
        
        Ok(())
    }
    
    /// Start background sync thread that automatically syncs pending heartbeats
    pub async fn start_background_sync(&self) -> Result<(), SyncError> {
        if !self.config.background_sync {
            tracing::info!("Background sync is disabled in configuration");
            return Ok(());
        }
        
        let sync_manager = self.clone();
        let sync_interval = Duration::from_secs(self.config.sync_interval_seconds);
        
        tokio::spawn(async move {
            tracing::info!("Starting background sync with interval: {} seconds", sync_interval.as_secs());
            
            loop {
                // Check if we're connected before attempting sync
                match sync_manager.check_connectivity().await {
                    Ok(is_connected) => {
                        if is_connected {
                            tracing::debug!("Network connected, attempting background sync");
                            
                            // Perform sync operation
                            match sync_manager.sync_pending().await {
                                Ok(result) => {
                                    if result.synced_count > 0 {
                                        tracing::info!("Background sync completed: {} heartbeats synced, {} failed",
                                            result.synced_count, result.failed_count);
                                    } else {
                                        tracing::debug!("Background sync: no heartbeats to sync");
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!("Background sync failed: {}", e);
                                }
                            }
                        } else {
                            tracing::debug!("Network disconnected, skipping background sync");
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Connectivity check failed for background sync: {}", e);
                    }
                }
                
                // Wait for next sync interval
                tokio::time::sleep(sync_interval).await;
            }
        });
        
        Ok(())
    }
    
    /// Perform a sync operation with automatic retry and error recovery
    pub async fn sync_with_retry(&self, operation: impl Fn() -> Result<SyncResult, SyncError> + Send + Sync) -> Result<SyncResult, SyncError> {
        let mut attempt = 0;
        let mut last_error: Option<SyncError> = None;
        
        while attempt <= self.retry_strategy.max_attempts {
            attempt += 1;
            
            match operation() {
                Ok(result) => {
                    // Success - return the result
                    return Ok(result);
                }
                Err(error) => {
                    last_error = Some(error.clone());
                    
                    // Check if this error is retryable
                    if !RetryStrategy::is_retryable_error(&error) {
                        tracing::warn!("Non-retryable error encountered: {}", error);
                        return Err(error);
                    }
                    
                    // Check if we should retry
                    if !self.retry_strategy.should_retry(attempt) {
                        tracing::warn!("Max retry attempts reached for error: {}", error);
                        return Err(error);
                    }
                    
                    // Calculate delay with exponential backoff
                    let delay = self.retry_strategy.calculate_delay(attempt);
                    tracing::info!("Sync operation failed (attempt {}), retrying in {} seconds: {}",
                        attempt, delay.as_secs(), error);
                    
                    // Wait before retry
                    tokio::time::sleep(delay).await;
                }
            }
        }
        
        // If we get here, all retry attempts failed
        Err(last_error.unwrap_or_else(|| SyncError::Unknown("All retry attempts failed".to_string())))
    }

    /// Start both connectivity monitoring and background sync
    pub async fn start_all_services(&self) -> Result<(), SyncError> {
        // Start connectivity monitoring
        self.start_connectivity_monitoring().await?;
        
        // Start background sync if enabled
        if self.config.background_sync {
            self.start_background_sync().await?;
        }
        
        tracing::info!("All sync services started successfully");
        Ok(())
    }
    
    /// Get the cached connectivity state
    pub fn get_cached_connectivity(&self) -> bool {
        self.connectivity_state.load(Ordering::SeqCst)
    }
    
    /// Get the time since last connectivity check
    pub async fn time_since_last_check(&self) -> Option<Duration> {
        let last_check_guard = self.last_connectivity_check.read().await;
        last_check_guard.map(|time| time.elapsed().unwrap_or(Duration::ZERO))
    }

    /// Record performance metrics for a sync operation
    pub fn record_sync_metrics(&self, result: &SyncResult) {
        // Increment total operations counter
        self.total_sync_operations.fetch_add(1, Ordering::Relaxed);

        // Update success/failure counters
        if result.error.is_none() && result.failed_count == 0 {
            self.successful_sync_operations.fetch_add(1, Ordering::Relaxed);
        } else {
            self.failed_sync_operations.fetch_add(1, Ordering::Relaxed);
        }

        // Record latency metrics
        if result.duration.as_millis() > 0 {
            self.total_sync_latency_ms.fetch_add(result.duration.as_millis() as u64, Ordering::Relaxed);
        }

        // Log detailed sync metrics
        tracing::info!(
            sync_id = %uuid::Uuid::new_v4().to_string(),
            synced_count = result.synced_count,
            failed_count = result.failed_count,
            total_count = result.total_count,
            duration_ms = result.duration.as_millis(),
            avg_latency_ms = result.avg_latency_ms.unwrap_or(0.0),
            total_operations = self.total_sync_operations.load(Ordering::Relaxed),
            successful_operations = self.successful_sync_operations.load(Ordering::Relaxed),
            failed_operations = self.failed_sync_operations.load(Ordering::Relaxed),
            "Sync operation completed"
        );
    }

    /// Get performance metrics for sync operations
    pub fn get_performance_metrics(&self) -> PerformanceMetrics {
        let total_ops = self.total_sync_operations.load(Ordering::Relaxed);
        let successful_ops = self.successful_sync_operations.load(Ordering::Relaxed);
        let failed_ops = self.failed_sync_operations.load(Ordering::Relaxed);
        let total_latency = self.total_sync_latency_ms.load(Ordering::Relaxed);

        let avg_latency_ms = if total_ops > 0 {
            total_latency as f64 / total_ops as f64
        } else {
            0.0
        };

        let success_rate = if total_ops > 0 {
            (successful_ops as f64 / total_ops as f64) * 100.0
        } else {
            0.0
        };

        PerformanceMetrics {
            total_operations: total_ops,
            successful_operations: successful_ops,
            failed_operations: failed_ops,
            average_latency_ms: avg_latency_ms,
            success_rate_percent: success_rate,
            total_latency_ms: total_latency,
        }
    }

    /// Update queue size monitoring
    pub async fn update_queue_size(&self, queue_size: usize) {
        let mut last_size_guard = self.last_queue_size.write().await;
        *last_size_guard = Some(queue_size);

        tracing::debug!(
            queue_size = queue_size,
            max_queue_size = self.config.max_queue_size,
            queue_utilization_percent = (queue_size as f64 / self.config.max_queue_size as f64) * 100.0,
            "Queue size updated"
        );

        // Log warning if queue is approaching capacity
        let utilization = queue_size as f64 / self.config.max_queue_size as f64;
        if utilization > 0.8 {
            tracing::warn!(
                queue_size = queue_size,
                max_queue_size = self.config.max_queue_size,
                utilization_percent = utilization * 100.0,
                "Queue approaching capacity"
            );
        }
    }

    /// Get the last recorded queue size
    pub async fn get_last_queue_size(&self) -> Option<usize> {
        let last_size_guard = self.last_queue_size.read().await;
        *last_size_guard
    }

    /// Calculate and log sync latency metrics
    pub fn calculate_latency_metrics(&self, start_time: Instant, end_time: Instant, count: usize) -> f64 {
        let duration = end_time.duration_since(start_time);
        let avg_latency_ms = if count > 0 {
            duration.as_millis() as f64 / count as f64
        } else {
            0.0
        };

        tracing::debug!(
            total_duration_ms = duration.as_millis(),
            heartbeat_count = count,
            avg_latency_per_heartbeat_ms = avg_latency_ms,
            "Sync latency calculated"
        );

        avg_latency_ms
    }

    /// Log structured sync operation start
    pub fn log_sync_start(&self, operation_type: &str, batch_size: Option<usize>) -> Instant {
        let start_time = Instant::now();
        
        tracing::info!(
            operation_type = operation_type,
            batch_size = batch_size.unwrap_or(0),
            start_time = ?SystemTime::now(),
            "Sync operation started"
        );

        start_time
    }

    /// Log structured sync operation completion
    pub fn log_sync_completion(&self, operation_type: &str, result: &SyncResult, start_time: Instant) {
        let end_time = Instant::now();
        let duration = end_time.duration_since(start_time);

        tracing::info!(
            operation_type = operation_type,
            synced_count = result.synced_count,
            failed_count = result.failed_count,
            total_count = result.total_count,
            duration_ms = duration.as_millis(),
            avg_latency_ms = result.avg_latency_ms.unwrap_or(0.0),
            success = result.error.is_none() && result.failed_count == 0,
            "Sync operation completed"
        );
    }
}

/// Performance metrics for sync operations
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    /// Total number of sync operations performed
    pub total_operations: u64,
    /// Number of successful sync operations
    pub successful_operations: u64,
    /// Number of failed sync operations
    pub failed_operations: u64,
    /// Average latency per sync operation in milliseconds
    pub average_latency_ms: f64,
    /// Success rate as a percentage
    pub success_rate_percent: f64,
    /// Total latency across all operations in milliseconds
    pub total_latency_ms: u64,
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            total_operations: 0,
            successful_operations: 0,
            failed_operations: 0,
            average_latency_ms: 0.0,
            success_rate_percent: 0.0,
            total_latency_ms: 0,
        }
    }
}

impl Default for ChronovaSyncManager {
    fn default() -> Self {
        // Create a default API client with a dummy URL - this will be replaced in actual usage
        let api_client = ApiClient::new("http://localhost:8080".to_string());
        Self::new(api_client)
    }
}

#[async_trait::async_trait]
impl SyncManager for ChronovaSyncManager {
    async fn sync_pending(&self) -> Result<SyncResult, SyncError> {
        use crate::heartbeat::Heartbeat;
        use crate::queue::Queue;

        let start = self.log_sync_start("sync_pending", None);
        let mut sync_result = SyncResult::default();
        sync_result.start_time = Some(SystemTime::now());

        // Choose a reasonable batch size for each network call (configurable)
        let batch_size = std::cmp::min(self.config.batch_size, self.config.max_queue_size);

        loop {
            // Fetch a batch of pending heartbeats from the on-disk queue inside a blocking thread
            let pending_res = tokio::task::spawn_blocking({
                let batch_size = batch_size;
                move || -> Result<Vec<Heartbeat>, SyncError> {
                    let queue = Queue::new().map_err(|e| SyncError::Database(format!("{}", e)))?;
                    let hbs = queue.get_pending(Some(batch_size), Some(SyncStatus::Pending))
                        .map_err(|e| SyncError::Database(format!("{}", e)))?;
                    Ok(hbs)
                }
            }).await.map_err(|e| SyncError::Unknown(format!("Join error: {}", e)))??;

            if pending_res.is_empty() {
                // Nothing left to sync
                break;
            }

            // Attempt to push the batch to the server
            sync_result.total_count += pending_res.len();
            let batch_start = Instant::now();

            match self.api_client.send_heartbeats_batch(&pending_res).await {
                Ok(_response) => {
                    // Mark and remove all entries in a single blocking operation to avoid
                    // repeated DB opens and visibility issues.
                    let ids: Vec<String> = pending_res.iter().map(|hb| hb.id.clone()).collect();
                    let _ = tokio::task::spawn_blocking(move || -> Result<(), SyncError> {
                        let q = Queue::new().map_err(|e| SyncError::Database(format!("{}", e)))?;
                        for id in ids {
                            q.update_sync_status(&id, SyncStatus::Synced, Some("synced".to_string()))
                                .map_err(|e| SyncError::Database(format!("{}", e)))?;
                            q.remove(&id).map_err(|e| SyncError::Database(format!("{}", e)))?;
                        }
                        Ok(())
                    }).await.map_err(|e| SyncError::Unknown(format!("Join error: {}", e)))??;

                    sync_result.synced_count += pending_res.len();
                }
                Err(api_err) => {
                    // Map ApiError to SyncError for metrics/logging
                    let mapped = match api_err {
                        crate::api::ApiError::Auth(msg) => SyncError::Auth(msg.to_string()),
                        crate::api::ApiError::RateLimit(msg) => SyncError::RateLimit(msg.to_string()),
                        crate::api::ApiError::Network(err) => SyncError::Network(format!("{}", err)),
                        crate::api::ApiError::Api(a, b) => SyncError::Network(format!("{}: {}", a, b)),
                    };

                    tracing::warn!(
                        "Batch sync failed with error: {}. Processing per-heartbeat retry logic.",
                        mapped
                    );

                    // Consolidate per-heartbeat retry handling into a single blocking operation
                    // to avoid multiple DB opens and improve atomicity.
                    let ids: Vec<String> = pending_res.iter().map(|hb| hb.id.clone()).collect();
                    let err_meta = format!("{}", mapped);
                    let max_attempts = self.retry_strategy.max_attempts;
    
                    let _ = tokio::task::spawn_blocking(move || -> Result<(), SyncError> {
                        let q = Queue::new().map_err(|e| SyncError::Database(format!("{}", e)))?;
                        for id in ids {
                            q.increment_retry(&id).map_err(|e| SyncError::Database(format!("{}", e)))?;
                            let rc = q.get_retry_count(&id).unwrap_or(0);
                            if rc >= max_attempts {
                                q.update_sync_status(&id, SyncStatus::PermanentFailure, Some(err_meta.clone()))
                                    .map_err(|e| SyncError::Database(format!("{}", e)))?;
                            } else {
                                q.update_sync_status(&id, SyncStatus::Failed, Some(err_meta.clone()))
                                    .map_err(|e| SyncError::Database(format!("{}", e)))?;
                            }
                        }
                        Ok(())
                    }).await.map_err(|e| SyncError::Unknown(format!("Join error: {}", e)))??;
    
                    sync_result.failed_count += pending_res.len();
                    // Do not abort the entire sync cycle; continue with next batches
                }
            }

            // Record batch latency
            let batch_end = Instant::now();
            let avg_latency = self.calculate_latency_metrics(batch_start, batch_end, pending_res.len());
            sync_result.avg_latency_ms = match sync_result.avg_latency_ms {
                Some(prev) => Some((prev + avg_latency) / 2.0),
                None => Some(avg_latency),
            };
        }

        let end = Instant::now();
        sync_result.duration = end.duration_since(start);
        sync_result.end_time = Some(SystemTime::now());

        self.log_sync_completion("sync_pending", &sync_result, start);
        self.record_sync_metrics(&sync_result);

        Ok(sync_result)
    }
    
    async fn sync_batch(&self, batch_size: usize) -> Result<SyncResult, SyncError> {
        use crate::heartbeat::Heartbeat;
        use crate::queue::Queue;

        let start = self.log_sync_start("sync_batch", Some(batch_size));
        let mut result = SyncResult::default();
        result.start_time = Some(SystemTime::now());

        // Fetch up to batch_size pending heartbeats
        let pending = tokio::task::spawn_blocking({
            let batch_size = batch_size;
            move || -> Result<Vec<Heartbeat>, SyncError> {
                let queue = Queue::new().map_err(|e| SyncError::Database(format!("{}", e)))?;
                let hbs = queue.get_pending(Some(batch_size), Some(SyncStatus::Pending))
                    .map_err(|e| SyncError::Database(format!("{}", e)))?;
                Ok(hbs)
            }
        }).await.map_err(|e| SyncError::Unknown(format!("Join error: {}", e)))??;

        if pending.is_empty() {
            // Nothing to do
            result.end_time = Some(SystemTime::now());
            result.duration = Instant::now().duration_since(start);
            self.log_sync_completion("sync_batch", &result, start);
            return Ok(result);
        }

        result.total_count = pending.len();

        match self.api_client.send_heartbeats_batch(&pending).await {
            Ok(_) => {
                // Mark and remove all entries in a single blocking operation
                let ids: Vec<String> = pending.iter().map(|hb| hb.id.clone()).collect();
                let _ = tokio::task::spawn_blocking(move || -> Result<(), SyncError> {
                    let q = Queue::new().map_err(|e| SyncError::Database(format!("{}", e)))?;
                    for id in ids {
                        q.update_sync_status(&id, SyncStatus::Synced, Some("synced".to_string()))
                            .map_err(|e| SyncError::Database(format!("{}", e)))?;
                        q.remove(&id).map_err(|e| SyncError::Database(format!("{}", e)))?;
                    }
                    Ok(())
                }).await.map_err(|e| SyncError::Unknown(format!("Join error: {}", e)))??;
                result.synced_count = pending.len();
            }
            Err(api_err) => {
                let mapped = match api_err {
                    crate::api::ApiError::Auth(msg) => SyncError::Auth(msg.to_string()),
                    crate::api::ApiError::RateLimit(msg) => SyncError::RateLimit(msg.to_string()),
                    crate::api::ApiError::Network(err) => SyncError::Network(format!("{}", err)),
                    crate::api::ApiError::Api(a, b) => SyncError::Network(format!("{}: {}", a, b)),
                };

                // Consolidate retry updates into one blocking operation
                let ids: Vec<String> = pending.iter().map(|hb| hb.id.clone()).collect();
                let err_meta = format!("{}", mapped);
                let max_attempts = self.retry_strategy.max_attempts;
    
                let _ = tokio::task::spawn_blocking(move || -> Result<(), SyncError> {
                    let q = Queue::new().map_err(|e| SyncError::Database(format!("{}", e)))?;
                    for id in ids {
                        q.increment_retry(&id).map_err(|e| SyncError::Database(format!("{}", e)))?;
                        let rc = q.get_retry_count(&id).unwrap_or(0);
                        if rc >= max_attempts {
                            q.update_sync_status(&id, SyncStatus::PermanentFailure, Some(err_meta.clone()))
                                .map_err(|e| SyncError::Database(format!("{}", e)))?;
                        } else {
                            q.update_sync_status(&id, SyncStatus::Failed, Some(err_meta.clone()))
                                .map_err(|e| SyncError::Database(format!("{}", e)))?;
                        }
                    }
                    Ok(())
                }).await.map_err(|e| SyncError::Unknown(format!("Join error: {}", e)))??;

                result.failed_count = pending.len();
                result.error = Some(mapped);
            }
        }

        let end = Instant::now();
        result.duration = end.duration_since(start);
        result.end_time = Some(SystemTime::now());

        self.log_sync_completion("sync_batch", &result, start);
        self.record_sync_metrics(&result);

        Ok(result)
    }
    
    async fn check_connectivity(&self) -> Result<bool, SyncError> {
        // First check if we have a recent cached connectivity state
        let time_since_last_check = self.time_since_last_check().await;
        
        if let Some(duration) = time_since_last_check {
            if duration < Duration::from_secs(30) {
                // Use cached state if recent enough
                return Ok(self.get_cached_connectivity());
            }
        }
        
        // If no recent cache or cache is stale, perform a fresh check
        let result = self.api_client.check_connectivity().await
            .map_err(|e| SyncError::Network(format!("Connectivity check failed: {}", e)));
        
        // Update cache with fresh result
        if let Ok(is_connected) = &result {
            self.connectivity_state.store(*is_connected, Ordering::SeqCst);
            
            let mut last_check_guard = self.last_connectivity_check.write().await;
            *last_check_guard = Some(SystemTime::now());
        }
        
        result
    }
    
    async fn get_status(&self) -> Result<SyncStatusSummary, SyncError> {
        // TODO: Implement status retrieval in Phase 8 when queue integration is available
        Ok(SyncStatusSummary::default())
    }
    
    async fn force_sync(&self) -> Result<SyncResult, SyncError> {
        // TODO: Implement force sync logic in Phase 8 when queue integration is available
        self.sync_pending().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_status_from_str() {
        assert_eq!(SyncStatus::from("pending"), SyncStatus::Pending);
        assert_eq!(SyncStatus::from("syncing"), SyncStatus::Syncing);
        assert_eq!(SyncStatus::from("synced"), SyncStatus::Synced);
        assert_eq!(SyncStatus::from("failed"), SyncStatus::Failed);
        assert_eq!(SyncStatus::from("permanent_failure"), SyncStatus::PermanentFailure);
        assert_eq!(SyncStatus::from("unknown"), SyncStatus::Pending); // Default case
    }

    #[test]
    fn test_sync_status_to_string() {
        assert_eq!(String::from(SyncStatus::Pending), "pending");
        assert_eq!(String::from(SyncStatus::Syncing), "syncing");
        assert_eq!(String::from(SyncStatus::Synced), "synced");
        assert_eq!(String::from(SyncStatus::Failed), "failed");
        assert_eq!(String::from(SyncStatus::PermanentFailure), "permanent_failure");
    }

    #[test]
    fn test_sync_status_default() {
        assert_eq!(SyncStatus::default(), SyncStatus::Pending);
    }

    #[test]
    fn test_sync_result_default() {
        let result = SyncResult::default();
        assert_eq!(result.synced_count, 0);
        assert_eq!(result.failed_count, 0);
        assert_eq!(result.total_count, 0);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_sync_status_summary_default() {
        let summary = SyncStatusSummary::default();
        assert_eq!(summary.pending, 0);
        assert_eq!(summary.syncing, 0);
        assert_eq!(summary.synced, 0);
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.permanent_failures, 0);
        assert_eq!(summary.total, 0);
        assert!(summary.last_sync.is_none());
    }

    #[test]
    fn test_sync_error_variants() {
        // Test Network error
        let network_error = SyncError::Network("connection failed".to_string());
        assert!(network_error.to_string().contains("Network error: connection failed"));
        
        // Test Auth error
        let auth_error = SyncError::Auth("invalid credentials".to_string());
        assert!(auth_error.to_string().contains("Authentication error: invalid credentials"));
        
        // Test RateLimit error
        let rate_limit_error = SyncError::RateLimit("too many requests".to_string());
        assert!(rate_limit_error.to_string().contains("Rate limit exceeded: too many requests"));
        
        // Test Database error
        let db_error = SyncError::Database("query failed".to_string());
        assert!(db_error.to_string().contains("Database error: query failed"));
        
        // Test Serialization error
        let serde_error = SyncError::Serialization("invalid json".to_string());
        assert!(serde_error.to_string().contains("Serialization error: invalid json"));
        
        // Test Config error
        let config_error = SyncError::Config("missing api key".to_string());
        assert!(config_error.to_string().contains("Invalid configuration: missing api key"));
        
        // Test Unknown error
        let unknown_error = SyncError::Unknown("something went wrong".to_string());
        assert!(unknown_error.to_string().contains("Unknown error: something went wrong"));
    }

    #[test]
    fn test_sync_error_clone() {
        let sync_error = SyncError::Network("test".to_string());
        let cloned_sync_error = sync_error.clone();
        assert_eq!(sync_error.to_string(), cloned_sync_error.to_string());
    }

    #[test]
    fn test_sync_error_debug() {
        let sync_error = SyncError::Network("test".to_string());
        let debug_output = format!("{:?}", sync_error);
        assert!(debug_output.contains("Network"));
    }

    #[tokio::test]
    async fn test_connectivity_check_success() {
        use wiremock::{Mock, MockServer, ResponseTemplate};
        use wiremock::matchers::method;
        
        let mock_server = MockServer::start().await;
        
        // Mock a successful response for connectivity check
        Mock::given(method("HEAD"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let api_client = ApiClient::new(mock_server.uri());
        let sync_manager = ChronovaSyncManager::new(api_client);
        
        let result = sync_manager.check_connectivity().await;
        assert!(result.is_ok(), "Connectivity check should succeed");
        assert!(result.unwrap(), "Should be connected");
    }

    #[tokio::test]
    async fn test_connectivity_check_failure() {
        // Use an invalid URL to simulate network failure
        let api_client = ApiClient::new("http://invalid-url-that-does-not-exist.local".to_string());
        let sync_manager = ChronovaSyncManager::new(api_client);
        
        let result = sync_manager.check_connectivity().await;
        assert!(result.is_ok(), "Connectivity check should return Ok even on failure");
        assert!(!result.unwrap(), "Should not be connected");
    }

    #[tokio::test]
    async fn test_connectivity_caching() {
        use wiremock::{Mock, MockServer, ResponseTemplate};
        use wiremock::matchers::method;
        
        let mock_server = MockServer::start().await;
        
        Mock::given(method("HEAD"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let api_client = ApiClient::new(mock_server.uri());
        let sync_manager = ChronovaSyncManager::new(api_client);
        
        // First check should update cache
        let result1 = sync_manager.check_connectivity().await;
        assert!(result1.is_ok());
        assert!(result1.unwrap());
        
        // Second check should use cached value
        let result2 = sync_manager.check_connectivity().await;
        assert!(result2.is_ok());
        assert!(result2.unwrap());
        
        // Verify cached state is accessible
        assert!(sync_manager.get_cached_connectivity(), "Cached state should be true");
    }

    #[tokio::test]
    async fn test_connectivity_monitoring_start() {
        use wiremock::{Mock, MockServer, ResponseTemplate};
        use wiremock::matchers::method;
        
        let mock_server = MockServer::start().await;
        
        Mock::given(method("HEAD"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let api_client = ApiClient::new(mock_server.uri());
        let sync_manager = ChronovaSyncManager::new(api_client);
        
        // Start connectivity monitoring
        let result = sync_manager.start_connectivity_monitoring().await;
        assert!(result.is_ok(), "Should start monitoring successfully");
        
        // Give it a moment to run the first check
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        
        // Should have updated the cache
        assert!(sync_manager.get_cached_connectivity(), "Monitoring should update cache");
        
        // Should have a recent check timestamp
        let time_since_check = sync_manager.time_since_last_check().await;
        assert!(time_since_check.is_some(), "Should have a check timestamp");
        assert!(time_since_check.unwrap() < std::time::Duration::from_secs(1), "Check should be recent");
    }

    #[test]
    fn test_cached_connectivity_default() {
        let api_client = ApiClient::new("http://localhost:8080".to_string());
        let sync_manager = ChronovaSyncManager::new(api_client);
        
        // Default should be false (disconnected)
        assert!(!sync_manager.get_cached_connectivity(), "Default cache should be false");
    }

    #[tokio::test]
    async fn test_time_since_last_check_none() {
        let api_client = ApiClient::new("http://localhost:8080".to_string());
        let sync_manager = ChronovaSyncManager::new(api_client);
        
        // Initially should be None
        let time_since_check = sync_manager.time_since_last_check().await;
        assert!(time_since_check.is_none(), "Initially no check timestamp");
    }

    #[test]
    fn test_retry_strategy_default() {
        let strategy = RetryStrategy::default();
        assert_eq!(strategy.base_delay_seconds, 1);
        assert_eq!(strategy.max_attempts, 5);
        assert_eq!(strategy.max_delay_seconds, 60);
        assert!(strategy.use_jitter);
    }

    #[test]
    fn test_calculate_delay_without_jitter() {
        let strategy = RetryStrategy {
            base_delay_seconds: 1,
            max_attempts: 5,
            max_delay_seconds: 60,
            use_jitter: false,
        };

        // Test attempt 0 (should be 0 seconds)
        assert_eq!(strategy.calculate_delay(0).as_secs(), 0);
        
        // Test exponential backoff
        assert_eq!(strategy.calculate_delay(1).as_secs(), 1);  // 1 * 2^0
        assert_eq!(strategy.calculate_delay(2).as_secs(), 2);  // 1 * 2^1
        assert_eq!(strategy.calculate_delay(3).as_secs(), 4);  // 1 * 2^2
        assert_eq!(strategy.calculate_delay(4).as_secs(), 8);  // 1 * 2^3
        assert_eq!(strategy.calculate_delay(5).as_secs(), 16); // 1 * 2^4
        
        // Test max delay cap
        assert_eq!(strategy.calculate_delay(10).as_secs(), 60); // Capped at max_delay_seconds
    }

    #[test]
    fn test_calculate_delay_with_jitter() {
        let strategy = RetryStrategy {
            base_delay_seconds: 1,
            max_attempts: 5,
            max_delay_seconds: 60,
            use_jitter: true,
        };

        // Test that jitter produces values within expected range
        for attempt in 1..=5 {
            let delay = strategy.calculate_delay(attempt).as_secs();
            let base_delay = 2u64.pow(attempt - 1);
            let min_delay = (base_delay as f64 * 0.5) as u64;
            let max_delay = (base_delay as f64 * 1.5) as u64;
            
            assert!(delay >= min_delay, "Delay {} should be >= {}", delay, min_delay);
            assert!(delay <= max_delay, "Delay {} should be <= {}", delay, max_delay);
        }
    }

    #[test]
    fn test_should_retry() {
        let strategy = RetryStrategy {
            base_delay_seconds: 1,
            max_attempts: 3,
            max_delay_seconds: 60,
            use_jitter: false,
        };

        assert!(strategy.should_retry(0));
        assert!(strategy.should_retry(1));
        assert!(strategy.should_retry(2));
        assert!(!strategy.should_retry(3)); // max_attempts is 3, so attempt 3 should not retry
        assert!(!strategy.should_retry(4));
    }

    #[test]
    fn test_is_retryable_error() {
        // Test retryable errors
        assert!(RetryStrategy::is_retryable_error(&SyncError::Network("test".to_string())));
        assert!(RetryStrategy::is_retryable_error(&SyncError::RateLimit("test".to_string())));
        assert!(RetryStrategy::is_retryable_error(&SyncError::Database("test".to_string())));
        assert!(RetryStrategy::is_retryable_error(&SyncError::Serialization("test".to_string())));
        assert!(RetryStrategy::is_retryable_error(&SyncError::Unknown("test".to_string())));

        // Test non-retryable errors
        assert!(!RetryStrategy::is_retryable_error(&SyncError::Auth("test".to_string())));
        assert!(!RetryStrategy::is_retryable_error(&SyncError::Config("test".to_string())));
    }

    #[test]
    fn test_custom_retry_strategy() {
        let strategy = RetryStrategy {
            base_delay_seconds: 5,
            max_attempts: 10,
            max_delay_seconds: 30,
            use_jitter: false,
        };

        assert_eq!(strategy.base_delay_seconds, 5);
        assert_eq!(strategy.max_attempts, 10);
        assert_eq!(strategy.max_delay_seconds, 30);
        assert!(!strategy.use_jitter);

        // Test custom exponential backoff
        assert_eq!(strategy.calculate_delay(1).as_secs(), 5);  // 5 * 2^0
        assert_eq!(strategy.calculate_delay(2).as_secs(), 10); // 5 * 2^1
        assert_eq!(strategy.calculate_delay(3).as_secs(), 20); // 5 * 2^2
        assert_eq!(strategy.calculate_delay(4).as_secs(), 30); // 5 * 2^3 = 40, but capped at 30
    }

    #[test]
    fn test_retry_strategy_clone() {
        let strategy1 = RetryStrategy {
            base_delay_seconds: 2,
            max_attempts: 7,
            max_delay_seconds: 50,
            use_jitter: true,
        };

        let strategy2 = strategy1.clone();
        
        assert_eq!(strategy1.base_delay_seconds, strategy2.base_delay_seconds);
        assert_eq!(strategy1.max_attempts, strategy2.max_attempts);
        assert_eq!(strategy1.max_delay_seconds, strategy2.max_delay_seconds);
        assert_eq!(strategy1.use_jitter, strategy2.use_jitter);
    }

    #[test]
    fn test_retry_strategy_debug() {
        let strategy = RetryStrategy::default();
        let debug_output = format!("{:?}", strategy);
        
        assert!(debug_output.contains("base_delay_seconds"));
        assert!(debug_output.contains("max_attempts"));
        assert!(debug_output.contains("max_delay_seconds"));
        assert!(debug_output.contains("use_jitter"));
    }

    #[tokio::test]
    async fn test_background_sync_start_disabled() {
        let api_client = ApiClient::new("http://localhost:8080".to_string());
        let mut config = SyncConfig::default();
        config.background_sync = false;
        let sync_manager = ChronovaSyncManager::with_config(config, api_client);
        
        let result = sync_manager.start_background_sync().await;
        assert!(result.is_ok(), "Should start successfully even when disabled");
    }

    #[tokio::test]
    async fn test_background_sync_start_enabled() {
        use wiremock::{Mock, MockServer, ResponseTemplate};
        use wiremock::matchers::method;
        
        let mock_server = MockServer::start().await;
        
        Mock::given(method("HEAD"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let api_client = ApiClient::new(mock_server.uri());
        let sync_manager = ChronovaSyncManager::new(api_client);
        
        let result = sync_manager.start_background_sync().await;
        assert!(result.is_ok(), "Should start background sync successfully");
        
        // Give it a moment to start
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    #[tokio::test]
    async fn test_start_all_services() {
        use wiremock::{Mock, MockServer, ResponseTemplate};
        use wiremock::matchers::method;
        
        let mock_server = MockServer::start().await;
        
        Mock::given(method("HEAD"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let api_client = ApiClient::new(mock_server.uri());
        let sync_manager = ChronovaSyncManager::new(api_client);
        
        let result = sync_manager.start_all_services().await;
        assert!(result.is_ok(), "Should start all services successfully");
        
        // Give services a moment to start
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        
        // Verify connectivity monitoring is running
        assert!(sync_manager.get_cached_connectivity(), "Connectivity monitoring should update cache");
    }

    #[tokio::test]
    async fn test_sync_interval_configuration() {
        let api_client = ApiClient::new("http://localhost:8080".to_string());
        let mut config = SyncConfig::default();
        config.sync_interval_seconds = 60; // 1 minute
        let sync_manager = ChronovaSyncManager::with_config(config, api_client);
        
        // Verify the configuration is properly set
        assert_eq!(sync_manager.config.sync_interval_seconds, 60);
    }
}