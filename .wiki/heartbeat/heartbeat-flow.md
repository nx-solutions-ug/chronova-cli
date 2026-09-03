---
type: guide
title: Heartbeat Flow
description: How a heartbeat travels from editor invocation through the CLI to the SQLite queue and the API.
tags: [heartbeat, cli, queue, sync, flow]
---

# Heartbeat Flow

This page walks one heartbeat from the moment an editor calls the CLI to the moment it lands in the local SQLite queue (and, optimistically, on the API).

For the module map see [Architecture Overview](../architecture/overview.md); for the offline/retry behavior when the network call fails, see [Offline & Sync Behavior](../operations/offline-sync.md).

## 1. Entry: `main.rs` selects a command

`main()` is a `#[tokio::main]` async function that does almost nothing besides parse `Cli`, look at flags, and delegate. The flag order in `src/main.rs` matters because some flags short-circuit before others.

Routing in `src/main.rs` (top to bottom, each one an early return):

| Flag | Behavior | Reference |
| --- | --- | --- |
| `--version` | Prints `chronova-cli v<CARGO_PKG_VERSION>` and exits. | `src/main.rs` |
| `--today` | Sets up logging (suppressing stdout when `--output json\|raw-json`), loads config, calls `fetch_today_activity()`. | `src/main.rs`, `src/api.rs` |
| `--config-read <key>` / `--config-write <key> <value>` | Reads or writes a single key in the config file (default section `[settings]`, overridable with `--config-section`). | `src/main.rs`, `src/config.rs` |
| `--offline-count` | Prints queue stats: `total`, `pending`, `syncing`, `synced`, `failed`, `permanent failures`. | `src/main.rs`, `src/heartbeat.rs` (`get_queue_stats`) |
| `--file-experts` / `--today-goal <id>` | Placeholder: returns a "not yet implemented" error. | `src/main.rs` |
| `--check-update` | Calls `Updater::check_for_update`, prints version + download URL or "up to date". | `src/main.rs`, `src/updater.rs` |
| `--self-update` | Calls `Updater::check_and_update`, downloads and atomically replaces the running binary on success. | `src/main.rs`, `src/updater.rs` |
| `--user-agent` | Internal: prints the UA string the binary would send, then exits. | `src/main.rs`, `src/user_agent.rs` |
| `--extra-heartbeats` | Reads a JSON array of heartbeats from STDIN until EOF. | `src/main.rs`, `src/heartbeat.rs` |
| `--sync-offline-activity <n>` / `--force-sync` | Drives `HeartbeatManagerExt::manual_sync`. | `src/heartbeat.rs` |
| (default, `--entity` required) | The normal heartbeat path — the rest of this page. | `src/main.rs`, `src/heartbeat.rs` |

After config load, `main.rs` overlays CLI values onto the `Config` struct: `--api-url` and the git-privacy flags (`--disable-git-info`, `--hide-commit-hash`, `--hide-commit-author`, `--hide-commit-message`, `--hide-repository-url`). If `auto_update = true` in config, a background task spawns an update check before the heartbeat path runs.

## 2. Build: `HeartbeatManager::process`

Once routing has decided "this is a heartbeat", `src/main.rs` constructs a `HeartbeatManager` and calls `.process(cli)`. Inside `src/heartbeat.rs`:

1. **Filter** — `should_ignore_entity(&entity)` matches the entity against `Config::ignore_patterns` (populated from the `exclude` config key and `--exclude`). Patterns ending in `$` are suffix matches, `*.`-prefixed patterns match file extensions, anything else is a substring match. A match bails out with a debug log.
2. **Time** — if `--time` was passed, that float is the timestamp; otherwise `chrono::Utc::now().timestamp_millis() as f64 / 1000.0`.
3. **Collect project context** — `DataCollector::detect_project(&entity)` walks up from the entity path looking for project markers (`.wakatime-project`, `package.json`, `Cargo.toml`, `pyproject.toml`, `.git`). It handles git worktrees, resolving to the main repository path when appropriate. Source: `src/collector.rs`.
4. **Collect git context** — `DataCollector::detect_git_info(&entity)` reads branch, commit hash / author / message, and the (credential-stripped) remote URL via `git2`. Inside a worktree, the branch reflects the worktree's HEAD.
5. **Collect language** — `DataCollector::detect_language(&entity)` looks up by special filename (Dockerfile, Makefile, ...), multi-part extensions, then final extension.
6. **Resolve priorities** — `project` resolves as `--project` > `--alternate-project` > auto-detected. `branch` and `language` resolve as CLI flag > auto-detected value.
7. **User agent** — `generate_user_agent(cli.plugin.as_deref())` builds a WakaTime-style string: `chronova/{version} ({os}-{core}-{platform}) {runtime} {plugin}`. With no plugin passed, the CLI token is duplicated: `chronova-cli/{version} chronova-cli/{version}`.
8. **Privacy flags** — if `Config::disable_git_info` is true, or any of the per-field `hide_commit_*` / `hide_repository_url` flags is true, the corresponding field is forced to `None` even if git info was detected.
9. **Construct the `Heartbeat`** with a fresh UUID, the resolved fields, `editor = None` and `operating_system = None` (populated API-side, not here), and an empty `dependencies` list.

## 3. Persist: queue first, network second

This is the key invariant: **the heartbeat is written to SQLite before any network call**. From `HeartbeatManager::process` in `src/heartbeat.rs`:

```rust
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
```

Two consequences:

- The blocking `rusqlite` call runs on a blocking pool thread, so the async runtime is never starved. This convention is explicit in `AGENTS.md`.
- A network failure cannot lose the heartbeat — the queue already has it. The retry path is `process_queue`'s job, not `process`'s.

## 4. Sync attempt: `process_queue`

`process_queue` (in `src/heartbeat.rs`) batches pending heartbeats and ships them to the API. Full detail lives in [Offline & Sync Behavior](../operations/offline-sync.md); the shape:

- In a single blocking task: promote retry-eligible `Failed` entries back to `Pending` (retry count below 3), then fetch a batch of `Pending` entries (batch size 50).
- If more than one heartbeat is pending, try a batch send via `AuthenticatedApiClient::send_heartbeats_batch` (falling back to `ApiClient` when no API key is configured).
- On success, mark each heartbeat `Synced` and remove it from the queue.
- On rate limit, sleep 60 seconds and retry the batch on the next loop iteration.
- On any other batch error, fall back to per-heartbeat sends so failures are handled granularly.
- Per-heartbeat rate limits back off exponentially (`2^min(retry_count, 6) * 5` seconds) and retry once before counting as failed.
- Failures increment the retry counter; at 3 attempts an entry becomes `PermanentFailure` and stops being retried.

The `process` call returns once the immediate batch has settled; entries still in `Failed` are picked up by a later `process` call or a manual `--sync-offline-activity` run.

## 5. Side paths that re-use the same building blocks

- **`--extra-heartbeats`** reads a JSON array of `Heartbeat` values from STDIN and queues each through the same offline-first path. A relaxed parse accepts the WakaTime extension shape without an `id` field — missing IDs are generated as fresh UUIDs, and `type` defaults to `file`.
- **`--sync-offline-activity <n>`** drives `HeartbeatManagerExt::manual_sync`, which runs `process_queue` and reports synced/failed counts.
- **`--offline-count`** is read-only: it calls `HeartbeatManager::get_queue_stats` and prints bucket counts without sending anything.

## Where to start when changing the flow

- **New field on `Heartbeat`** → add to the struct in `src/heartbeat.rs`, update the SQLite schema and migrations in `src/queue.rs`, update JSON serialization, and update the API request payload. See `AGENTS.md`, "Adding a New Heartbeat Field".
- **New step before queueing** → add it in `process` *before* the `spawn_blocking` block; do not block the runtime inside the async function.
- **New step after queueing** → either inline in `process` or in `process_queue`, depending on whether it should be retried with the heartbeat.
- **New CLI flag** → add to `Cli` in `src/cli.rs`, route in `src/main.rs`, consume in `HeartbeatManager::process`. See `AGENTS.md`, "Adding a New CLI Flag".

## Related pages

- [Architecture Overview](../architecture/overview.md)
- [Offline & Sync Behavior](../operations/offline-sync.md)
- [API Compatibility](../api-compatibility/index.md)
- [Configuration](../configuration/index.md)