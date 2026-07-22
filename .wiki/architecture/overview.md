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
| `logger` | `src/logger.rs` | `tracing` setup with file / stdout output |
| `user_agent` | `src/user_agent.rs` | User-Agent string generation |
| `updater` | `src/updater.rs` | GitHub release lookup and self-update |

## Dependency graph

```
main.rs
├── cli.rs
├── config.rs
└── heartbeat.rs
    ├── api.rs
    ├── collector.rs
    ├── queue.rs
    │   └── sync.rs (status model)
    └── user_agent.rs

logger.rs — used by main.rs for tracing setup
updater.rs — used by main.rs for --check-update and --self-update
```

## Critical paths

### Heartbeat flow

1. **CLI parse** — `main.rs` parses `Cli` via `clap`.
2. **Config load** — `Config::load()` reads `~/.chronova.cfg` and merges with CLI overrides.
3. **Heartbeat creation** — `HeartbeatManager::process()` builds a `Heartbeat` from CLI args plus auto-detected project, git, and language data.
4. **Queue first** — The heartbeat is written to SQLite via `QueueOps::add()` in a `spawn_blocking` task (offline-first strategy).
5. **Sync attempt** — `HeartbeatManager::process_queue()` fetches pending entries and sends them to the API.

### Sync flow

1. `Queue::process_queue` retrieves pending heartbeats (batch size 50 by default).
2. Retry-eligible failed entries are promoted back to `Pending`.
3. Entries are sent to the API individually or in batches.
4. Successful entries are removed from the queue; failures are retried up to the configured limit.

### Error flow

1. Any fallible function returns `anyhow::Result` or a typed `thiserror` enum.
2. `tracing::error!` records the failure path.
3. The error propagates up to `main.rs`, which prints a user-friendly message and exits.

## Design patterns

### Offline-first queue

Heartbeats are always written to SQLite first. Sync happens asynchronously after queuing, so editor activity is never lost during network outages. The queue supports batched reads, status tracking, retries, and cleanup.

### Trait-based operations

- `QueueOps` defines the contract for queue storage, making the queue mockable in tests.
- `HeartbeatManagerExt` adds offline-first and manual sync methods to `HeartbeatManager`.
- `SyncManager` / `ChronovaSyncManager` separates the sync abstraction from the implementation.

### Error handling

- Custom error enums use `thiserror` (e.g., `ConfigError`, `ApiError`, `QueueError`, `UpdaterError`).
- Application functions return `anyhow::Result` and propagate errors with `?`.
- `main.rs` maps errors to clean exit messages.

### Async + blocking isolation

- `tokio` runs the main async runtime.
- All SQLite work is wrapped in `tokio::task::spawn_blocking` to avoid blocking async worker threads.
- Shared state uses `tokio::sync::RwLock` where needed.

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

- [Heartbeat Flow](./heartbeat/index.md)
- [Configuration](../configuration/index.md)
- [Offline & Sync Behavior](../operations/offline-sync.md)
- [Development Guide](../development/index.md)
