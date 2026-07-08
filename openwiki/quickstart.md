# Chronova CLI — OpenWiki

This is the OpenWiki for `chronova-cli`, a high-performance, drop-in replacement for `wakatime-cli` written in Rust.

Start here, then follow the links to the area you care about.

---

## What this project is

Chronova CLI tracks coding activity by recording *heartbeats* (a file edit, a line of code, a project switch) and syncing them to a WakaTime-compatible backend. It is built in Rust with `tokio` for async I/O and `rusqlite` for a local offline queue, and is intended to be a transparent drop-in for the official `wakatime-cli` — the same CLI surface, the same config file, the same editor plugins.

- Package: `chronova-cli` (crate `chronova_cli`), version 1.3.4, Rust edition 2021. See `Cargo.toml`.
- Binary: `chronova-cli` from `src/main.rs`; library: `chronova_cli` from `src/lib.rs`.
- Default API: `https://chronova.dev/api/v1` (overridable).
- Default config: `~/.chronova.cfg` (same shape as WakaTime's `~/.wakatime.cfg`).
- Default log file: `~/.chronova/chronova.log`.
- Offline queue: SQLite database under `~/.chronova/queue.db`.

## Where to look

- **Quick first build & run** → [Build & run](#build--run) below.
- **Installers (Linux / macOS / Windows)** → [Installation](#installation) below.
- **How the code is organized** → [architecture.md](architecture.md).
- **What happens to a single heartbeat** → [heartbeat-flow.md](heartbeat-flow.md).
- **CLI flags and config file** → [cli-and-config.md](cli-and-config.md).
- **Queue, sync, retry, offline behavior** → [sync-and-offline.md](sync-and-offline.md).
- **Tests and how to run them** → [testing.md](testing.md).
- **CI, release, installers, self-update, automation** → [operations.md](operations.md).

## Existing docs to prefer

This OpenWiki summarizes the project. For deeper material, prefer the in-repo docs:

- [`README.md`](../README.md) — user-facing overview, CLI examples, install commands, basic architecture diagram.
- [`INSTALL.md`](../INSTALL.md) — full installation walkthrough, what the installer does, verification, troubleshooting.
- [`docs/ARCHITECTURE.md`](../docs/ARCHITECTURE.md) — long-form module-by-module architecture, design patterns, data structures, command flow. Use this when you need the details behind a page in this OpenWiki.
- [`docs/editor-integration.md`](../docs/editor-integration.md) — how to wire VS Code / JetBrains / Vim / Sublime / Atom / Emacs to the CLI.
- [`docs/migration-guide.md`](../docs/migration-guide.md) — moving a setup from WakaTime to Chronova.
- [`docs/plans/2026-02-26-code-review-fixes.md`](../docs/plans/2026-02-26-code-review-fixes.md) — historical plan with the rationale for several recent code-review fixes.
- [`AGENTS.md`](../AGENTS.md) — agent-facing guidance (error handling, async, DB, config, testing conventions). Required reading before you start changing code.
- [`CHANGELOG.md`](../CHANGELOG.md) — release notes; the `[Unreleased]` section lists the sync/queue/retry/observability work that landed on top of the 0.1.0 baseline.

## Build & run

Prerequisites: Rust 1.70+, `cargo`.

```bash
# Development build
cargo build

# Release build (LTO, panic=abort, opt-level=z — see Cargo.toml [profile.release])
cargo build --release

# Run a single heartbeat manually
cargo run -- --entity src/main.rs --language rust --write --verbose
```

Cross-compilation is driven by [`Cross.toml`](../Cross.toml) and [`build.sh`](../build.sh); Docker-based cross builds use [`build-docker.sh`](../build-docker.sh).

## Installation

The supported one-line installers live in the repo:

- Linux (glibc): `curl -fsSL https://raw.githubusercontent.com/nx-solutions-ug/chronova-cli/main/install-linux.sh | bash`
- Linux (musl, e.g. Alpine): `CHRONOVA_CLI_MUSL=true curl -fsSL .../install-linux.sh | bash`
- macOS: `curl -fsSL https://raw.githubusercontent.com/nx-solutions-ug/chronova-cli/main/install-macos.sh | bash`
- Windows (PowerShell): `irm https://raw.githubusercontent.com/nx-solutions-ug/chronova-cli/main/install-windows.ps1 | iex`

The installers detect architecture, back up any existing `~/.wakatime` folder, install to `~/.local/bin`, write a default `~/.chronova.cfg`, and create compatibility symlinks so existing WakaTime editor plugins keep working. See [`INSTALL.md`](../INSTALL.md) for the full list of supported asset names, what the installer touches, and post-install verification.

## Features at a glance

- **Offline-first queue** — every heartbeat is written to SQLite before any network call. Sync is retried with exponential backoff + jitter.
- **Drop-in WakaTime compatibility** — CLI flags, config keys, plugin UA format, and API request shape mirror `wakatime-cli` so existing editor integrations work unchanged.
- **Multiple auth methods** — Bearer, Basic Auth, and `X-API-Key` header (see `src/api.rs`).
- **Automatic detection** — project root (from `.wakatime-project`, `package.json`, `Cargo.toml`, etc.), git branch / commit / author / message / remote URL, and language from filename. See `src/collector.rs`.
- **Git worktree support** — `DataCollector` returns the correct branch and main-repo path when the entity lives inside a worktree.
- **Self-update** — `chronova-cli --check-update` and `--self-update` query the GitHub releases API and atomically replace the running binary. See `src/updater.rs`.
- **Structured logging** — `tracing` to a non-blocking file writer, with stdout suppressed when `--output json` is set so the JSON payload stays clean.
- **Privacy flags** — `--hide-file-names`, `--hide-project-names`, `--hide-branch-names`, `--hide-commit-hash`, `--hide-commit-author`, `--hide-commit-message`, `--hide-repository-url`, `--disable-git-info`, all mirrored in `Config` (`src/config.rs`).

## How a heartbeat moves through the system

```
CLI args (src/cli.rs)
        │
        ▼
main.rs routing  ──  --version / --today / --config-read|write
        │           ──  --offline-count / --file-experts / --today-goal
        │           ──  --check-update / --self-update / --user-agent
        │           ──  default: build a heartbeat
        ▼
HeartbeatManager::process  (src/heartbeat.rs)
        │   ├── should_ignore_entity (config patterns)
        │   ├── create_heartbeat    (CLI + collector + UA)
        │   ├── Queue::add          (spawn_blocking; SQLite first)
        │   └── process_queue       (batch send → API → status update)
        ▼
Chronova / WakaTime-compatible API  (src/api.rs)
```

Read [heartbeat-flow.md](heartbeat-flow.md) for the full walk-through, or [sync-and-offline.md](sync-and-offline.md) for what happens when the API is unreachable.

## Where to start when changing code

- New CLI flag → edit `Cli` in `src/cli.rs`, then add the handling in `src/main.rs`. See `AGENTS.md` "Adding a New CLI Flag".
- New config field → `Config` in `src/config.rs`, plus a getter and a default. See `AGENTS.md` "Adding a New Config Option".
- New heartbeat field → `Heartbeat` in `src/heartbeat.rs`, the schema in `src/queue.rs`, and the JSON serialization. See `AGENTS.md` "Adding a New Heartbeat Field".
- New API endpoint → `ApiClient` in `src/api.rs`; cover all auth methods. See `AGENTS.md` "Adding API Endpoints".
- New sync behavior → `SyncManager`/`ChronovaSyncManager` in `src/sync.rs` and the `QueueOps` trait in `src/queue.rs`.

Run `cargo fmt`, `cargo clippy -- -D warnings`, and `cargo test` before submitting. CI in `.github/workflows/test.yml` enforces all three on Linux, macOS, and Windows. See [testing.md](testing.md).
