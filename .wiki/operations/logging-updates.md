---
type: reference
title: Logging & Updates
description: Structured logging setup, log file locations, and the self-update mechanism.
tags: [logging, tracing, updates, operations]
---

# Logging & Updates

Chronova CLI uses the `tracing` ecosystem for structured logging and ships its own updater for release delivery.

## Logging

`src/logger.rs` provides two entry points:

- `setup_logging(verbose)` — standard setup.
- `setup_logging_with_output_format(verbose, json_output)` — disables stdout logging so JSON output (from `--output json`) is not corrupted.

### Log file location

Default: `~/.chronova.log`.

Override with `--log-file`. Enable debug logging with `--verbose` or `debug = true` in config.

### Output modes

- **Text** — default human-readable output.
- **JSON** / **raw-json** — used by editor plugins; automatically suppresses stdout log emission.

### Logging in the codebase

The project style is to use `tracing::error!()` on error paths rather than `println!`. See the [Development Guide](../development/index.md) for conventions.

## Self-update

`src/updater.rs` implements a built-in updater. It queries GitHub releases and, when a newer version is available, downloads the platform-specific archive and atomically replaces the running executable.

### Commands

Check for a new release:

```bash
chronova-cli --check-update
```

Download and install the latest version:

```bash
chronova-cli --self-update
```

### Release conventions

The updater depends on the release layout in `nx-solutions-ug/chronova-cli`:

- Tag format: `v.{version}` (note the dot after `v`).
- Asset name: `chronova-cli-{tag}-{target_triple}.{tar.gz|zip}`.

Example:
```
https://github.com/nx-solutions-ug/chronova-cli/releases/download/v.1.2.0/chronova-cli-v.1.2.0-x86_64-unknown-linux-gnu.tar.gz
```

### Atomic replacement

- **Unix** — writes to `<current_exe>.new`, then `rename(2)` over the original. The old inode stays alive until the process exits.
- **Windows** — the running executable is locked, so the original is renamed to `.old` first and the new binary is moved into place. Leftover `.old` files are removed on the next update.

### Update errors

`UpdaterError` covers:

- `Network` — GitHub API or download failures.
- `Parse` — invalid release JSON.
- `InvalidVersion` — unexpected version / tag format.
- `UnsupportedPlatform` — no asset for the current platform.
- `Io` — filesystem / extract / rename failures.

## Troubleshooting

1. Check the log file at `~/.chronova.log` for detailed error traces.
2. Run with `--verbose` to enable debug logging.
3. Verify `api_url` and `api_key` if sync or today queries fail.
4. Use `--offline-count` to inspect the queue state.
5. For update issues, confirm the binary was installed from the expected release and that the platform triple is supported.

## Related pages

- [Configuration](../configuration/index.md)
- [Offline & Sync Behavior](./offline-sync.md)
- [Development Guide](../development/index.md)
