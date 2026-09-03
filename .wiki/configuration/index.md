---
type: reference
title: Configuration
description: The config file, every recognized key, precedence rules, and the sync-specific settings.
tags: [config, ini, settings, precedence, sync]
---

# Configuration

Chronova CLI reads an INI file (default `~/.chronova.cfg`, overridable with `--config`) into the `Config` struct in `src/config.rs`. The format is WakaTime-compatible: all recognized keys live in the `[settings]` section, and a missing file falls back to `Config::default()` so first run works without setup.

## Precedence

CLI > config file > defaults. `Config::load()` reads the file and produces the struct; `main.rs` then overlays the `--api-url` flag and the git-privacy flags on top. For the API key, `Config::get_api_key(cli_key)` resolves `--key` first, then `api_key` from the file.

Note the inverted key for offline queueing: `offline = true` means offline queueing is **enabled** (it maps to `disable_offline = false`).

## Config keys

| Key | Type | CLI override | Default | Notes |
| --- | --- | --- | --- | --- |
| `api_key` | string | `--key` | none | Required for API sends and `--today`. |
| `api_url` | string | `--api-url` | `https://chronova.dev/api/v1` | |
| `debug` | bool | `--verbose` | `false` | Debug logging to the log file. |
| `proxy` | string | `--proxy` | none | HTTPS / SOCKS / NTLM. |
| `offline` | bool | `--disable-offline` | `true` | Inverted: `offline = true` enables the offline queue. |
| `exclude` | list (multiline) | `--exclude` | `COMMIT_EDITMSG$`, `PULLREQ_EDITMSG$`, `MERGE_MSG$`, `TAG_EDITMSG$` | POSIX regex, one per line. |
| `include` | list (multiline) | `--include` | empty | Honored even when `--exclude` matches. |
| `hide_file_names` | bool | `--hide-file-names` | `false` | |
| `hide_project_names` | bool | `--hide-project-names` | `false` | |
| `hide_branch_names` | bool | `--hide-branch-names` | `false` | |
| `hide_commit_hash` | bool | `--hide-commit-hash` | `false` | |
| `hide_commit_author` | bool | `--hide-commit-author` | `false` | |
| `hide_commit_message` | bool | `--hide-commit-message` | `false` | |
| `hide_repository_url` | bool | `--hide-repository-url` | `false` | |
| `disable_git_info` | bool | `--disable-git-info` | `false` | Nulls all four git fields. |
| `hide_project_folder` | bool | `--hide-project-folder` | `false` | Sends the path relative to the project folder. |
| `exclude_unknown_project` | bool | `--exclude-unknown-project` | `false` | |
| `guess_language` | bool | `--guess-language` | `false` | Detect language from file contents. |
| `hostname` | string | `--hostname` | local hostname | |
| `log_file` | string | `--log-file` | `~/.chronova.log` | See [Logging & Updates](../operations/logging-updates.md). |
| `no_ssl_verify` | bool | `--no-ssl-verify` | `false` | |
| `ssl_certs_file` | string | `--ssl-certs-file` | system CA | |
| `metrics` | bool | `--metrics` | `false` | Writes metrics to `~/.wakatime/metrics`. |
| `include_only_with_project_file` | bool | `--include-only-with-project-file` | `false` | Only track folders containing `.wakatime-project`. |
| `auto_update` | bool | — | `false` | Spawns a background update check on every heartbeat invocation. |

## Sync settings

Sync knobs are parsed by `Config::parse_sync_config` from the same `[settings]` section into `SyncConfig` (`src/sync.rs`). Defaults:

| Key | Type | Default | Drives |
| --- | --- | --- | --- |
| `sync_enabled` | bool | `true` | Master switch for offline queueing. |
| `sync_max_queue_size` | int | `1000` | Queue capacity before oldest entries are trimmed. |
| `sync_interval` | int (seconds) | `300` | Background sync cadence (5 minutes). |
| `sync_max_retries` | int | `5` | Retry attempts before `PermanentFailure` in `SyncConfig` terms. |
| `sync_retry_base_delay` | int (seconds) | `1` | Exponential backoff base. |
| `sync_retry_max_delay` | int (seconds) | `60` | Backoff cap. |
| `sync_retry_use_jitter` | bool | `true` | Prevents synchronized retry storms. |
| `sync_retention_days` | int | `7` | Cleanup horizon for old entries. |
| `sync_background` | bool | `true` | Background sync task. |

Note on the inline sync path: `HeartbeatManager::process_queue` currently uses a hardcoded batch size of 50 and promotes `Failed` entries with fewer than 3 retries back to `Pending`; the `SyncConfig` values above are the declared defaults for the sync machinery. See [Offline & Sync Behavior](../operations/offline-sync.md).

## Reading and writing keys from the CLI

```bash
# Read a key (prints empty string if unset)
chronova-cli --config-read api_key

# Write a key
chronova-cli --config-write api_key your-key-here

# Target a section other than [settings]
chronova-cli --config-section other --config-read some_key
```

These operations resolve the config path with the same rules as the heartbeat path (`Config::resolve_config_path`): absolute paths are used as-is, `~/` is expanded to the home directory, and bare filenames resolve relative to the current directory.

## Minimal config

```ini
[settings]
api_key = your-api-key
api_url = https://chronova.dev/api/v1
```

Everything else is a privacy tweak, network tweak, or sync knob. The file uses `configparser` with multiline values enabled — that is how `exclude`/`include` hold one regex per line.

## Where to look in the source

- `Config` struct, `load`, path resolution, key precedence: `src/config.rs`.
- Sync key parsing: `Config::parse_sync_config` → `SyncConfig` in `src/sync.rs`.
- Flag definitions: `src/cli.rs`.
- Compatibility pin: `tests/config_wakatime_compatibility.rs`.

## Related pages

- [Heartbeat Flow](../heartbeat/index.md)
- [Offline & Sync Behavior](../operations/offline-sync.md)
- [Logging & Updates](../operations/logging-updates.md)
- [API Compatibility](../api-compatibility/index.md)