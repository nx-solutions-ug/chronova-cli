# Heartbeat flow

This page walks one heartbeat from the moment an editor calls the CLI to the moment it lands in the local SQLite queue (and, optimistically, on the API). The point is to make the "what runs when" answerable from the code without re-reading `main.rs`.

For the broader module map see [architecture.md](architecture.md); for the offline / retry behavior that kicks in when the network call fails, see [sync-and-offline.md](sync-and-offline.md).

## 1. Entry: `main.rs` selects a command

`main()` is a `#[tokio::main]` async function that does almost nothing besides parse `Cli`, look at flags, and delegate. The flag order in `src/main.rs` matters because some flags short-circuit before others.

Routing in `src/main.rs` (top to bottom, each one an early-return):

| Flag | Behavior | Reference |
| --- | --- | --- |
| `--version` | Prints `chronova-cli v<CARGO_PKG_VERSION>` and exits. | `src/main.rs` |
| `--today` | Sets up logging (suppressing stdout when `--output json\|raw-json`), loads config, calls `fetch_today_activity()`. | `src/main.rs`, `src/api.rs` |
| `--config-read <key>` / `--config-write <key> <value>` | Reads or writes a single key in `~/.chronova.cfg` via `handle_config_operations`. | `src/main.rs`, `src/config.rs` |
| `--offline-count` | Prints the queue stats: `total`, `pending`, `syncing`, `synced`, `failed`, `permanent_failures`. | `src/main.rs`, `src/heartbeat.rs` (`get_queue_stats`) |
| `--file-experts` / `--today-goal <id>` | Placeholder: returns `anyhow::anyhow!("…not yet implemented")`. | `src/main.rs` |
| `--check-update` | Calls `Updater::check_for_update`, prints version + download URL or "up to date". | `src/main.rs`, `src/updater.rs` |
| `--self-update` | Calls `Updater::check_and_update`, downloads and atomically replaces the running binary on success. | `src/main.rs`, `src/updater.rs` |
| `--user-agent` | Internal: print the UA string the binary would send, then exit. | `src/main.rs`, `src/user_agent.rs` |
| `--extra-heartbeats` | Reads a JSON array of heartbeats from STDIN until EOF. | `src/main.rs`, `src/heartbeat.rs` |
| `--sync-offline-activity <n>` / `--force-sync` | Drives `HeartbeatManagerExt::manual_sync`. | `src/heartbeat.rs`, `src/sync.rs` |
| (default, `--entity` required) | The normal heartbeat path — what the rest of this page describes. | `src/main.rs`, `src/heartbeat.rs` |

A heartbeat can also be injected in bulk via `--extra-heartbeats` (STDIN JSON array) or queued by the previous offline runs being drained by `--sync-offline-activity`.

## 2. Build: `HeartbeatManager::process`

Once routing has decided "this is a heartbeat", `src/main.rs` constructs a `HeartbeatManager` and calls `.process(cli)`. Inside `src/heartbeat.rs`:

1. **Filter** — `should_ignore_entity(&entity)` matches against `Config::ignore_patterns` (and friends) and bails with a debug log if it matches.
2. **Time** — if `--time` was passed, that float is the timestamp; otherwise `chrono::Utc::now().timestamp_millis() as f64 / 1000.0`.
3. **Collect project context** — `DataCollector::detect_project(&entity)` walks up from the entity path looking for project markers (`.wakatime-project`, `package.json`, `Cargo.toml`, `pyproject.toml`, etc.). It also handles git worktrees, resolving to the main repo path when appropriate. Source: `src/collector.rs`.
4. **Collect git context** — `DataCollector::detect_git_info(&entity)` reads branch, commit hash / author / message, and the (credential-stripped) `origin` URL. Uses `git2` (libgit2) with vendored OpenSSL per `Cargo.toml`. Inside a worktree, the branch reflects the worktree.
5. **Collect language** — `DataCollector::detect_language(&entity)` looks up by filename and extension, with multi-part extensions handled.
6. **Resolve priorities** — for `project`, `branch`, and `language`, the CLI flag wins; if absent, the auto-detected value wins. `--alternate-project` is the fallback project if `--project` is not given.
7. **User agent** — `generate_user_agent(cli.plugin.as_deref())` builds a WakaTime-style string: `chronova/{version} ({os}-{core}-{platform}) {runtime} {plugin}`. If no plugin is passed, the CLI token is duplicated to look like a WakaTime token.
8. **Privacy flags** — if `Config::disable_git_info` is true, or any of the per-field `hide_commit_*` / `hide_repository_url` is true, the corresponding field is forced to `None` even if git info was detected.
9. **Construct the `Heartbeat`** with a fresh UUID, the resolved fields, `editor = None` and `operating_system = None` (those are populated by the API side, not here), and `dependencies: Vec::new>()`.

## 3. Persist: queue first, network second

This is the key invariant: **the heartbeat is written to SQLite before any network call**. The exact code in `HeartbeatManager::process`:

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

- The blocking `rusqlite` call runs on a blocking pool thread, so the async runtime is never starved. `AGENTS.md` is explicit about this convention.
- A network failure doesn't lose the heartbeat — the queue already has it. The retry path is then `process_queue`'s job, not `process`'s.

## 4. Sync attempt: `process_queue`

`process_queue` (in `HeartbeatManager` / `HeartbeatManagerExt`) batches pending heartbeats and ships them to the API. Details live in [sync-and-offline.md](sync-and-offline.md), but the shape is:

- Fetch a batch of `Pending` heartbeats from the queue.
- Try a batch send via `ApiClient::send_heartbeats_batch`.
- On success, mark each heartbeat `Synced` and remove it.
- On rate limit / network error, mark `Failed` and let the retry strategy back off.
- On auth / config errors, mark `PermanentFailure` — these will not be retried.

The `process` call returns once the immediate batch has settled; any heartbeats still in `Failed` are picked up later by background sync, `--force-sync`, or the next `process` call.

## 5. Side paths that re-use the same building blocks

- **`--extra-heartbeats`** reads a JSON array of `Heartbeat` values from STDIN and runs each through the same `process_queue` path. This is how a single editor invocation can pack multiple edits.
- **`--sync-offline-activity <n>`** drives `HeartbeatManagerExt::manual_sync` with a target count.
- **`--offline-count`** is read-only — it calls `HeartbeatManager::get_queue_stats` and prints the bucket counts without sending anything.

## Where to start when changing the flow

- New field on `Heartbeat` → add to the struct in `src/heartbeat.rs`, update the SQLite schema and migrations in `src/queue.rs`, update JSON serialization, and update the API request payload. See `AGENTS.md` "Adding a New Heartbeat Field".
- New step before queueing → add it to `process` *before* the `spawn_blocking` block; do not block the runtime inside the async function.
- New step after queueing → either inline in `process` or in `process_queue`, depending on whether it should be retried with the heartbeat.
- New CLI flag → add to `Cli` in `src/cli.rs`, route in `src/main.rs`, consume in `HeartbeatManager::process`. See `AGENTS.md` "Adding a New CLI Flag".
