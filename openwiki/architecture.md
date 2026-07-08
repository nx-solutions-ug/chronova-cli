# Architecture

This page summarizes how `chronova-cli` is laid out. It is the orientation map; for module-by-module detail (function signatures, schema layouts, line counts) see [`docs/ARCHITECTURE.md`](../docs/ARCHITECTURE.md), which this page intentionally does not duplicate.

## Crate shape

- One crate, two targets: a binary (`chronova-cli`, `src/main.rs`) and a library (`chronova_cli`, `src/lib.rs`). The library re-exports the modules that downstream code and tests need: `api`, `cli`, `collector`, `config`, `heartbeat`, `logger`, `queue`, `sync`, `updater`, `user_agent`. Re-exports at the crate root give the common entry points (`ApiClient`, `Cli`, `Config`, `HeartbeatManager`, `Queue`, `Updater`).
- All non-trivial state lives in library modules. `main.rs` is a thin router that selects the right command, sets up logging, loads config, and calls the appropriate entry point.

## Module map

```
main.rs ──┬─ cli.rs            (clap derive; 60+ flags)
          ├─ config.rs         (INI parser, defaults, getter methods)
          ├─ logger.rs         (tracing + non-blocking file writer)
          ├─ updater.rs        (GitHub release check + atomic replace)
          ├─ heartbeat.rs ─────┬─ api.rs       (reqwest client + auth)
          │                    ├─ queue.rs ─── sync.rs
          │                    ├─ collector.rs (project/git/language)
          │                    └─ user_agent.rs (WakaTime-style UA)
          └─ lib.rs            (re-exports)
```

The dependency direction is one-way: `main` and `cli`/`config`/`logger` are leaves; `heartbeat` is the orchestrator that pulls in the rest.

## Layering

There are three logical layers, even though they live in the same crate:

1. **Edge / I/O** — `cli.rs` (parse), `config.rs` (load), `logger.rs` (tracing), `updater.rs` (GitHub API + atomic file replace).
2. **Domain** — `heartbeat.rs` (the `Heartbeat` value type + `HeartbeatManager` orchestrator), `collector.rs` (project/git/language detection from a path), `user_agent.rs` (UA string construction).
3. **Infrastructure** — `queue.rs` (SQLite-backed `QueueOps`), `sync.rs` (retry, status, sync manager), `api.rs` (HTTP client and response types).

This layering is informal — `heartbeat` reaches into all three — but it explains why every persistent piece of state (queue, sync, api client, config) is owned by `HeartbeatManager`.

## Key abstractions

These three traits are the seams that make the code testable and that the sync path depends on:

- **`QueueOps`** (`src/queue.rs`) — every operation the offline queue exposes: `add`, `add_batch`, `get_pending`, `remove`, `update_sync_status`, `count_by_status`, `get_sync_stats`, `cleanup_old_entries`, `enforce_max_count`, `vacuum`, plus dedup/retry helpers. `Queue` is the SQLite implementation; everything that touches persistence goes through this trait.
- **`SyncManager`** (`src/sync.rs`) — async trait driving the sync loop. `ChronovaSyncManager` is the production implementation, with `SyncResult`, `SyncStatusSummary`, `SyncError`, and a `RetryStrategy` (exponential backoff with jitter) supporting it.
- **`HeartbeatManagerExt`** (`src/heartbeat.rs`) — extension trait on `HeartbeatManager` that adds offline-first behavior (`process_offline_first`), queue stats, and manual sync. Default impls keep the core struct small.

## Design decisions that show up everywhere

- **Offline-first.** A heartbeat is *always* written to SQLite first, then an attempt is made to sync. The "offline" path is therefore the *normal* path; the online path is the optimization. See `HeartbeatManager::process` in `src/heartbeat.rs`.
- **Trait-based design over inheritance.** The three traits above let tests substitute in-memory fakes (see `tests/` for examples) and let the sync pipeline compose with new backends later.
- **Async runtime is `tokio`, SQLite work runs on `spawn_blocking`.** The `tokio` features are enabled with `full` in `Cargo.toml`. The queue add path explicitly uses `tokio::task::spawn_blocking` so the async runtime is never blocked on a sync SQLite call.
- **Errors are split between `thiserror` (typed, for libraries) and `anyhow` (opaque, for `main`).** Library enums: `ConfigError`, `QueueError`, `ApiError`, `SyncError`, `UpdaterError`. `main` returns `anyhow::Result` and uses `unwrap_or_else` + `process::exit(1)` to print a clean message and bail.
- **Blocking I/O is hidden behind `tokio::sync::RwLock` and `Arc<Atomic…>`.** The sync manager tracks connectivity state with `Arc<AtomicBool>` and last-check time with `Arc<RwLock<Option<SystemTime>>>`.
- **WakaTime compatibility at every boundary.** CLI flag names, config keys, plugin UA format (`chronova/{version} ({os}-{core}-{platform}) {runtime} {plugin}` — see `src/user_agent.rs`), and the API request/response shapes are all designed so existing editor plugins keep working. The WakaTime configuration compatibility test in `tests/config_wakatime_compatibility.rs` enforces this.

## Key data types

- **`Heartbeat`** (`src/heartbeat.rs`) — the unit of work. Fields include `id` (UUID), `entity`, `entity_type` (`file` / `domain` / `url` / `app`), `time` (unix seconds), `project`, `branch`, `language`, `is_write`, `lines`, `lineno`, `cursorpos`, `user_agent`, `category`, `machine`, optional `editor` / `operating_system`, `commit_*` fields, and `dependencies: Vec<String>`. Serialized with `serde`; the `type` field is renamed from `entity_type`.
- **`Config`** (`src/config.rs`) — every option the CLI respects. Includes auth, proxy, the family of `hide_*` / `disable_*` privacy flags, network settings, and a nested `SyncConfig`.
- **`SyncStatus`** (`src/sync.rs`) — `Pending | Syncing | Synced | Failed | PermanentFailure`. Stored on every queue row; drives the retry loop.
- **`QueueEntry`** / **`QueueStats`** (`src/queue.rs`) — a `Heartbeat` plus sync metadata (`sync_status`, `sync_metadata`, `retry_count`, `created_at`, `last_attempt`).
- **`ApiClient` / `AuthenticatedApiClient`** (`src/api.rs`) — base HTTP client (30 s timeout) and a wrapper that holds auth. Auth modes: Bearer, Basic, `X-API-Key`.
- **`ProjectInfo` / `GitInfo`** (`src/collector.rs`) — the outputs of the data collector. `GitInfo` correctly reflects the worktree's branch when the entity is inside a `git worktree` (see the module-level rustdoc in `src/collector.rs`).

## Where to look next

- [heartbeat-flow.md](heartbeat-flow.md) — end-to-end path of a single heartbeat.
- [sync-and-offline.md](sync-and-offline.md) — what `SyncManager`, the retry strategy, and the queue schema do.
- [cli-and-config.md](cli-and-config.md) — how flags map to behavior, and the config file shape.
- [operations.md](operations.md) — how the binary is built, released, and updated.
- [testing.md](testing.md) — how the trait seams are exercised in tests.
