---
type: reference
title: Offline & Sync Behavior
description: How the SQLite offline queue works, how sync is retried, and how failures are handled.
tags: [offline, queue, sync, sqlite, retry]
---

# Offline & Sync Behavior

Chronova CLI is offline-first: every heartbeat is written to a local SQLite queue before any network request is attempted. This ensures no activity is lost during outages, sleep, or unreliable networks.

## Queue storage

The queue is implemented in `src/queue.rs` using `rusqlite` with bundled SQLite. It exposes a `QueueOps` trait so the storage layer can be mocked in tests.

Default queue location: `~/.chronova/queue.db` (override with `--offline-queue-file` or `--offline-queue-file-legacy`).

### Schema

`Queue::init_database()` in `src/queue.rs` creates the following schema:

- `heartbeats` table:
  - `id TEXT PRIMARY KEY` — heartbeat UUID.
  - `data TEXT NOT NULL` — serialized `Heartbeat` JSON.
  - `sync_status TEXT` — `pending`, `syncing`, `synced`, `failed`, or `permanent_failure`.
  - `sync_metadata TEXT` — error notes / sync context.
  - `retry_count INTEGER` — number of attempts.
  - `created_at DATETIME` — when the entry was queued.
  - `last_attempt DATETIME` — last sync attempt.
- `schema_version` table — tracks applied migrations (migration v1 adds `sync_status` and `sync_metadata`).
- Indexes on `sync_status`, `created_at`, and `retry_count`.

The connection uses WAL journal mode (`journal_mode=WAL`, `synchronous=NORMAL`) for better write concurrency and reduced fsync overhead.

## Queue operations

`QueueOps` methods include:

- `add(heartbeat)` — insert a new heartbeat as `Pending`.
- `add_batch(heartbeats)` — insert multiple heartbeats in one transaction.
- `get_pending(limit, status_filter)` — fetch entries by status.
- `update_sync_status(id, status, metadata)` — move an entry to a new state.
- `remove(id)` — delete a synced heartbeat.
- `get_retry_count(id)` — inspect retry counter.
- `increment_retry(id)` — bump retry counter.
- `cleanup_old_entries(days)` — remove old entries.
- `enforce_max_count(max_count)` — trim oldest entries when the queue exceeds a size limit.
- `deduplicate(time_window_seconds)` — collapse duplicate pending entries within a time window.
- `vacuum()` — reclaim disk space.
- `count_by_status()` / `get_sync_stats()` / `count()` — produce statistics.

## Sync status lifecycle

Statuses are defined in `src/sync.rs`:

1. `Pending` — queued but not yet sent.
2. `Syncing` — currently in flight.
3. `Synced` — successfully sent; removed from queue.
4. `Failed` — transient failure; eligible for retry.
5. `PermanentFailure` — exceeded retries; left in queue for inspection.

## Sync flow

`HeartbeatManager::process_queue()` drives sync:

1. Open a single DB connection per loop iteration.
2. Promote retry-eligible `Failed` entries back to `Pending` (up to 3 attempts in the heartbeat manager loop).
3. Fetch a batch of `Pending` entries (default batch size 50).
4. Send each heartbeat (or batch) via `AuthenticatedApiClient`.
5. On success, mark `Synced` and remove the entry from the queue.
6. On transient failure, mark `Failed` and increment retry count.
7. On permanent failure (auth, max retries reached by `ChronovaSyncManager`), mark `PermanentFailure`.
8. Repeat until no pending entries remain.

## Retry strategy

`src/sync.rs::RetryStrategy` uses exponential backoff with jitter. Default configuration (from `SyncConfig::default()` in `src/sync.rs`):

- `max_retry_attempts`: 5
- `retry_base_delay_seconds`: 1
- `retry_max_delay_seconds`: 60
- `retry_use_jitter`: true
- `sync_interval_seconds`: 300 (5 minutes)

These defaults can be overridden in `~/.chronova.cfg` under `[settings]` using the keys `sync_max_retries`, `sync_retry_base_delay`, `sync_retry_max_delay`, `sync_interval`, `sync_retry_use_jitter`, `sync_max_queue_size`, `sync_retention_days`, and `sync_background`.

## Manual sync

Force an immediate sync of offline activity:

```bash
chronova-cli --sync-offline-activity 100
```

Force sync all queued heartbeats regardless of connectivity:

```bash
chronova-cli --force-sync
```

Inspect queue status:

```bash
chronova-cli --offline-count
```

Read extra heartbeats from STDIN:

```bash
echo '[{...}]' | chronova-cli --extra-heartbeats
```

## Failure handling

- **Network errors** — retry with exponential backoff.
- **Rate limits** — pause and retry after the backoff window.
- **Authentication errors** — surfaced immediately; check `api_key` and `api_url`.
- **Queue corruption** — `queue.rs` has backup/recovery logic to protect the SQLite file.

## Related pages

- [Heartbeat Flow](../heartbeat/index.md)
- [API Compatibility](../api-compatibility/index.md)
- [Configuration](../configuration/index.md)
- [Logging & Updates](./logging-updates.md)
