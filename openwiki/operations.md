# Operations

This page covers the day-to-day operational side of `chronova-cli`: how to build it, how releases are produced, how the self-update flow works, and how the automation in `.github/` is wired. It does not duplicate the user-facing install instructions — see [`INSTALL.md`](../INSTALL.md) for those.

## Build

The crate is a normal Rust binary + library.

```bash
# Dev build
cargo build

# Release build (LTO, panic=abort, opt-level=z — see Cargo.toml)
cargo build --release
# Binary: target/release/chronova-cli
```

Key `Cargo.toml` notes:

- `name = "chronova-cli"`, `version = "1.3.4"`, `edition = "2021"` (checked at the time of this wiki). The version is what `--version`, `Updater`, and the User-Agent string report.
- `[[bin]] name = "chronova-cli" path = "src/main.rs"` and `[lib] name = "chronova_cli" path = "src/lib.rs"` — both targets are built from the same source.
- `[profile.release]` sets `lto = true`, `panic = "abort"`, `opt-level = "z"` for size-optimized release builds.
- `Cross.toml` is present for cross-compilation. `build.sh` and `build-docker.sh` are convenience wrappers; the `Cross.toml` `[target.aarch64-apple-darwin] image = "aarch64-apple-darwin-cross.local"` is the only explicit cross target checked in.

## Distribution

The release artifacts follow a fixed layout that the self-update flow depends on (documented in `src/updater.rs`):

- Repository: `nx-solutions-ug/chronova-cli`
- Tag format: `v.{version}` (note the dot after the `v`)
- Asset name: `chronova-cli-{tag}-{target_triple}.{tar.gz|zip}`

Example: `https://github.com/nx-solutions-ug/chronova-cli/releases/download/v.1.2.0/chronova-cli-v.1.2.0-x86_64-unknown-linux-gnu.tar.gz`.

Supported targets and filenames (from `INSTALL.md`):

| Platform | Architecture | File |
| --- | --- | --- |
| Linux (glibc) | x86_64 | `chronova-cli-v{VERSION}-x86_64-unknown-linux-gnu.tar.gz` |
| Linux (glibc) | arm64 | `chronova-cli-v{VERSION}-aarch64-unknown-linux-gnu.tar.gz` |
| Linux (musl) | x86_64 | `chronova-cli-v{VERSION}-x86_64-unknown-linux-musl.tar.gz` |
| Linux (musl) | arm64 | `chronova-cli-v{VERSION}-aarch64-unknown-linux-musl.tar.gz` |
| macOS | Intel | `chronova-cli-v{VERSION}-x86_64-apple-darwin.tar.gz` |
| macOS | Apple Silicon | `chronova-cli-v{VERSION}-aarch64-apple-darwin.tar.gz` |
| Windows | x86_64 | `chronova-cli-v{VERSION}-x86_64-pc-windows-msvc.zip` |
| Windows | arm64 | `chronova-cli-v{VERSION}-aarch64-pc-windows-msvc.zip` |

The matching platform installers are `install-linux.sh`, `install-macos.sh`, and `install-windows.ps1`. The Linux installer accepts `CHRONOVA_CLI_MUSL=true` to pick the musl asset (used for Alpine). The Windows installer accepts `-ExecutionPolicy Bypass` for one-liner use.

## Self-update

Implemented in `src/updater.rs`. The module is documented in detail (release artifact layout, atomicity, error modes), so this is a short summary.

- `--check-update` → `Updater::check_for_update` calls GitHub `/repos/.../releases/latest` and returns `Some(UpdateInfo)` only when the remote version is strictly greater than the running `CARGO_PKG_VERSION`. It prints the version and download URL on success, "up to date" otherwise.
- `--self-update` → `Updater::check_and_update` combines the check with `perform_update`, which:
  1. Downloads the asset for the current target triple to a temp directory.
  2. Extracts it via `tar` / `unzip` (depending on extension).
  3. Atomically replaces the running executable.

Atomicity differs by platform:

- **Unix** — write the new bytes to `<current_exe>.new`, then `rename(2)` over the original. The kernel keeps the old inode alive until the process exits, so a running binary can replace itself.
- **Windows** — the running exe is locked, so the updater renames the original to `<current_exe>.old` first, then moves the new binary into place. A leftover `.old` from a previous successful update is silently removed.

The `UpdaterError` enum is the typed failure surface: `Network`, IO, version parsing, and unsupported platform.

## CI

GitHub Actions workflows live in [`.github/workflows/`](../.github/workflows/):

- **`test.yml`** — runs `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test --verbose` on `ubuntu-latest`, `macos-latest`, and `windows-latest`. This is the gating check for any PR. Includes a `~/.cargo` / `target` cache keyed by `Cargo.lock`.
- **`release.yml`** — produces a new release. `workflow_dispatch` accepts an optional `force_version` (skips semantic-release analysis) and a `prerelease` boolean. The job graph starts with a "Determine Version" job (with `contents: write`, `issues: write`, `pull-requests: write` permissions) that runs in `.github/release-tooling/`.
- **`openwiki-update.yml`** — the OpenWiki sync workflow. Runs on `workflow_dispatch`, on push to `main`, and on a daily schedule (`0 8 * * *` UTC). It builds the OpenWiki CLI from a fork (`niklasschaeffer/openwiki@feat/ollama-provider`), runs `node /tmp/openwiki/dist/cli.js --update --print`, then opens a PR with the `openwiki/` changes via `peter-evans/create-pull-request@v8`. Auth uses a GitHub App token (`APP_CLIENT_ID` / `APP_PRIVATE_KEY`).
- **`omp.yml` / `omp-ci.yml`** — the "omp" (Open-MAD? — see `.omp/`) automation. `omp-ci.yml` triggers on `issues: opened`, `pull_request: opened|synchronize|ready_for_review`, and `workflow_dispatch`; jobs include `triage-issue` and `review-pr`. The `.omp/commands/` folder holds the prompts (`label-pr.md`, `review-pr.md`, `triage-issue.md`) and `.omp/rules/gh-label-idempotent.md` is a rule the agent enforces.
- **`auto-manage.yml`** — auto-management workflow (label hygiene, stale issue/PR handling, etc.).

Other CI-shaped config:

- [`.github/release-tooling/`](../.github/release-tooling) — `package.json` and `release.config.js` for the semantic-release driven build. This is where the "Determine Version" job runs.
- [`renovate.json`](../renovate.json) — Renovate config for dependency updates.

## Operational runbooks

### Drain the offline queue manually

```bash
# Show what's there
chronova-cli --offline-count

# Force a sync regardless of connectivity caching
chronova-cli --force-sync

# Or sync up to N heartbeats explicitly
chronova-cli --sync-offline-activity 200
```

This is the typical "I have stuck `Failed` entries, what do I do?" answer. If the queue has `PermanentFailure` entries, those will not be retried — they need to be cleared or the auth/config issue fixed first.

### Inspect the queue database

The SQLite file lives at `~/.chronova/queue.db` (or `--offline-queue-file` if set). Schema is in `src/queue.rs`. Two tables of interest: the queue entries table and `schema_version` (migrations).

### Add a new release

Two paths:

1. **Automated** — push a commit with a conventional commit message; the semantic-release machinery in `release.yml` figures out the next version.
2. **Manual** — `Actions → Build and Release → Run workflow`, set `force_version` to the target version (e.g. `v1.3.0`) and toggle `prerelease` if needed. The release workflow then produces the assets using the `v.{version}` tag format the updater expects.

### Roll back a release

There is no built-in "downgrade" command in `chronova-cli`. The safe path is:

1. Mark the bad release as a pre-release or yank the asset on GitHub.
2. Ship a fix and let the normal release flow produce a new version that is strictly greater than the bad one (the updater only offers updates, not downgrades — see `Updater::check_for_update`).
3. If users need to revert immediately, point their plugin at an older binary they still have, or temporarily change `--api-url` to a no-op backend and reinstall a known-good version from an earlier release.

## Where to look in the source

- Build configuration: [`Cargo.toml`](../Cargo.toml), [`Cross.toml`](../Cross.toml), [`build.sh`](../build.sh), [`build-docker.sh`](../build-docker.sh).
- User-facing install instructions: [`INSTALL.md`](../INSTALL.md).
- Self-update logic: [`src/updater.rs`](../src/updater.rs) (heavy inline rustdoc, recommended reading before changing the flow).
- CI definitions: [`.github/workflows/`](../.github/workflows/).
- Release tooling: [`.github/release-tooling/`](../.github/release-tooling/).
- Renovate config: [`renovate.json`](../renovate.json).
