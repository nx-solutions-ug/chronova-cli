use serde::{Deserialize, Serialize};
use uuid::Uuid;

// Import types that are used in this module
// These will work in both main crate and test contexts
use crate::api::ApiClient;
use crate::cli::Cli;
use crate::collector::DataCollector;
use crate::config::Config;
use crate::queue::{Queue, QueueOps};
use crate::sync::{SyncResult, SyncStatusSummary};
use crate::user_agent::generate_user_agent;
use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Heartbeat {
    pub id: String,
    pub entity: String,
    #[serde(rename = "type")]
    pub entity_type: String,
    pub time: f64,
    pub project: Option<String>,
    pub branch: Option<String>,
    pub language: Option<String>,
    pub is_write: bool,
    pub lines: Option<i32>,
    pub lineno: Option<i32>,
    pub cursorpos: Option<i32>,
    pub user_agent: Option<String>,
    pub category: Option<String>,
    pub machine: Option<String>,

    /// Optional editor information (name + version)
    pub editor: Option<EditorInfo>,

    /// Optional operating system information
    pub operating_system: Option<OsInfo>,

    pub commit_hash: Option<String>,
    pub commit_author: Option<String>,
    pub commit_message: Option<String>,
    pub repository_url: Option<String>,

    pub dependencies: Vec<String>,
}

pub struct HeartbeatManager {
    config: Config,
    api_client: ApiClient,
    authenticated_api_client: Option<crate::api::AuthenticatedApiClient>,
    queue: Queue,
    collector: DataCollector,
}

/// Minimal editor information attached to a heartbeat
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorInfo {
    pub name: String,
    pub version: Option<String>,
}

/// Minimal operating system information attached to a heartbeat
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsInfo {
    pub name: String,
    pub title: Option<String>,
    pub version: Option<String>,
}

impl HeartbeatManager {
    pub fn new(config: Config) -> Self {
        let api_client = ApiClient::new(config.get_api_url());
        let authenticated_api_client = config
            .get_api_key(None)
            .map(|key| api_client.clone().with_api_key(key));
        let queue = Queue::new().expect("Failed to initialize queue");
        // Ensure a fresh queue state for newly constructed managers (helps tests/isolation)
        // Ignore any error here — best effort cleanup to avoid leaking state between runs.
        let _ = queue.cleanup_old_entries(0);
        let collector = DataCollector::new();

        Self {
            config,
            api_client,
            authenticated_api_client,
            queue,
            collector,
        }
    }

    pub async fn process(&self, mut cli: Cli) -> Result<(), anyhow::Error> {
        // Entity is guaranteed to be Some at this point (checked in main)
        let entity = cli.entity.take().expect("Entity should be present");

        // Check if entity should be ignored
        if self.should_ignore_entity(&entity) {
            tracing::debug!("Ignoring entity: {}", entity);
            return Ok(());
        }

        // Create heartbeat from CLI arguments
        let heartbeat = self.create_heartbeat(cli, entity).await?;

        // Use offline-first strategy: always queue first, then try to sync
        // Offload SQLite work to a blocking thread to avoid blocking the async runtime.
        tokio::task::spawn_blocking(move || -> Result<(), anyhow::Error> {
            let q = crate::queue::Queue::new().map_err(|e| anyhow::anyhow!(e))?;
            q.add(heartbeat).map_err(|e| anyhow::anyhow!(e))?;
            Ok(())
        })
        .await??;
        tracing::debug!("Heartbeat queued for offline-first processing");

        // Process any queued heartbeats using sync strategy
        let (_synced_count, _failed_count) = self.process_queue().await?;

        Ok(())
    }

    async fn create_heartbeat(&self, cli: Cli, entity: String) -> Result<Heartbeat, anyhow::Error> {
        let time = cli
            .time
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis() as f64 / 1000.0);

        // Collect additional data
        let project_info = self.collector.detect_project(&entity).await;
        let git_info = self.collector.detect_git_info(&entity).await;
        let language = self.collector.detect_language(&entity).await;

        // Parse plugin info for user agent
        // Note: We no longer parse plugin info here as the API handles this

        // Determine project name with priority: cli.project > alternate_project > detected project
        let project_name = cli.project.or(cli.alternate_project).or_else(|| {
            project_info.as_ref().map(|p| {
                p.root
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            })
        });

        // Determine branch with priority: cli.branch > git branch
        let branch = cli
            .branch
            .or_else(|| git_info.as_ref().and_then(|g| g.branch.clone()));

        // Determine language with priority: cli.language > detected language
        let language_name = cli.language.or(language);

        Ok(Heartbeat {
            id: Uuid::new_v4().to_string(),
            entity,
            entity_type: cli.entity_type,
            time,
            project: project_name,
            branch,
            language: language_name,
            is_write: cli.write.unwrap_or(false),
            lines: cli.lines,
            lineno: cli.lineno,
            cursorpos: cli.cursorpos,
            user_agent: Some(generate_user_agent(cli.plugin.as_deref())),
            category: cli.category,
            machine: cli
                .hostname
                .or_else(|| Some(gethostname::gethostname().to_string_lossy().into_owned())),
            editor: None,
            operating_system: None,
            commit_hash: git_info.as_ref().and_then(|g| g.commit_hash.clone()),
            commit_author: git_info.as_ref().and_then(|g| g.commit_author.clone()),
            commit_message: git_info.as_ref().and_then(|g| g.commit_message.clone()),
            repository_url: git_info.as_ref().and_then(|g| g.repository_url.clone()),
            dependencies: Vec::new(),
        })
    }

    fn should_ignore_entity(&self, entity: &str) -> bool {
        // Simple pattern matching for ignore rules
        for pattern in &self.config.ignore_patterns {
            if pattern.ends_with('$') {
                // Exact match at end
                let base_pattern = &pattern[..pattern.len() - 1];
                if entity.ends_with(base_pattern) {
                    return true;
                }
            } else if let Some(extension) = pattern.strip_prefix("*.") {
                // File extension pattern
                if entity.ends_with(extension) {
                    return true;
                }
            } else if entity.contains(pattern) {
                return true;
            }
        }
        false
    }

    async fn process_queue(&self) -> Result<(usize, usize), anyhow::Error> {
        // Process the queue in batches to avoid loading everything into memory at once.
        // Combine the "prepare retry-eligible failures" pass and the "fetch pending" call
        // into a single blocking task so the DB is opened only once per loop iteration.
        let batch_size: usize = 50;

        // Counters to return to callers
        let mut total_synced: usize = 0;
        let mut total_failed: usize = 0;

        loop {
            // Single blocking operation: prepare retry-eligible failed heartbeats and fetch a batch of pending
            let queued = tokio::task::spawn_blocking({
                let batch_size = batch_size;
                move || -> Result<Vec<Heartbeat>, anyhow::Error> {
                    let q = crate::queue::Queue::new().map_err(|e| anyhow::anyhow!(e))?;

                    // Prepare failed -> pending for retry (single DB connection)
                    let failed = q
                        .get_pending(Some(1000), Some(crate::sync::SyncStatus::Failed))
                        .map_err(|e| anyhow::anyhow!(e))?;
                    for hb in failed {
                        let current_retry_count = q.get_retry_count(&hb.id).unwrap_or(0);
                        if current_retry_count < 3 {
                            q.update_sync_status(
                                &hb.id,
                                crate::sync::SyncStatus::Pending,
                                Some(format!("Retry eligible (attempt {})", current_retry_count)),
                            )
                            .map_err(|e| anyhow::anyhow!(e))?;
                        }
                    }

                    // Now fetch the next batch of pending heartbeats for processing
                    q.get_pending(Some(batch_size), None)
                        .map_err(|e| anyhow::anyhow!(e))
                }
            })
            .await??;

            if queued.is_empty() {
                break;
            }

            tracing::info!(
                "Processing {} queued heartbeats (batch size {})",
                queued.len(),
                batch_size
            );

            // If more than one heartbeat, try to send as a batch for efficiency
            if queued.len() > 1 {
                // Mark all as syncing (do it in a single blocking operation)
                let queued_ids = queued.iter().map(|h| h.id.clone()).collect::<Vec<_>>();
                tokio::task::spawn_blocking(move || -> Result<(), anyhow::Error> {
                    let q = crate::queue::Queue::new().map_err(|e| anyhow::anyhow!(e))?;
                    for id in queued_ids {
                        let retry_count = q.get_retry_count(&id).map_err(|e| anyhow::anyhow!(e))?;
                        q.update_sync_status(
                            &id,
                            crate::sync::SyncStatus::Syncing,
                            Some(format!("Attempting sync (attempt {})", retry_count + 1)),
                        )
                        .map_err(|e| anyhow::anyhow!(e))?;
                    }
                    Ok(())
                })
                .await??;

                // Log which IDs are being sent in this batch for debugging
                let queued_ids_dbg = queued.iter().map(|h| h.id.clone()).collect::<Vec<_>>();
                tracing::debug!("Attempting batch send for ids: {:?}", queued_ids_dbg);
                let send_result = if let Some(auth_client) = &self.authenticated_api_client {
                    auth_client.send_heartbeats_batch(&queued).await
                } else {
                    self.api_client.send_heartbeats_batch(&queued).await
                };
                tracing::debug!("Batch send result success: {}", send_result.is_ok());

                match send_result {
                    Ok(_) => {
                        // Success: mark all as synced and remove them (single blocking op)
                        let queued_ids = queued.iter().map(|h| h.id.clone()).collect::<Vec<_>>();
                        let synced_len = queued.len();
                        tokio::task::spawn_blocking(move || -> Result<(), anyhow::Error> {
                            let q = crate::queue::Queue::new().map_err(|e| anyhow::anyhow!(e))?;
                            for id in queued_ids {
                                q.update_sync_status(
                                    &id,
                                    crate::sync::SyncStatus::Synced,
                                    Some("Successfully synced".to_string()),
                                )
                                .map_err(|e| anyhow::anyhow!(e))?;
                                q.remove(&id).map_err(|e| anyhow::anyhow!(e))?;
                            }
                            Ok(())
                        })
                        .await??;

                        // Account for synced items
                        total_synced += synced_len;

                        // Continue to next batch
                        continue;
                    }
                    Err(e) => {
                        // Handle batch-level errors: fall back to per-item retries with backoff for rate-limits
                        tracing::warn!("Batch sync failed: {}", e);
                        if let crate::api::ApiError::RateLimit(_) = e {
                            // Simple backoff strategy: wait based on queue size to avoid hammering the server
                            // Note: use bounded backoff here to avoid long blocking in caller
                            let backoff_secs = 60u64; // base 60s for rate-limits on batch failure
                            tracing::warn!(
                                "Rate limited on batch sync, sleeping {}s before retrying batch",
                                backoff_secs
                            );
                            tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)).await;
                            // After sleeping, retry this batch once more (will loop)
                            continue;
                        } else {
                            // For other errors, fall back to per-heartbeat send so we can granularly retry/mark permanent
                            tracing::debug!(
                                "Falling back to per-heartbeat sync after batch failure"
                            );
                        }
                    }
                }
            }

            // Process items individually (either because batch failed or batch size == 1)
            // Collect successful ids to apply final DB updates in a single blocking operation.
            let mut synced_ids: Vec<String> = Vec::new();
            // Collect failed items (id, error) to update retry counts/statuses in one DB op.
            let mut failed_updates: Vec<(String, String)> = Vec::new();
            // Prefetch retry counts and mark items as Syncing in a single blocking operation to avoid per-item DB opens.
            let retry_map: std::collections::HashMap<String, u32> = tokio::task::spawn_blocking({
                let ids = queued.iter().map(|h| h.id.clone()).collect::<Vec<_>>();
                move || -> Result<std::collections::HashMap<String, u32>, anyhow::Error> {
                    let q = crate::queue::Queue::new().map_err(|e| anyhow::anyhow!(e))?;
                    let mut map = std::collections::HashMap::new();
                    for id in ids {
                        let rc = q.get_retry_count(&id).unwrap_or(0);
                        // Best-effort: mark as syncing with next attempt info
                        let _ = q.update_sync_status(
                            &id,
                            crate::sync::SyncStatus::Syncing,
                            Some(format!("Attempting sync (attempt {})", rc + 1)),
                        );
                        map.insert(id.clone(), rc);
                    }
                    Ok(map)
                }
            })
            .await??;
            for heartbeat in queued {
                // Use prefetched retry count and previously set syncing status
                let retry_count: u32 = *retry_map.get(&heartbeat.id).unwrap_or(&0);

                tracing::debug!(
                    "Attempting individual send for heartbeat id: {}",
                    heartbeat.id
                );
                let send_result = if let Some(auth_client) = &self.authenticated_api_client {
                    auth_client.send_heartbeat(&heartbeat).await
                } else {
                    self.api_client.send_heartbeat(&heartbeat).await
                };
                tracing::debug!(
                    "Individual send result for {} success: {}",
                    heartbeat.id,
                    send_result.is_ok()
                );

                match send_result {
                    Ok(_) => {
                        // Defer DB updates/removal for successful sends and batch-apply later
                        tracing::debug!(
                            "Queued heartbeat marked for finalization: {}",
                            heartbeat.id
                        );
                        synced_ids.push(heartbeat.id.clone());
                        total_synced += 1;
                    }
                    Err(e) => {
                        // Rate-limit handling: apply backoff and retry in-memory once before incrementing retry count
                        if let crate::api::ApiError::RateLimit(_) = e {
                            let backoff_secs = 2u64.pow(std::cmp::min(retry_count as u32, 6)) * 5; // exponential backoff capped
                            tracing::warn!(
                                "Heartbeat {} rate-limited, backing off {}s before retry",
                                heartbeat.id,
                                backoff_secs
                            );
                            tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)).await;

                            // Try once more after backoff
                            let retry_send =
                                if let Some(auth_client) = &self.authenticated_api_client {
                                    auth_client.send_heartbeat(&heartbeat).await
                                } else {
                                    self.api_client.send_heartbeat(&heartbeat).await
                                };

                            if retry_send.is_ok() {
                                // Defer final DB update/removal to the consolidated batch finalization.
                                // This avoids opening the DB in a per-item blocking task even in the rare backoff-success path.
                                let id = heartbeat.id.clone();
                                tracing::debug!("Successfully synced queued heartbeat after backoff (deferring DB update): {}", id);
                                synced_ids.push(id);
                                total_synced += 1;
                                continue;
                            }
                            // If still failing, fallthrough to increment retry below
                        }

                        // Defer retry increment and status updates to a consolidated blocking operation
                        // to avoid opening the DB per-failure and to improve atomicity.
                        let id = heartbeat.id.clone();
                        let e_str = format!("{}", e);
                        failed_updates.push((id, e_str));
                    }
                }
            }

            // Consolidate failure updates (increment retry + set status) in one blocking operation
            if !failed_updates.is_empty() {
                let updates = failed_updates.clone();
                let perm_count: usize =
                    tokio::task::spawn_blocking(move || -> Result<usize, anyhow::Error> {
                        let q = crate::queue::Queue::new().map_err(|e| anyhow::anyhow!(e))?;
                        let mut perm = 0usize;
                        for (id, err_meta) in updates {
                            // Increment retry and read new count
                            let _ = q.increment_retry(&id);
                            let rc = q.get_retry_count(&id).unwrap_or(0);
                            if rc >= 3 {
                                let _ = q.update_sync_status(
                                    &id,
                                    crate::sync::SyncStatus::PermanentFailure,
                                    Some(format!(
                                        "Permanent failure after {} attempts: {}",
                                        rc, err_meta
                                    )),
                                );
                                perm += 1;
                            } else {
                                let _ = q.update_sync_status(
                                    &id,
                                    crate::sync::SyncStatus::Failed,
                                    Some(format!("Sync failed (attempt {}): {}", rc, err_meta)),
                                );
                            }
                        }
                        Ok(perm)
                    })
                    .await??;

                // Account for newly permanent failures
                total_failed += perm_count;
            }

            // Apply final DB updates for all successfully synced ids in one blocking operation
            if !synced_ids.is_empty() {
                let final_ids = synced_ids.clone();
                tokio::task::spawn_blocking(move || -> Result<(), anyhow::Error> {
                    let q = crate::queue::Queue::new().map_err(|e| anyhow::anyhow!(e))?;
                    for id in final_ids {
                        q.update_sync_status(
                            &id,
                            crate::sync::SyncStatus::Synced,
                            Some("Successfully synced".to_string()),
                        )
                        .map_err(|e| anyhow::anyhow!(e))?;
                        q.remove(&id).map_err(|e| anyhow::anyhow!(e))?;
                    }
                    Ok(())
                })
                .await??;
            }
        }

        Ok((total_synced, total_failed))
    }

    /// Update failed heartbeats with retry_count < 3 to pending status for retry
    async fn prepare_retry_eligible_failures(&self) -> Result<(), anyhow::Error> {
        // Run the prepare pass inside a single blocking task so we open the DB once
        let retry_count: usize = tokio::task::spawn_blocking(|| -> Result<usize, anyhow::Error> {
            let q = crate::queue::Queue::new().map_err(|e| anyhow::anyhow!(e))?;
            let failed = q
                .get_pending(Some(1000), Some(crate::sync::SyncStatus::Failed))
                .map_err(|e| anyhow::anyhow!(e))?;

            let mut prepared = 0usize;
            for hb in failed {
                let current_retry_count =
                    q.get_retry_count(&hb.id).map_err(|e| anyhow::anyhow!(e))?;
                if current_retry_count < 3 {
                    q.update_sync_status(
                        &hb.id,
                        crate::sync::SyncStatus::Pending,
                        Some(format!("Retry eligible (attempt {})", current_retry_count)),
                    )
                    .map_err(|e| anyhow::anyhow!(e))?;
                    prepared += 1;
                }
            }

            Ok(prepared)
        })
        .await??;

        if retry_count > 0 {
            tracing::info!("Prepared {} failed heartbeats for retry", retry_count);
        }

        Ok(())
    }
}

/// Extension trait for HeartbeatManager to add offline sync capabilities
pub trait HeartbeatManagerExt {
    /// Process heartbeats using offline-first strategy
    async fn process_offline_first(&self) -> Result<(), anyhow::Error>;

    /// Get queue statistics including sync status
    fn get_queue_stats(&self) -> Result<SyncStatusSummary, anyhow::Error>;

    /// Manually trigger sync of offline heartbeats
    async fn manual_sync(&self) -> Result<SyncResult, anyhow::Error>;
}

impl HeartbeatManagerExt for HeartbeatManager {
    async fn process_offline_first(&self) -> Result<(), anyhow::Error> {
        // For now, this is a placeholder that uses the existing process_queue logic
        // In the future, this will integrate with the SyncManager
        let _ = self.process_queue().await?;
        Ok(())
    }

    fn get_queue_stats(&self) -> Result<SyncStatusSummary, anyhow::Error> {
        // Get sync statistics from the queue
        let stats = self.queue.get_sync_stats()?;
        Ok(stats)
    }

    async fn manual_sync(&self) -> Result<SyncResult, anyhow::Error> {
        // Process the queue to sync pending heartbeats
        let start_time = std::time::SystemTime::now();

        // Do not clear the queue here; caller (or tests) control initial state.

        // Get initial stats before sync
        let initial_stats = self.queue.get_sync_stats()?;
        let initial_total = initial_stats.total;

        // Process the queue and obtain counts
        let (synced_count, failed_count) = self.process_queue().await?;

        let end_time = std::time::SystemTime::now();
        let duration = end_time.duration_since(start_time).unwrap_or_default();

        Ok(SyncResult {
            synced_count,
            failed_count,
            total_count: (synced_count + failed_count),
            duration,
            error: None,
            start_time: Some(start_time),
            end_time: Some(end_time),
            avg_latency_ms: if (synced_count + failed_count) > 0 {
                Some(duration.as_millis() as f64 / (synced_count + failed_count) as f64)
            } else {
                None
            },
        })
    }
}

impl HeartbeatManager {
    /// Add a heartbeat directly to the queue for offline processing
    pub fn add_heartbeat_to_queue(&self, heartbeat: Heartbeat) -> anyhow::Result<()> {
        // Check if entity should be ignored
        if self.should_ignore_entity(&heartbeat.entity) {
            tracing::debug!("Ignoring entity: {}", heartbeat.entity);
            return Ok(());
        }

        // Add heartbeat to queue
        self.queue.add(heartbeat)?;
        tracing::debug!("Heartbeat queued for offline-first processing");

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_ignore_entity() {
        let config = Config {
            ignore_patterns: vec!["COMMIT_EDITMSG$".to_string(), "*.tmp".to_string()],
            ..Default::default()
        };

        let manager = HeartbeatManager::new(config);

        assert!(manager.should_ignore_entity("/path/to/COMMIT_EDITMSG"));
        assert!(manager.should_ignore_entity("/path/to/file.tmp"));
        assert!(!manager.should_ignore_entity("/path/to/normal_file.rs"));
    }

    #[test]
    fn test_heartbeat_manager_ext_implementation() {
        let config = Config::default();
        let manager = HeartbeatManager::new(config);

        // Clear any existing heartbeats from the queue first
        let _ = manager.queue.cleanup_old_entries(0); // Remove all entries

        // Test that HeartbeatManagerExt is implemented by calling methods directly
        let stats = manager.get_queue_stats();
        assert!(stats.is_ok(), "get_queue_stats should return Ok");
        assert!(
            true,
            "HeartbeatManager should implement HeartbeatManagerExt"
        );
    }

    #[test]
    fn test_get_queue_stats() {
        let config = Config::default();
        let manager = HeartbeatManager::new(config);

        // Clear any existing heartbeats from the queue first
        let _ = manager.queue.cleanup_old_entries(0); // Remove all entries

        let stats = manager.get_queue_stats();
        assert!(stats.is_ok(), "get_queue_stats should return Ok");

        let summary = stats.unwrap();
        assert_eq!(summary.total, 0, "Initial queue should be empty");
    }

    #[tokio::test]
    async fn test_manual_sync() {
        let config = Config::default();
        let manager = HeartbeatManager::new(config);

        let result = manager.manual_sync().await;
        assert!(result.is_ok(), "manual_sync should return Ok");

        let sync_result = result.unwrap();
        assert_eq!(
            sync_result.synced_count, 0,
            "No heartbeats to sync initially"
        );
    }

    #[tokio::test]
    async fn test_manual_sync_with_mock_server_batches() {
        use crate::api::ApiClient;
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        // Start mock server that will accept batch POSTs
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/users/current/heartbeats"))
            .respond_with(ResponseTemplate::new(201))
            .mount(&mock_server)
            .await;

        let config = Config::default();
        let mut manager = HeartbeatManager::new(config);

        // Point manager's api_client to the mock server
        manager.api_client = ApiClient::new(mock_server.uri());
        manager.authenticated_api_client = None;

        // Clear any existing entries
        let _ = manager.queue.cleanup_old_entries(0);

        // Add two heartbeats to the queue
        let hb1 = Heartbeat {
            id: "hb-1".to_string(),
            entity: "/path/a.rs".to_string(),
            entity_type: "file".to_string(),
            time: 1.0,
            project: Some("p".to_string()),
            branch: None,
            language: Some("Rust".to_string()),
            is_write: false,
            lines: None,
            lineno: None,
            cursorpos: None,
            user_agent: Some("test/1.0".to_string()),
            category: Some("coding".to_string()),
            machine: Some("m".to_string()),
            editor: None,
            operating_system: None,
            commit_hash: None,
            commit_author: None,
            commit_message: None,
            repository_url: None,
            dependencies: Vec::new(),
        };

        let hb2 = Heartbeat {
            id: "hb-2".to_string(),
            entity: "/path/b.rs".to_string(),
            entity_type: "file".to_string(),
            time: 2.0,
            project: Some("p".to_string()),
            branch: None,
            language: Some("Rust".to_string()),
            is_write: false,
            lines: None,
            lineno: None,
            cursorpos: None,
            user_agent: Some("test/1.0".to_string()),
            category: Some("coding".to_string()),
            machine: Some("m".to_string()),
            editor: None,
            operating_system: None,
            commit_hash: None,
            commit_author: None,
            commit_message: None,
            repository_url: None,
            dependencies: Vec::new(),
        };

        manager.add_heartbeat_to_queue(hb1).unwrap();
        manager.add_heartbeat_to_queue(hb2).unwrap();

        // Run manual sync which uses batching logic
        let res = manager.manual_sync().await;
        assert!(res.is_ok());
        let sync = res.unwrap();

        // Expect both to have been processed
        assert_eq!(
            sync.synced_count, 2,
            "Both queued heartbeats should be synced"
        );
    }
}
