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

/// How many times a heartbeat is re-sent before the queue gives up on it.
const MAX_SYNC_ATTEMPTS: u32 = 3;

/// Why this failure means "stop sending" rather than "try the next one".
///
/// A rate limit is the server asking us to back off; a transport failure means
/// we never reached it. Pushing on would repeat the same failure once per
/// heartbeat and charge each of them one of its three attempts for a condition
/// that is not its fault — a single offline invocation would exhaust the queue's
/// whole retry budget.
fn defer_reason(error: &crate::api::ApiError) -> Option<String> {
    match error {
        crate::api::ApiError::RateLimit { retry_after, .. } => Some(format!(
            "Rate limited{}",
            retry_after
                .map(|d| format!(", retry after {}s", d.as_secs()))
                .unwrap_or_default()
        )),
        crate::api::ApiError::Network(e) => Some(format!("Cannot reach the server ({})", e)),
        _ => None,
    }
}

/// Put heartbeats back on the queue as pending without touching their retry
/// count: they were either never attempted, or the server asked us to come
/// back later, and neither is the heartbeat's fault.
async fn requeue_pending(ids: Vec<String>, reason: &str) -> Result<(), anyhow::Error> {
    if ids.is_empty() {
        return Ok(());
    }

    let reason = reason.to_string();
    tokio::task::spawn_blocking(move || -> Result<(), anyhow::Error> {
        let q = crate::queue::Queue::new().map_err(|e| anyhow::anyhow!(e))?;
        for id in ids {
            q.update_sync_status(&id, crate::sync::SyncStatus::Pending, Some(reason.clone()))
                .map_err(|e| anyhow::anyhow!(e))?;
        }
        Ok(())
    })
    .await?
}

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

    /// AI telemetry, flattened into the payload so it matches the field names
    /// the Chronova API expects. `default` keeps heartbeats that were queued
    /// by an older build (whose JSON lacks these keys) deserializable.
    #[serde(default, flatten)]
    pub ai: AiTelemetry,
}

/// Optional AI-assistance telemetry attached to a heartbeat.
///
/// Field names mirror the Chronova API's heartbeat schema rather than
/// wakatime-cli's internal `ai_line_changes`/`ai_tokens` naming, because the
/// server is what ultimately validates them. Every line count is required by
/// the API to be non-negative, so callers must clamp before populating.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AiTelemetry {
    /// Which assistant produced the activity, e.g. `claude-code`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_agent: Option<String>,

    /// What the assistant did, e.g. `edit`, `create`, `prompt`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_action: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_prompt_tokens: Option<i64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_completion_tokens: Option<i64>,

    /// Magnitude of the change the assistant proposed, in lines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_lines_suggested: Option<i32>,

    /// Net lines the assistant added, in lines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_lines_accepted: Option<i32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_lines_rejected: Option<i32>,

    /// Marks the heartbeat as AI-generated for the server's analytics split.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_ai_agent: Option<bool>,
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
    /// Build a manager on the shared on-disk queue.
    ///
    /// Note that this empties that queue; see `new_with_queue` when the queued
    /// heartbeats must survive.
    pub fn new(config: Config) -> Result<Self, anyhow::Error> {
        let queue = Queue::new().map_err(|e| anyhow::anyhow!(e))?;
        // Ensure a fresh queue state for newly constructed managers (helps tests/isolation)
        // Ignore any error here — best effort cleanup to avoid leaking state between runs.
        let _ = queue.cleanup_old_entries(0);

        Self::new_with_queue(config, queue)
    }

    /// Create a HeartbeatManager with a custom queue (useful for testing with isolated queues)
    pub fn new_with_queue(config: Config, queue: Queue) -> Result<Self, anyhow::Error> {
        // A bad proxy URL or certificate bundle is reported here rather than
        // silently ignored or turned into a panic.
        let api_client =
            ApiClient::with_transport(config.get_api_url(), &config.transport_options())
                .map_err(|e| anyhow::anyhow!(e))?;
        let authenticated_api_client = config
            .get_api_key(None)
            .map(|key| api_client.clone().with_api_key(key));
        let collector = DataCollector::new();

        Ok(Self {
            config,
            api_client,
            authenticated_api_client,
            queue,
            collector,
        })
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

        // Process any queued heartbeats using sync strategy.
        //
        // The heartbeat is already on disk, so a send that fails here is
        // deferred, not lost, and this process exits 0. Editor plugins invoke
        // the CLI once per keystroke batch; reporting a queued heartbeat as a
        // failed run would have every one of them log an error for data that is
        // safe. Non-zero exit is reserved for paths that actually drop data —
        // `--disable-offline`, which never queues in the first place.
        if let Err(e) = self.process_queue().await {
            tracing::warn!(
                "Queued heartbeats could not be sent yet ({}); they stay queued for the next run",
                e
            );
        }

        Ok(())
    }

    /// Builds a `Heartbeat` for `entity` from `cli`, collecting project,
    /// git and language metadata for it. Public so callers outside this
    /// module (the `--extra-heartbeats` path in `main.rs`) can reuse the
    /// same construction the normal `process` flow uses.
    pub async fn create_heartbeat(
        &self,
        cli: Cli,
        entity: String,
    ) -> Result<Heartbeat, anyhow::Error> {
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
        let branch = if self.config.disable_git_info || self.config.hide_branch_names {
            None
        } else {
            cli.branch
                .or_else(|| git_info.as_ref().and_then(|g| g.branch.clone()))
        };

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
            commit_hash: if self.config.disable_git_info || self.config.hide_commit_hash {
                None
            } else {
                git_info.as_ref().and_then(|g| g.commit_hash.clone())
            },
            commit_author: if self.config.disable_git_info || self.config.hide_commit_author {
                None
            } else {
                git_info.as_ref().and_then(|g| g.commit_author.clone())
            },
            commit_message: if self.config.disable_git_info || self.config.hide_commit_message {
                None
            } else {
                git_info.as_ref().and_then(|g| g.commit_message.clone())
            },
            repository_url: if self.config.disable_git_info || self.config.hide_repository_url {
                None
            } else {
                git_info.as_ref().and_then(|g| g.repository_url.clone())
            },
            dependencies: Vec::new(),
            ai: Default::default(),
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

    /// Flush the offline queue, one batch at a time.
    ///
    /// Returns `(synced, not_synced)`. Anything the server did not accept but
    /// might yet — an entry it never mentioned, or a batch it asked us to stop
    /// sending — counts as not synced, *not* only the entries that have
    /// exhausted their retries.
    ///
    /// A heartbeat the server refused outright with a `400` counts as neither.
    /// It is dropped rather than retried, so it is not synced, but it is not
    /// unfinished business either; counting it as failed would make a window the
    /// server deliberately discarded look like a total failure and send
    /// `ai_sync` back to re-derive it forever.
    ///
    /// That distinction is load-bearing. `ai_sync::sync_ai_activity` rolls its
    /// batch back and holds `ai_logs_last_parsed_at` when
    /// `synced_count == 0 && failed_count > 0`, and advances the cutoff
    /// otherwise. If a batch the server silently dropped reported
    /// `(0, 0)` instead, the cutoff would advance as though the work were done
    /// while those heartbeats sat in a queue that the next `--entity`
    /// invocation wipes (`HeartbeatManager::new`). The transcripts are the
    /// durable store; this number is what decides whether to go back to them.
    async fn process_queue(&self) -> Result<(usize, usize), anyhow::Error> {
        // Process the queue in batches to avoid loading everything into memory at once.
        let batch_size: usize = 50;

        // Counters to return to callers
        let mut total_synced: usize = 0;
        let mut total_failed: usize = 0;

        // Give heartbeats that failed in an *earlier* run another turn. This
        // runs once, before the drain, and deliberately not inside the loop:
        // promoting mid-drain would feed a heartbeat that just failed straight
        // back into the next iteration, so one invocation would spend all three
        // of its attempts on the same unreachable server instead of leaving two
        // for later runs.
        tokio::task::spawn_blocking(move || -> Result<(), anyhow::Error> {
            let q = crate::queue::Queue::new().map_err(|e| anyhow::anyhow!(e))?;
            let failed = q
                .get_pending(Some(1000), Some(crate::sync::SyncStatus::Failed))
                .map_err(|e| anyhow::anyhow!(e))?;
            for hb in failed {
                let current_retry_count = q.get_retry_count(&hb.id).unwrap_or(0);
                if current_retry_count < MAX_SYNC_ATTEMPTS {
                    q.update_sync_status(
                        &hb.id,
                        crate::sync::SyncStatus::Pending,
                        Some(format!("Retry eligible (attempt {})", current_retry_count)),
                    )
                    .map_err(|e| anyhow::anyhow!(e))?;
                }
            }
            Ok(())
        })
        .await??;

        loop {
            let queued =
                tokio::task::spawn_blocking(move || -> Result<Vec<Heartbeat>, anyhow::Error> {
                    let q = crate::queue::Queue::new().map_err(|e| anyhow::anyhow!(e))?;
                    q.get_pending(Some(batch_size), None)
                        .map_err(|e| anyhow::anyhow!(e))
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
                    Ok(outcome) => {
                        // A bulk request can answer 2xx while dropping single
                        // heartbeats, so only the ones the server named as
                        // accepted may leave the queue; the rest stay for a
                        // later attempt.
                        let batch = queued.clone();
                        let applied = tokio::task::spawn_blocking(
                            move || -> Result<crate::sync::OutcomeApplied, anyhow::Error> {
                                let q =
                                    crate::queue::Queue::new().map_err(|e| anyhow::anyhow!(e))?;
                                crate::sync::apply_batch_outcome(
                                    &q,
                                    &batch,
                                    &outcome,
                                    MAX_SYNC_ATTEMPTS,
                                )
                                .map_err(|e| anyhow::anyhow!(e))
                            },
                        )
                        .await??;

                        if applied.permanent > 0 {
                            tracing::warn!(
                                "{} heartbeat(s) exhausted their {} attempts and were given up on",
                                applied.permanent,
                                MAX_SYNC_ATTEMPTS
                            );
                        }
                        total_synced += applied.accepted;
                        total_failed += applied.failed;

                        // Continue to next batch
                        continue;
                    }
                    Err(e) => {
                        // Handle batch-level errors: fall back to per-item retries with backoff for rate-limits
                        tracing::warn!("Batch sync failed: {}", e);

                        if matches!(e, crate::api::ApiError::Auth(_)) {
                            // The cascade already offered this key as Bearer,
                            // Basic and X-API-Key and the server refused all
                            // three, so the key is the problem. Falling back to
                            // one request per heartbeat would re-run that
                            // cascade for each of them — 50 heartbeats is 150
                            // requests that cannot succeed, and the
                            // failed-to-pending promotion would do it again on
                            // the next pass. Put the batch back untouched and
                            // report it; the counts go with the error, so a
                            // caller that retries re-derives them from scratch.
                            tracing::error!(
                                "Authentication rejected for the whole batch ({}); leaving {} heartbeat(s) queued",
                                e,
                                queued.len()
                            );
                            requeue_pending(
                                queued.iter().map(|h| h.id.clone()).collect(),
                                "Authentication rejected; deferred until the credentials change",
                            )
                            .await?;

                            return Err(e.into());
                        }

                        if let Some(reason) = defer_reason(&e) {
                            // Falling back to one request per heartbeat would
                            // be the opposite of backing off, and against an
                            // unreachable server it just repeats the same
                            // failure fifty more times.
                            tracing::warn!(
                                "{} on batch sync; leaving {} heartbeat(s) queued",
                                reason,
                                queued.len()
                            );
                            total_failed += queued.len();
                            requeue_pending(
                                queued.iter().map(|h| h.id.clone()).collect(),
                                &format!("{}; deferred to the next sync", reason),
                            )
                            .await?;
                            break;
                        }

                        // For other errors, fall back to per-heartbeat send so we can granularly retry/mark permanent
                        tracing::debug!("Falling back to per-heartbeat sync after batch failure");
                    }
                }
            }

            // Process items individually (either because batch failed or batch size == 1)
            // Collect successful ids to apply final DB updates in a single blocking operation.
            let mut synced_ids: Vec<String> = Vec::new();
            // Collect failed items (id, error) to update retry counts/statuses in one DB op.
            let mut failed_updates: Vec<(String, String)> = Vec::new();
            // Mark every item as Syncing in a single blocking operation to avoid per-item DB opens.
            tokio::task::spawn_blocking({
                let ids = queued.iter().map(|h| h.id.clone()).collect::<Vec<_>>();
                move || -> Result<(), anyhow::Error> {
                    let q = crate::queue::Queue::new().map_err(|e| anyhow::anyhow!(e))?;
                    for id in ids {
                        let rc = q.get_retry_count(&id).unwrap_or(0);
                        // Best-effort: mark as syncing with next attempt info
                        let _ = q.update_sync_status(
                            &id,
                            crate::sync::SyncStatus::Syncing,
                            Some(format!("Attempting sync (attempt {})", rc + 1)),
                        );
                    }
                    Ok(())
                }
            })
            .await??;
            // Set when the server rate-limits us mid-batch: the rest of the
            // batch is put back and left to a later flush.
            let mut deferred_ids: Vec<String> = Vec::new();
            for (index, heartbeat) in queued.iter().enumerate() {
                tracing::debug!(
                    "Attempting individual send for heartbeat id: {}",
                    heartbeat.id
                );
                let send_result = if let Some(auth_client) = &self.authenticated_api_client {
                    auth_client.send_heartbeat(heartbeat).await
                } else {
                    self.api_client.send_heartbeat(heartbeat).await
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
                        // Some answers mean "stop sending", not "try the next
                        // one": the server asked us to slow down, refused the
                        // key under every scheme, or never answered at all.
                        // Sleeping would stall a CLI the editor plugin
                        // re-invokes every minute, and pushing on would repeat
                        // the same failure for every remaining heartbeat and
                        // spend an attempt on each. None of them has earned
                        // that, so they go back untouched.
                        let stop = if matches!(e, crate::api::ApiError::Auth(_)) {
                            tracing::error!(
                                "Authentication rejected after {} heartbeat(s) ({}); leaving the rest queued",
                                index,
                                e
                            );
                            true
                        } else if let Some(reason) = defer_reason(&e) {
                            tracing::warn!(
                                "{} after {} heartbeat(s); leaving the rest queued",
                                reason,
                                index
                            );
                            true
                        } else {
                            false
                        };

                        if stop {
                            deferred_ids.extend(queued[index..].iter().map(|h| h.id.clone()));
                            break;
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
                            if rc >= MAX_SYNC_ATTEMPTS {
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

                if perm_count > 0 {
                    tracing::warn!(
                        "{} heartbeat(s) exhausted their {} attempts and were given up on",
                        perm_count,
                        MAX_SYNC_ATTEMPTS
                    );
                }
                total_failed += failed_updates.len();
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

            if !deferred_ids.is_empty() {
                total_failed += deferred_ids.len();
                requeue_pending(deferred_ids, "Send deferred to the next sync").await?;
                break;
            }
        }

        Ok((total_synced, total_failed))
    }

    /// Update failed heartbeats with retry_count < 3 to pending status for retry
    #[allow(dead_code)]
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
#[allow(async_fn_in_trait)]
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
        let _initial_total = initial_stats.total;

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

    /// Create a HeartbeatManager backed by an isolated temp database.
    /// Returns the manager and the TempDir guard — the caller must keep
    /// the TempDir alive for as long as the manager is used.
    /// This prevents parallel tests from contending on the shared `~/.chronova/queue.db`.
    fn create_test_manager(config: Config) -> (HeartbeatManager, tempfile::TempDir) {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let db_path = temp_dir.path().join("test_queue.db");
        let queue = Queue::with_path(db_path).expect("Failed to create test queue");
        let manager =
            HeartbeatManager::new_with_queue(config, queue).expect("Failed to create test manager");
        (manager, temp_dir)
    }

    #[test]
    fn test_should_ignore_entity() {
        let config = Config {
            ignore_patterns: vec!["COMMIT_EDITMSG$".to_string(), "*.tmp".to_string()],
            ..Default::default()
        };

        let (manager, _temp_dir) = create_test_manager(config);

        assert!(manager.should_ignore_entity("/path/to/COMMIT_EDITMSG"));
        assert!(manager.should_ignore_entity("/path/to/file.tmp"));
        assert!(!manager.should_ignore_entity("/path/to/normal_file.rs"));
    }

    #[test]
    fn test_heartbeat_manager_ext_implementation() {
        let config = Config::default();
        let (manager, _temp_dir) = create_test_manager(config);

        // Clear any existing heartbeats from the queue first
        let _ = manager.queue.cleanup_old_entries(0); // Remove all entries

        // Test that HeartbeatManagerExt is implemented by calling methods directly
        let stats = manager.get_queue_stats();
        assert!(stats.is_ok(), "get_queue_stats should return Ok");
    }

    #[test]
    fn test_get_queue_stats() {
        let config = Config::default();
        let (manager, _temp_dir) = create_test_manager(config);

        // Clear any existing heartbeats from the queue first
        let _ = manager.queue.cleanup_old_entries(0); // Remove all entries

        let stats = manager.get_queue_stats();
        assert!(stats.is_ok(), "get_queue_stats should return Ok");

        let summary = stats.unwrap();
        assert_eq!(summary.total, 0, "Initial queue should be empty");
    }

    #[tokio::test]
    #[ignore = "manual_sync internally opens Queue::new() which uses the shared DB path"]
    async fn test_manual_sync() {
        let config = Config::default();
        let (manager, _temp_dir) = create_test_manager(config);

        let result = manager.manual_sync().await;
        assert!(result.is_ok(), "manual_sync should return Ok");

        let sync_result = result.unwrap();
        assert_eq!(
            sync_result.synced_count, 0,
            "No heartbeats to sync initially"
        );
    }

    #[tokio::test]
    #[ignore = "manual_sync internally opens Queue::new() which uses the shared DB path"]
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
        let (mut manager, _temp_dir) = create_test_manager(config);

        // Point manager's api_client to the mock server
        manager.api_client = ApiClient::new(mock_server.uri());
        manager.authenticated_api_client = None;

        // Clear any existing entries from previous test runs
        let _ = manager.queue.cleanup_old_entries(0);

        // Add two heartbeats to the queue using the manager's queue directly
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
            ai: Default::default(),
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
            ai: Default::default(),
        };

        // Add heartbeats directly to the manager's queue
        manager.queue.add(hb1).unwrap();
        manager.queue.add(hb2).unwrap();

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

#[cfg(test)]
mod ai_telemetry_tests {
    use super::*;

    fn base_json() -> serde_json::Value {
        serde_json::json!({
            "id": "abc",
            "entity": "/proj/src/main.rs",
            "type": "file",
            "time": 1_757_000_000.0,
            "project": "proj",
            "branch": null,
            "language": "Rust",
            "is_write": true,
            "lines": null,
            "lineno": null,
            "cursorpos": null,
            "user_agent": "ua",
            "category": "ai coding",
            "machine": "host",
            "editor": null,
            "operating_system": null,
            "commit_hash": null,
            "commit_author": null,
            "commit_message": null,
            "repository_url": null,
            "dependencies": []
        })
    }

    #[test]
    fn a_heartbeat_queued_before_ai_fields_existed_still_deserializes() {
        let heartbeat: Heartbeat = serde_json::from_value(base_json())
            .expect("legacy queue rows must remain readable after the schema grew");
        assert_eq!(heartbeat.ai, AiTelemetry::default());
    }

    #[test]
    fn ai_fields_serialize_flat_and_omit_empties() {
        let mut heartbeat: Heartbeat = serde_json::from_value(base_json()).unwrap();
        heartbeat.ai = AiTelemetry {
            ai_agent: Some("claude-code".to_string()),
            ai_action: Some("edit".to_string()),
            ai_prompt_tokens: Some(120),
            ai_completion_tokens: None,
            ai_lines_suggested: Some(18),
            ai_lines_accepted: Some(0),
            ai_lines_rejected: None,
            is_ai_agent: Some(true),
        };

        let value = serde_json::to_value(&heartbeat).unwrap();
        let map = value.as_object().unwrap();

        assert_eq!(map.get("ai_agent").unwrap(), "claude-code");
        assert_eq!(map.get("ai_prompt_tokens").unwrap(), 120);
        assert_eq!(map.get("ai_lines_accepted").unwrap(), 0);
        assert_eq!(map.get("is_ai_agent").unwrap(), true);
        assert!(
            map.get("ai").is_none(),
            "the telemetry must be flattened, not nested under `ai`"
        );
        assert!(
            !map.contains_key("ai_completion_tokens"),
            "unset AI fields must be omitted rather than sent as null"
        );
        assert!(!map.contains_key("ai_lines_rejected"));
    }

    #[test]
    fn ai_fields_round_trip_through_the_queues_json_blob() {
        let mut heartbeat: Heartbeat = serde_json::from_value(base_json()).unwrap();
        heartbeat.ai.ai_lines_suggested = Some(7);
        heartbeat.ai.is_ai_agent = Some(true);

        let encoded = serde_json::to_string(&heartbeat).unwrap();
        let decoded: Heartbeat = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded.ai.ai_lines_suggested, Some(7));
        assert_eq!(decoded.ai.is_ai_agent, Some(true));
        assert_eq!(decoded.category.as_deref(), Some("ai coding"));
    }
}
