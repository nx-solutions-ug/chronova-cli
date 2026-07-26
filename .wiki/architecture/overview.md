---
type: architecture
title: Architecture Overview
description: Module responsibilities, critical data flows, and design patterns in the Chronova CLI codebase.
tags: [architecture, modules, design, rust]
---

# Architecture Overview

Chronova CLI is a Rust binary and library (`chronova-cli` crate) that tracks coding activity and sends it to a WakaTime-compatible backend. The architecture is async-first, offline-first, and trait-based for testability.

## Module responsibilities

The crate root is `src/lib.rs`, which declares and re-exports the public modules. `src/main.rs` is the binary entry point and consumes the library via `chronova_cli::*`.

| Module | Source | Responsibility |
|--------|--------|----------------|
| `main.rs` | `src/main.rs` | `tokio::main` entry point, CLI parse, flag routing, logging setup |
| `cli` | `src/cli.rs` | `clap::Parser` struct with all WakaTime-compatible flags |
| `config` | `src/config.rs` | INI file parsing, path resolution, config precedence |
| `heartbeat` | `src/heartbeat.rs` | Heartbeat creation, ignore rules, queue interaction |
| `queue` | `src/queue.rs` | SQLite-based persistent queue with `QueueOps` trait |
| `sync` | `src/sync.rs` | Sync status model, retry strategy, sync manager trait |
| `api` | `src/api.rs` | HTTP client, auth wrappers, rate-limit handling |
| `collector` | `src/collector.rs` | Project, git, and language detection |
| `logger` | `src/logger.rs` | `tracing` setup with file / stdout output; default log file `~/.chronova.log` |
| `user_agent` | `src/user_agent.rs` | User-Agent string generation |
| `updater` | `src/updater.rs` | GitHub release lookup and self-update (uses `reqwest`, `serde`, `tokio::process::Command`) |

## Dependency graph

The library crate `src/lib.rs` declares all modules and re-exports the public surface. The binary `src/main.rs` consumes those re-exports. Modules are connected as follows:

| Consumer | Depends on |
|---|---|
| `main.rs` | `Cli` (`cli.rs`), `Config` (`config.rs`), `ApiClient` (`api.rs`), `HeartbeatManager` / `HeartbeatManagerExt` (`heartbeat.rs`), `logger.rs`, `Updater` (`updater.rs`) |
| `heartbeat.rs` | `Cli`, `Config`, `ApiClient` / `AuthenticatedApiClient` (`api.rs`), `Queue` (`queue.rs`), `DataCollector` (`collector.rs`), `user_agent.rs`, `sync.rs` (status model + `SyncResult`) |
| `queue.rs` | `heartbeat.rs` (`Heartbeat`), `sync.rs` (`SyncStatus`) |
| `sync.rs` | `api.rs`, `heartbeat.rs` (`Heartbeat`), `queue.rs` (`Queue`, `QueueOps`) |
| `api.rs` | `heartbeat.rs` (`Heartbeat`) |
| `updater.rs` | `reqwest`, `tokio::process::Command`, `serde` |

## Critical paths

### Heartbeat flow

1. **CLI parse** — `main.rs` parses `Cli` via `clap`.
2. **Config load** — `Config::load()` reads `~/.chronova.cfg` and merges with CLI overrides.
3. **Heartbeat creation** — `HeartbeatManager::process()` builds a `Heartbeat` from CLI args plus auto-detected project, git, and language data.
4. **Queue first** — The heartbeat is written to SQLite via `QueueOps::add()` in a `spawn_blocking` task (offline-first strategy).
5. **Sync attempt** — `HeartbeatManager::process_queue()` fetches pending entries and sends them to the API, first as a batch and then individually on non-rate-limit failures.

### Sync flow

1. `HeartbeatManager::process_queue()` in `src/heartbeat.rs` retrieves pending heartbeats (batch size 50 by default).
2. Retry-eligible `Failed` entries are promoted back to `Pending`.
3. Entries are sent to the API via `AuthenticatedApiClient` or `ApiClient`, first as a batch and then individually if the batch fails for a non-rate-limit reason.
4. Successful entries are marked `Synced` and removed from the queue; failures are retried up to the configured limit.

### Error flow

1. Fallible functions return `anyhow::Result` or a typed `thiserror` enum (`ConfigError`, `ApiError`, `QueueError`, `UpdaterError`, `SyncError`).
2. `tracing::error!` records the failure path.
3. The error propagates up to `main.rs`, which prints a user-friendly message and exits.

## Design patterns

### Offline-first queue

Heartbeats are always written to SQLite first. Sync happens asynchronously after queuing, so editor activity is never lost during network outages.

The queue is implemented in `src/queue.rs` via `Queue` (which implements `QueueOps`). It stores data in `~/.chronova/queue.db`, enables WAL journal mode (`journal_mode=WAL`, `synchronous=NORMAL`), opens the database with corruption handling (backup + recreate on integrity failure), and initializes/migrates the following schema:

- `heartbeats` table: `id TEXT PRIMARY KEY`, `data TEXT` (serialized `Heartbeat`), `sync_status TEXT` (default `pending`), `sync_metadata TEXT`, `retry_count INTEGER` (default 0), `created_at DATETIME` (default `CURRENT_TIMESTAMP`), `last_attempt DATETIME`.
- `schema_version` table: tracks applied migrations (migration v1 adds `sync_status` and `sync_metadata`).
- Indexes on `sync_status`, `created_at`, and `retry_count`.

Default sync configuration (from `SyncConfig::default()` in `src/sync.rs`):

- `enabled`: `true`
- `max_queue_size`: 1000
- `batch_size`: 50
- `sync_interval_seconds`: 300 (5 minutes)
- `max_retry_attempts`: 5
- `retry_base_delay_seconds`: 1
- `retry_max_delay_seconds`: 60
- `retry_use_jitter`: `true`
- `retention_days`: 7
- `background_sync`: `true`


`QueueOps` methods include `add`, `add_batch`, `get_pending`, `update_sync_status`, `remove`, `count_by_status`, `get_sync_stats`, `cleanup_old_entries`, `enforce_max_count`, `deduplicate`, `vacuum`, `increment_retry`, `get_retry_count`, and `count`. Override defaults in `~/.chronova.cfg` with keys such as `sync_enabled`, `sync_max_retries`, `sync_retry_base_delay`, `sync_retry_max_delay`, `sync_interval`, `sync_retry_use_jitter`, `sync_max_queue_size`, `sync_retention_days`, and `sync_background`.

> Note: the current `HeartbeatManager` runs sync inline in `process()` rather than using the separate `ChronovaSyncManager` background task. `ChronovaSyncManager` is present in `src/sync.rs` but is not wired into the default heartbeat flow.

### Trait-based operations

- `QueueOps` defines the contract for queue storage, making the queue mockable in tests.
- `HeartbeatManagerExt` adds offline-first and manual sync methods to `HeartbeatManager`.
- `SyncManager` / `ChronovaSyncManager` defines a sync abstraction and background-sync implementation in `src/sync.rs`. It is not currently wired into the default heartbeat flow; `HeartbeatManager::process_queue()` performs inline sync instead.

### Error handling

- Custom error enums use `thiserror` v2.x (`ConfigError`, `ApiError`, `QueueError`, `UpdaterError`, `SyncError`).
- Application functions return `anyhow::Result` and propagate errors with `?`.
- `main.rs` maps errors to clean exit messages.

### Async + blocking isolation

- `tokio` runs the main async runtime.
- All SQLite work is wrapped in `tokio::task::spawn_blocking` to avoid blocking async worker threads.
- Shared sync-manager state uses `tokio::sync::RwLock` and atomics where needed.

## Public API surface

`src/lib.rs` re-exports the commonly used types:

- `ApiClient`
- `Cli`
- `Config`
- `HeartbeatManager`
- `Queue`
- `Updater`

These re-exports are what `main.rs` and external consumers use. The full module tree remains accessible if needed.

## Related pages

- [Heartbeat Flow](../heartbeat/index.md)
- [Configuration](../configuration/index.md)
- [Offline & Sync Behavior](../operations/offline-sync.md)
- [Development Guide](../development/index.md)
- [Logging & Updates](../operations/logging-updates.md)
