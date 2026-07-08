# Sync and offline

The `[Unreleased]` section of `CHANGELOG.md` describes the major surface here: an offline-first SQLite queue, automatic background sync, retry with exponential backoff and jitter, and a `PermanentFailure` state for entries that should not be retried. This page explains how that surface hangs together in the code.

For the in-line heartbeat path that drops work into this machinery, see [heartbeat-flow.md](heartbeat-flow.md). For the build and module map, see [architecture.md](architecture.md).

## Mental model

There are three concerns, and each is owned by one module:

- **Storage** — `src/queue.rs`. SQLite-backed persistent queue with a schema version, indexes, dedup, and a `QueueOps` trait that the rest of the code talks to.
- **Sync** — `src/sync.rs`. The `SyncManager` trait and the `ChronovaSyncManager` implementation that decide *what to send, when, and how often*. Holds connectivity state, retry strategy, and `SyncResult` metrics.
- **Transport** — `src/api.rs`. `ApiClient` and `AuthenticatedApiClient` — the actual HTTP layer, with typed errors (`ApiError::Network`, `::Api`, `::Auth`, `::RateLimit`).

`HeartbeatManager` (in `src/heartbeat.rs`) wires the three together and exposes the offline-first workflow via the `HeartbeatManagerExt` extension trait.

## Queue: `src/queue.rs`

### Storage

- SQLite database in `~/.chronova/queue.db` by default, overridable via `--offline-queue-file` (internal flag, mainly for tests).
- Schema is versioned via a `schema_version` table; the queue runs migrations on open.
- Indexes on `sync_status`, `created_at`, and `retry_count` to keep batch lookups and retry sweeps cheap.

### `QueueOps` trait

The trait is the contract every queue consumer goes through, so tests can substitute an in-memory queue. Methods include:

- `add(heartbeat)`, `add_batch(heartbeats)` — single + bulk insert; the bulk path uses a transaction.
- `get_pending(limit, status_filter)` — fetch a batch of heartbeats to send.
- `remove(id)`, `update_sync_status(id, status, metadata)` — bookkeeping.
- `count_by_status(status)`, `get_sync_stats()` — read-only counts.
- `cleanup_old_entries(max_age_days)`, `enforce_max_count(max_count)`, `vacuum()` — retention and storage hygiene.
- `deduplicate()`, `increment_retry()`, `get_retry_count()`, `count()` — helpers used by the sync layer for dedup and retry policy.

### `QueueError`

`QueueError` covers the failure modes: `Database`, `Serialization`, `Io`, `SyncStatusNotFound`, `InvalidSyncStatus`, `EntryNotFound`, `QueueFull`, `StorageLimitExceeded`, `DatabaseCorruption`. The corruption variant matters: the queue detects and recovers from corruption (per the `[Unreleased]` notes in `CHANGELOG.md`).

### `QueueEntry` / `QueueStats`

A `QueueEntry` is a `Heartbeat` plus `sync_status`, `sync_metadata: Option<String>`, `retry_count: u32`, `created_at`, and `last_attempt`. `QueueStats` (and the `SyncStatusSummary` returned to `--offline-count`) breaks the count down by status.

## Sync: `src/sync.rs`

### Status state machine

```
Pending ──► Syncing ──► Synced       (removed from queue)
                  │
                  ├──► Failed        (retry with backoff, up to N attempts)
                  │
                  └──► PermanentFailure  (no more retries)
```

`SyncStatus` is the enum that drives that state. `From<&str>` and `From<SyncStatus> for String` keep the SQLite text columns and the in-memory enum in sync.

### Retry strategy

`RetryStrategy` calculates per-attempt delays with exponential backoff and a configurable jitter band:

- `base_delay_seconds` doubles per attempt, capped at `max_delay_seconds`.
- Jitter is `0.5..=1.5` by default (`use_jitter = true`), so a thundering herd of heartbeats from many editors doesn't synchronize their retry storms.

`SyncError::is_retryable_error` is the other half of the policy: `Network` and `RateLimit` are retryable, `Auth` and `Config` are not (and become `PermanentFailure`).

### `SyncManager` / `ChronovaSyncManager`

- `SyncManager` is the async trait; `ChronovaSyncManager` is the production implementation.
- Connectivity is tracked with `Arc<AtomicBool>` and the last check is cached in `Arc<RwLock<Option<SystemTime>>>` so the sync loop doesn't hammer the network.
- The manager owns a `QueueOps` (so tests can pass a fake) and an `ApiClient` (so tests can wrap `wiremock`).
- Each sync attempt produces a `SyncResult` with `synced_count`, `failed_count`, `total_count`, `duration`, optional `error`, `start_time` / `end_time`, and `avg_latency_ms`. A run also updates `SyncStatusSummary` for `--offline-count` and for `get_queue_stats`.

### Background sync

The `[Unreleased]` notes mention a 5-minute background sync. The pattern is the standard one:

```rust
tokio::spawn(async move {
    loop {
        if let Err(e) = sync_once().await { tracing::warn!(?e, "sync failed"); }
        tokio::time::sleep(sync_config.interval).await;
    }
});
```

Manual triggers (`--force-sync`, `--sync-offline-activity <n>`) call the same `sync_once` shape, just synchronously from the CLI invocation.

## Transport: `src/api.rs`

- `ApiClient` is the base HTTP client with a 30 s timeout (matches `--timeout`).
- `AuthenticatedApiClient` adds the auth headers; the constructor is the place to plug in Bearer / Basic / `X-API-Key` based on the credentials it was built with.
- Methods: `send_heartbeat`, `send_heartbeats_batch`, `get_today_stats`, `get_today_statusbar`, `check_connectivity`, etc.
- Response types: `StatsResponse`, `StatusBarResponse`, `StatusBarFullResponse`, `LanguageStat`, `ProjectStat`, `EditorStat`, `OsStat`, `CategoryStat`, `BestDay`, `DailyStat` — all matching the WakaTime JSON shape so the same parser works on a Chronova backend.
- `ApiError` is the typed failure mode the sync layer pattern-matches on. `ApiError::RateLimit` triggers the backoff path; `ApiError::Auth` triggers `PermanentFailure`; `ApiError::Network` is the default retryable.

## End-to-end: what happens to a heartbeat

The same heartbeat hits the queue and the API in two steps. From `HeartbeatManager::process` (in `src/heartbeat.rs`):

1. `tokio::task::spawn_blocking(...).await??` writes the heartbeat to SQLite via `Queue::add`.
2. `self.process_queue().await?` then drives the sync:

   ```
   Queue::get_pending(50) ──► ApiClient::send_heartbeats_batch
                                   │
                 ┌─────────────────┼──────────────────┐
              success           4xx/5xx            rate limit
                 │                 │                   │
        update_sync_status    update_sync_status   back off + retry
        (Synced) + remove     (Failed)             (Failed → Syncing)
                                  │
                         attempts > max_retries
                                  │
                         update_sync_status(PermanentFailure)
   ```

The `--offline-count` view is what an operator or support engineer uses to see whether anything is stuck:

```
Offline heartbeats queue status:
  Total: …
  Pending: …
  Syncing: …
  Synced: …
  Failed: …
  Permanent failures: …
```

If `Failed` or `PermanentFailure` is non-zero, the typical remedies are `--force-sync` to drain manually, or to fix the underlying issue (auth, network) and let background sync take over.

## Configuration that affects sync

`Config::sync_config` (nested struct in `src/config.rs`) is the in-memory representation of the sync-specific `[settings]` keys. The most important knobs:

- `max_retries` — after this many failed attempts, the heartbeat goes to `PermanentFailure`.
- `base_delay_seconds`, `max_delay_seconds`, `use_jitter` — drive `RetryStrategy`.
- `batch_size` — the cap used by `get_pending` and `send_heartbeats_batch`.
- `background_interval` — how often the background sync loop runs (default 5 minutes).
- `connectivity_cache_seconds` — how long the cached `check_connectivity` result is trusted.

CLI overrides: `--sync-offline-activity <n>` and `--force-sync` exist when you want to bypass the scheduled loop and drain now.

## Where to start when changing this area

- **New queue field** — add to `QueueEntry` *and* the SQLite schema, plus a migration in `src/queue.rs`. Don't forget the index if you'll filter on it.
- **New sync status** — extend the `SyncStatus` enum, update `From<&str>` / `From<SyncStatus> for String`, and add a branch to the state machine in `ChronovaSyncManager`.
- **New retry policy** — extend `RetryStrategy` and `is_retryable_error`, then update the unit test for backoff in `tests/retry_mechanism_test.rs`.
- **New API method** — add to `ApiClient`, cover all three auth modes, and add a `wiremock` integration test. See [testing.md](testing.md).
- **Race conditions** — recent test work (see `tests/AGENTS.md`) ignored a few tests that share the default queue. The current pattern is to spin up a queue in a `tempfile::TempDir` per test. New tests should do the same; see [testing.md](testing.md).
