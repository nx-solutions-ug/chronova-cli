# CLI and configuration

The CLI is the entire public surface of `chronova-cli`. Because it must be a drop-in for `wakatime-cli`, almost every flag has a WakaTime counterpart; the config file uses the same `~/.chronova.cfg` (or WakaTime-shaped) INI format. This page summarizes that surface and where each option is enforced.

For the heartbeat logic that consumes these flags, see [heartbeat-flow.md](heartbeat-flow.md). For what runs after the CLI decides "this is a heartbeat", see [sync-and-offline.md](sync-and-offline.md).

## CLI surface

The `Cli` struct lives in `src/cli.rs` and is parsed with `clap`'s derive macros. `disable_version_flag = true` is set so the binary can render its own version line via `--version` (matching WakaTime's "client/version" string).

| Flag (alias) | Purpose | Notes |
| --- | --- | --- |
| `--entity` (`--file`) | File path / URL / domain / app being edited. | Required for the normal heartbeat path; not required for `--today`, `--offline-count`, `--extra-heartbeats`, etc. |
| `--key` | Inline API key; otherwise read from config or `CHRONOVA_API_KEY`. | |
| `--plugin` | Editor plugin name + version (e.g. `vscode/1.106 vscode-wakatime/25.5.0`). | Used by `generate_user_agent` to build the WakaTime-style UA. |
| `--time` | Floating-point unix epoch timestamp. | Defaults to "now". |
| `--lineno`, `--cursorpos`, `--lines` (`--lines-in-file`) | Editor position info. | `--lines` is normally auto-detected. |
| `--category` | `coding`, `building`, `debugging`, `learning`, `meeting`, etc. | Defaults to `coding`. |
| `--project`, `--alternate-project` | Override / fallback for the auto-detected project. | Auto-detection wins over `--alternate-project`. |
| `--language` | Override auto-detected language. | |
| `--config` | Path to config file. | Defaults to `~/.chronova.cfg`. `~` is expanded. |
| `--timeout` | Seconds to wait when sending heartbeats. | Default `30`. |
| `--verbose` | Turns on `DEBUG` level logging to the log file. | |
| `--write` | Marks the heartbeat as a write. | Accepts explicit `true` / `false` for plugin compatibility. |
| `--entity-type` | `file` / `domain` / `url` / `app`. | Default `file`. |
| `--today` | Print today's coding time, then exit. | Honors `--today-hide-categories`. |
| `--api-url` | Override the API base URL. | Default `https://chronova.dev/api/v1`. |
| `--hostname` | Override the machine name. | Default = `gethostname()`. |
| `--branch` | Override the auto-detected branch. | |
| `--hide-branch-names` | Obfuscate the branch field. | |
| `--disable-git-info` | Send no git fields at all. | |
| `--hide-commit-hash`, `--hide-commit-author`, `--hide-commit-message`, `--hide-repository-url` | Per-field git privacy flags. | |
| `--hide-file-names`, `--hide-project-names`, `--hide-project-folder` | Obfuscate file / project name fields. | |
| `--exclude`, `--include` | POSIX regex patterns. `--include` is honored even when `--exclude` matches. | May be passed multiple times. |
| `--disable-offline` | Do not queue heartbeats; only send if online. | |
| `--exclude-unknown-project` | Skip heartbeats where the project cannot be detected. | |
| `--guess-language` | Detect language from file contents, not just extension. | |
| `--local-file` | Override the entity path used for line counting. | |
| `--log-file` | Override the log file path. | Default `~/.chronova.log`. |
| `--no-ssl-verify`, `--ssl-certs-file` | SSL configuration for HTTPS. | |
| `--output` | `text` / `json` / `raw-json`. | `json` and `raw-json` suppress stdout logging so the payload stays clean. |
| `--project-folder` | Override the project root detection. | |
| `--proxy` | HTTPS / SOCKS / NTLM proxy URL. | |
| `--send-diagnostics-on-errors` | Even non-crash errors get diagnostics when verbose. | |
| `--metrics` | Write metrics to `~/.wakatime/metrics`. | |
| `--sync-offline-activity <n>` | Drain up to `n` queued heartbeats. | |
| `--force-sync` | Drain the queue regardless of connectivity. | |
| `--offline-count` | Print queue stats, then exit. | |
| `--extra-heartbeats` | Read a JSON array of heartbeats from STDIN until EOF. | |
| `--file-experts` | (Placeholder) Top developer for an entity. | Returns "not yet implemented". |
| `--config-read <key>`, `--config-write <key> <value>`, `--config-section <name>` | Read / write a single config key. | |
| `--internal-config` | Override the internal config file. | Default `~/.wakatime/wakatime-internal.cfg`. |
| `--log-to-stdout` | Mirror logs to stdout. | |
| `--print-offline-heartbeats <n>` | Print queued heartbeats to stdout. | |
| `--today-goal <id>` | (Placeholder) Today's goal time. | Returns "not yet implemented". |
| `--today-hide-categories` | Hide category breakdown from `--today` output. | |
| `--user-agent` | (Internal) Print the user-agent string and exit. | |
| `--offline-queue-file`, `--offline-queue-file-legacy` | (Internal) Override queue file path. | |
| `--is-unsaved-entity` | Track the entity even if the file does not exist. | |
| `--human-line-changes`, `--ai-line-changes` | Optional diff line counts (may be negative). | |
| `--include-only-with-project-file` | Skip folders without a `.wakatime-project`. | |
| `--version` | Print `chronova-cli v<CARGO_PKG_VERSION>` and exit. | |
| `--check-update` | Print whether a newer GitHub release exists. | |
| `--self-update` | Download and atomically replace the running binary. | |

## Precedence

A single field (e.g. `api_url`, `api_key`, `hide_commit_hash`) can come from a CLI flag, a config file, or a default. The `AGENTS.md` rule is **CLI > file > defaults**; the `Config::load` function in `src/config.rs` reads the file and produces the struct, and the relevant code in `HeartbeatManager` (and `main.rs`) overlays CLI values on top.

The same precedence applies to auth: `--key` wins, then `api_key` in the config, then `CHRONOVA_API_KEY` from the environment (consumed inside `Config::get_api_key`).

## Config file

`Config` is the in-memory representation (`src/config.rs`). It is loaded from an INI file with the `[settings]` section. If the file does not exist, `Config::load` returns `Config::default()` — so first-run works without any setup. The `ConfigError` enum (`ParseError`, `NotFound`, `InvalidPath`) is the only error type callers need to handle.

`Config` fields and their config-file keys (all in `[settings]`):

| Config key | Type | CLI override | Notes |
| --- | --- | --- | --- |
| `api_key` | string | `--key` | Also reads `CHRONOVA_API_KEY`. |
| `api_url` | string | `--api-url` | |
| `debug` | bool | `--verbose` | |
| `proxy` | string | `--proxy` | HTTPS / SOCKS / NTLM. |
| `hide_file_names` | bool | `--hide-file-names` | |
| `hide_project_names` | bool | `--hide-project-names` | |
| `hide_branch_names` | bool | `--hide-branch-names` | |
| `hide_commit_hash` / `hide_commit_author` / `hide_commit_message` / `hide_repository_url` | bool | matching `--hide-*` flag | |
| `disable_git_info` | bool | `--disable-git-info` | |
| `hide_project_folder` | bool | `--hide-project-folder` | |
| `exclude_unknown_project` | bool | `--exclude-unknown-project` | |
| `include_patterns` / `ignore_patterns` | list of regex | `--include` / `--exclude` | |
| `disable_offline` | bool | `--disable-offline` | |
| `guess_language` | bool | `--guess-language` | |
| `hostname` | string | `--hostname` | |
| `log_file` | string | `--log-file` | |
| `no_ssl_verify` | bool | `--no-ssl-verify` | |
| `ssl_certs_file` | string | `--ssl-certs-file` | |
| `metrics` | bool | `--metrics` | |
| `include_only_with_project_file` | bool | `--include-only-with-project-file` | |
| `auto_update` | bool | (no CLI flag) | |
| `sync_config` | nested | `--sync-offline-activity` / `--force-sync` | See [sync-and-offline.md](sync-and-offline.md). |

### Minimal config

```ini
[settings]
api_key = your-api-key
api_url = https://chronova.dev/api/v1
```

That is enough for the default heartbeat path. Everything else is either a privacy tweak, a network tweak, or a sync tuning knob.

## Authentication

`src/api.rs` implements the three auth methods `wakatime-cli` supports:

- **Bearer token** — `Authorization: Bearer <token>`. Used when the config / CLI provides an API key.
- **Basic auth** — `Authorization: Basic base64(user:pass)`. Used by some editor plugins.
- **X-API-Key header** — sent as a custom header. Used by other plugins.

The `AuthenticatedApiClient` wrapper around `ApiClient` carries the credentials and chooses the right header scheme per request. `ApiError::Auth` is the typed failure mode; the sync layer treats it as non-retryable (see [sync-and-offline.md](sync-and-offline.md)).

## Editor plugin compatibility

Editor plugins are the main entry point in practice. The plugins call the CLI with the WakaTime flag set; the CLI mirrors those flags exactly, so the same plugin works against a Chronova backend. To finish the wiring, the editor must point its CLI path at `chronova-cli` and the API key at the Chronova key. See [`docs/editor-integration.md`](../docs/editor-integration.md) for VS Code / JetBrains / Vim / Sublime / Atom / Emacs instructions, and [`docs/migration-guide.md`](../docs/migration-guide.md) for moving an existing WakaTime setup over.

## Where to look in the source

- Flag definitions and help text: [`src/cli.rs`](../src/cli.rs).
- Config struct, parser, default, and `get_api_key` / `get_api_url` helpers: [`src/config.rs`](../src/config.rs).
- Routing decisions in `main`: [`src/main.rs`](../src/main.rs).
- Editor plugin UA construction: [`src/user_agent.rs`](../src/user_agent.rs).
- Auth header logic: [`src/api.rs`](../src/api.rs).
- Compatibility test that pins the WakaTime-shaped config: [`tests/config_wakatime_compatibility.rs`](../tests/config_wakatime_compatibility.rs).
