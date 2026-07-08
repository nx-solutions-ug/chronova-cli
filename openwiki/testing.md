# Testing

Tests live under [`tests/`](../tests). The crate is a binary + library, so the layout is a mix of integration tests (each `.rs` file in `tests/` is its own crate) and the `tests/unit` and `tests/integration` subfolders for more substantial test modules. This page explains how the suite is organized, how to run it, and the conventions new tests should follow.

For the conventions a new test should follow, see [`AGENTS.md`](../AGENTS.md) (the "Testing" section) and the `open-mem` notes in [`tests/AGENTS.md`](../tests/AGENTS.md).

## Test layout

```
tests/
├── AGENTS.md                    # recent test-activity notes (auto-generated)
├── mod.rs                       # shared helpers
├── cli_offline_commands.rs      # offline CLI subcommands (--offline-count, etc.)
├── cli_parsing.rs               # CLI flag parsing
├── config_wakatime_compatibility.rs  # INI config compatibility with WakaTime
├── error_recovery_test.rs       # queue corruption recovery
├── extra_heartbeats_test.rs     # --extra-heartbeats (STDIN) path
├── heartbeat_sync.rs            # HeartbeatManager end-to-end
├── observability.rs             # tracing / metrics output
├── performance_test.rs          # throughput & latency
├── retry_mechanism_test.rs      # RetryStrategy, sync retry
├── sync_config.rs               # SyncConfig parsing & defaults
├── wakatime_compatibility.rs    # WakaTime-shape compatibility
├── worktree_test.rs             # git worktree detection
├── integration/
│   ├── AGENTS.md
│   ├── cli_commands.rs          # full-CLI tests via assert_cmd
│   ├── network_failures.rs      # wiremock scenarios
│   ├── offline_storage.rs       # queue & storage limits
│   ├── storage_limits.rs
│   └── sync_operations.rs       # (currently placeholder)
└── unit/
    ├── AGENTS.md
    └── mod.rs                   # per-module unit tests
```

`tests/mod.rs` is the common helpers module (not a test target by itself). The folder structure is the result of incremental refactors — the auto-generated `AGENTS.md` files in `tests/`, `tests/integration/`, and `tests/unit/` show recent churn (e.g. race-condition fixes, isolated queues, deduplication of `PermanentFailure` work).

## How to run

```bash
# Everything, with output
cargo test -- --nocapture

# Only library unit tests (fast, no network)
cargo test --lib

# Only integration tests (one binary per .rs file)
cargo test --test cli_parsing
cargo test --test heartbeat_sync
cargo test --test retry_mechanism_test

# Just the integration subfolder
cargo test --test cli_commands      # uses assert_cmd
cargo test --test network_failures  # uses wiremock

# Formatting & lint checks the CI runs first
cargo fmt -- --check
cargo clippy -- -D warnings
```

The CI job in [`.github/workflows/test.yml`](../.github/workflows/test.yml) runs the full matrix on `ubuntu-latest`, `macos-latest`, and `windows-latest`, so the same `cargo test` invocation must pass on all three.

## Tooling

- `tempfile = "3.8"` (dev-dep) — every test that touches the queue creates its own `TempDir` and points the queue there, so tests don't share a single `~/.chronova/queue.db`. The recent test-activity notes flag the previous race condition and the move to per-test queues as the fix; new tests must keep doing this.
- `wiremock = "0.6"` (dev-dep) — used by `tests/integration/network_failures.rs` (and any other HTTP-mock test) to stub the WakaTime/Chronova API.
- `assert_cmd = "2.0"` and `predicates = "3.0"` (dev-deps) — drive the compiled binary as a subprocess for end-to-end CLI tests in `tests/integration/cli_commands.rs`.
- `tokio-test = "0.4"` (dev-dep) — async test helpers.
- `tracing` + `tracing-subscriber` — used by `tests/observability.rs` to assert on log output and metrics.

## Conventions a new test should follow

The auto-generated notes in `tests/AGENTS.md` and `tests/unit/AGENTS.md` and `tests/integration/AGENTS.md` record the recent rule changes. Distilled:

1. **Use a tempdir for the queue.** Do not depend on the user's `~/.chronova/queue.db`. The current pattern (visible in `tests/retry_mechanism_test.rs`, `tests/heartbeat_sync.rs`, `tests/worktree_test.rs`) is to construct the queue inside a `tempfile::TempDir` and pass it through `HeartbeatManager::new_with_queue` or a similar seam. See [sync-and-offline.md](sync-and-offline.md) for the trait surface that makes this possible.
2. **Mock the API with `wiremock`.** Any test that exercises a network call should stub it; do not hit a real backend in CI. `tests/integration/network_failures.rs` shows the recipe.
3. **Drive the CLI with `assert_cmd`.** Any test that needs to assert on a CLI subcommand (`--today`, `--offline-count`, `--config-read`, `--extra-heartbeats`, etc.) should run the binary rather than call `HeartbeatManager` directly. The point is to catch flag-routing regressions in `src/main.rs`.
4. **No `println!` in tests.** Use `tracing` and let `--nocapture` decide. The CI runner is happy either way; the local debug loop benefits.
5. **Honor the existing 4-step "Testing checklist" in [`AGENTS.md`](../AGENTS.md):** `cargo test`, integration tests, `cargo clippy -- -D warnings`, `cargo fmt`, no compiler warnings, manual test of changed functionality.
6. **For privacy flags, prove the field is `None`.** When adding or changing a `--hide-*` flag, extend the assertions in `tests/cli_parsing.rs` to confirm the field is dropped from the JSON, and `tests/config_wakatime_compatibility.rs` to confirm the config key round-trips.
7. **For worktree behavior, mirror the rustdoc example.** The `DataCollector` module-level example in `src/collector.rs` is a contract — `tests/worktree_test.rs` enforces it.

## What to test when changing a specific area

| Change | Where to add coverage |
| --- | --- |
| New CLI flag | `tests/cli_parsing.rs` for shape; `tests/integration/cli_commands.rs` for end-to-end. |
| New `Config` field | `tests/sync_config.rs` (if sync) or `tests/config_wakatime_compatibility.rs` (otherwise). |
| New `Heartbeat` field | `tests/heartbeat_sync.rs` to assert it lands in the queue, plus a wiremock assertion in `tests/integration/network_failures.rs` to assert it reaches the API. |
| New `SyncStatus` value | `tests/retry_mechanism_test.rs` for the retry state machine, plus a new test in `tests/integration/sync_operations.rs` (currently placeholder — see below). |
| Queue schema change | `tests/error_recovery_test.rs` plus the dedicated test for your migration. |
| Self-update behavior | The binary path is hard to test in-process; a smoke test that calls `Updater::check_for_update` against a stubbed GitHub API is the realistic coverage. |
| Update logic change | Extend `tests/heartbeat_sync.rs`. |

## Known gaps

- `tests/integration/sync_operations.rs` is a placeholder (`test_sync_operations_placeholder`). The comment in that file says "Integration tests for sync operations would be complex due to binary crate structure / The existing unit tests already verify the sync functionality works correctly." If you add behavior that needs end-to-end sync coverage, this is the file to grow — but use `assert_cmd` and `wiremock` rather than touching the real `~/.chronova/queue.db`.
- The auto-generated `AGENTS.md` files (`src/AGENTS.md`, `docs/AGENTS.md`, `tests/AGENTS.md`, `tests/integration/AGENTS.md`, `tests/unit/AGENTS.md`) are inserted by `open-mem`. They are real activity logs and worth skimming when picking up after another session, but they are not documentation — don't write code against their contents.

## Where to look in the source

- [`.cargo/`](https://doc.rust-lang.org/cargo/) — `cargo test`, `cargo clippy`, `cargo fmt` are the entire toolchain.
- [`AGENTS.md`](../AGENTS.md) — testing checklist and common-task recipes.
- [`.github/workflows/test.yml`](../.github/workflows/test.yml) — what CI actually runs.
