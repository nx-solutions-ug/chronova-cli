---
type: reference
title: Development Guide
description: Building, testing, CI gates, release automation, and contribution conventions for Chronova CLI.
tags: [development, build, test, ci, release, contributing]
---

# Development Guide

How to build, test, and ship changes to Chronova CLI — plus the conventions the codebase expects.

## Build

Requires Rust 1.70+.

```bash
git clone https://github.com/nx-solutions-ug/chronova-cli.git
cd chronova-cli
cargo build --release
# Binary at target/release/chronova-cli
```

The crate is both a binary (`src/main.rs`) and a library (`src/lib.rs`, which re-exports `ApiClient`, `Cli`, `Config`, `HeartbeatManager`, `Queue`, `Updater`). The release profile enables `lto = true`, `panic = "abort"`, and `opt-level = "z"` for a compact binary. For cross-target builds, `cross` is used with `Cross.toml` passing `CARGO_NET_GIT_FETCH_WITH_CLI=true`.

## Project layout

| Path | Contents |
| --- | --- |
| `src/main.rs` | Binary entry point, flag routing, `fetch_today_activity`, config read/write ops, extra-heartbeats ingestion |
| `src/cli.rs` | `clap` derive struct with all WakaTime-compatible flags |
| `src/config.rs` | INI config parsing, path resolution, precedence |
| `src/heartbeat.rs` | `Heartbeat` struct, `HeartbeatManager`, `process`, `process_queue` |
| `src/queue.rs` | SQLite queue, `QueueOps` trait, schema + migrations |
| `src/sync.rs` | `SyncStatus`, `RetryStrategy`, `SyncConfig`, `SyncManager` / `ChronovaSyncManager` |
| `src/api.rs` | HTTP client, auth fallback chain, response types |
| `src/collector.rs` | Project / git / language detection, worktree handling |
| `src/logger.rs` | `tracing` setup; default log file `~/.chronova.log` |
| `src/user_agent.rs` | WakaTime-style User-Agent generation |
| `src/updater.rs` | GitHub release lookup and self-update |
| `tests/` | Integration test crates + `tests/unit` |

See [Architecture Overview](../architecture/overview.md) for the dependency graph and design patterns.

## Testing

```bash
# Everything
cargo test

# Only library unit tests (fast, no network)
cargo test --lib

# One integration target
cargo test --test heartbeat_sync
cargo test --test retry_mechanism_test
cargo test --test config_wakatime_compatibility

# Formatting & lint gates CI runs first
cargo fmt -- --check
cargo clippy -- -D warnings
```

CI (`.github/workflows/test.yml`) runs `cargo fmt --check`, `clippy -- -D warnings`, and `cargo test --verbose` on ubuntu, macos, and windows runners — all three must pass.

### Tooling conventions

- `tempfile` — every queue-touching test builds its queue in a `TempDir`; never touch the user's `~/.chronova/queue.db`.
- `wiremock` — stub the API; never hit a real backend from tests.
- `assert_cmd` + `predicates` — end-to-end CLI tests run the compiled binary.
- `tokio-test` — async test helpers.

### Where to add coverage

| Change | Test home |
| --- | --- |
| New CLI flag | `tests/cli_parsing.rs` (shape), `tests/integration/cli_commands.rs` (end-to-end). |
| New `Config` field | `tests/config_wakatime_compatibility.rs` (or `tests/sync_config.rs` for sync keys). |
| New `Heartbeat` field | `tests/heartbeat_sync.rs` + a wiremock assertion in `tests/integration/network_failures.rs`. |
| New `SyncStatus` / retry behavior | `tests/retry_mechanism_test.rs`. |
| Queue schema change | `tests/error_recovery_test.rs` + a migration test. |

`tests/integration/sync_operations.rs` is currently a placeholder; grow it with `assert_cmd` + `wiremock` if you need end-to-end sync coverage.

## Code conventions

- Errors: `thiserror` enums for library boundaries (`ConfigError`, `ApiError`, `QueueError`, `SyncError`, `UpdaterError`), `anyhow::Result` internally, `?` for propagation, `tracing::error!` on failure paths — never `println!`.
- Async: wrap SQLite (`rusqlite`) calls in `tokio::task::spawn_blocking`; shared mutable state uses `tokio::sync::RwLock` / atomics.
- Database: all queue access goes through the `QueueOps` trait; batch writes use transactions; schema changes need a migration in `src/queue.rs`.
- Config: respect CLI > file > defaults; expand `~` with the path-resolution helpers.
- Public APIs get rustdoc comments; keep functions small and trait-based.

## Release automation

Releases run through `.github/workflows/release.yml`, triggered by `workflow_dispatch` (optionally with a forced version):

1. `semantic-release` (config in `.github/release-tooling/release.config.js`) determines the next version from commit messages; a `force_version` input skips this.
2. `Cargo.toml` / `Cargo.lock` are bumped and committed (`chore(release): bump version to v.X.Y.Z`).
3. The matrix builds 8 targets — `x86_64`/`aarch64` × `gnu`/`musl` Linux, `x86_64`/`aarch64` macOS, `x86_64`/`aarch64` Windows — natively or via `cross`.
4. Artifacts are packaged as `chronova-cli-v.{version}-{target}.tar.gz` (Unix) or `.zip` (Windows) and published on the `v.{version}` tag (note the dot after `v`).

`--check-update` and `--self-update` in `src/updater.rs` consume exactly this layout. See [Logging & Updates](../operations/logging-updates.md) for the updater's atomic replacement mechanics.

## Wiki automation

`.github/workflows/update-wiki.yml` regenerates `.wiki/` daily (cron 08:00 UTC), on `main` pushes, and on demand, then opens a staging PR with the changed wiki content. Files under `.wiki/` are generated — edit them via the regeneration pipeline, not by hand, unless fixing up like-for-like content as done here.

## Contributing

PRs are welcome from vouched contributors — see [`CONTRIBUTING.md`](https://github.com/nx-solutions-ug/chronova-cli/blob/main/CONTRIBUTING.md). Bots and write-access collaborators are exempt from vouching. Before submitting: `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt`, no compiler warnings, and a manual test of the changed functionality.

## Related pages

- [Architecture Overview](../architecture/overview.md)
- [Heartbeat Flow](../heartbeat/index.md)
- [Offline & Sync Behavior](../operations/offline-sync.md)
- [Logging & Updates](../operations/logging-updates.md)