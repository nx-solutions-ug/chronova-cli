---
type: reference
title: API Compatibility
description: WakaTime-compatible endpoints, payload shape, authentication fallback chain, and user agent format.
tags: [api, wakatime, auth, compatibility, http]
---

# API Compatibility

Chronova CLI is a drop-in replacement for `wakatime-cli`: it speaks the same endpoints, the same JSON payload, and the same authentication schemes as the WakaTime API. A plugin configured for WakaTime works against a Chronova backend by changing only the API URL and key.

## Endpoints

The HTTP layer lives in `src/api.rs`. `ApiClient` holds a `reqwest::Client` with a 30-second timeout (matching `--timeout`); `AuthenticatedApiClient` wraps it with credentials. Base URL defaults to `https://chronova.dev/api/v1` (`Config::get_api_url()`), overridable with `--api-url` or `api_url` in `~/.chronova.cfg`.

| Method | Path | Used by |
| --- | --- | --- |
| `POST` | `/users/current/heartbeats` | `send_heartbeat` (single), `send_heartbeats_batch` (array payload) |
| `GET` | `/users/current/stats/today` | `get_today_stats` |
| `GET` | `/users/current/statusbar/today` | `get_today_statusbar` (drives `--today`) |
| `GET` | `/` (base URL root) | `check_connectivity` |

## Heartbeat payload

The `Heartbeat` struct in `src/heartbeat.rs` serializes to the WakaTime JSON shape. `entity_type` is renamed to `type` on the wire:

| Field | Type | Notes |
| --- | --- | --- |
| `id` | String (UUID v4) | Generated client-side per heartbeat. |
| `entity` | String | File path, URL, domain, or app. |
| `type` | String | `file`, `domain`, `url`, or `app`. |
| `time` | Float | Unix epoch seconds. |
| `project`, `branch`, `language` | String / null | Resolved from CLI flags or auto-detection. |
| `is_write` | Bool | From `--write`. |
| `lines`, `lineno`, `cursorpos` | Int / null | Editor position info. |
| `user_agent` | String / null | Sent both as JSON field and as the `User-Agent` header. |
| `category` | String / null | `coding`, `debugging`, etc. |
| `machine` | String / null | `--hostname` or the local hostname. |
| `editor`, `operating_system` | Object / null | Optional structured info. |
| `commit_hash`, `commit_author`, `commit_message`, `repository_url` | String / null | Git context; nulled by the privacy flags. |
| `dependencies` | Array of String | Currently always empty. |

Batch sends serialize the same structs as a JSON array to the same endpoint.

## Authentication fallback chain

`AuthenticatedApiClient` does not pick one auth scheme — every send walks a fallback chain, in order, until a request returns a success status:

1. **Bearer token** — `Authorization: Bearer <api_key>`.
2. **Basic auth** — `Authorization: Basic base64("<api_key>:")` (WakaTime compatibility; the key is the username with an empty password).
3. **X-API-Key header** — `X-API-Key: <api_key>` (WakaTime compatibility).

This means any plugin or server that accepts any of the three schemes works without configuration. The chain applies to single sends, batch sends, and the `--today` stats fetch alike. If all three attempts fail, the last error is typed as one of:

| `ApiError` variant | Meaning | Sync layer treatment |
| --- | --- | --- |
| `Network(reqwest::Error)` | Connection, timeout, DNS. | Retryable. |
| `Api(String, String)` | Non-success status from the API. | Depends on status. |
| `Auth(String)` | Authentication rejected. | Not retryable → `PermanentFailure`. |
| `RateLimit(String)` | Rate limited. | Backoff, then retry. |

## `--today` output shapes

`--today` hits `/users/current/statusbar/today`. Two response shapes are modeled: the statusbar shape (`StatusBarResponse` with `text` and `has_team_features`) and a fallback full-summary shape (`StatusBarFullResponse`). With `--output json` or `--output raw-json`, the CLI prints exactly:

```json
{"text":"3 hrs 12 mins","has_team_features":false}
```

…matching what the VS Code WakaTime extension parses. `format_today_output` in `src/api.rs` renders the text; `--today-hide-categories` drops the category breakdown. The same structures parse WakaTime-style stat objects: `StatsResponse` with `LanguageStat`, `ProjectStat`, `EditorStat`, `OsStat`, `CategoryStat`, `BestDay`, and `DailyStat`.

## User-Agent

`src/user_agent.rs::generate_user_agent` produces the WakaTime-style UA:

```
chronova/{version} ({os}-{kernel}-{platform}) {runtime} {plugin}
```

- The `--plugin` value (e.g. `vscode/1.106 vscode-wakatime/25.5.0`) is sanitized (surrounding quotes stripped) and its first two whitespace-separated tokens are used.
- A single-token plugin is duplicated to fill both slots.
- No plugin: `chronova-cli/{version} chronova-cli/{version}` — a two-part token matching WakaTime's expectation.

`--user-agent` prints the string the binary would send and exits (internal flag used by editor plugins).

## Extra heartbeats from external sources

`--extra-heartbeats` (STDIN JSON array) accepts both the strict internal `Heartbeat` shape and a relaxed WakaTime extension shape: `id` is optional (a UUID is generated), `type` defaults to `file`, and most other fields are optional. This is what lets unmodified WakaTime editor extensions feed Chronova directly.

## Compatibility pins in tests

- `tests/config_wakatime_compatibility.rs` — WakaTime-shaped INI config keys round-trip.
- `tests/wakatime_compatibility.rs` — payload and flag compatibility.
- `tests/integration/network_failures.rs` — wiremock stubs of `/users/current/heartbeats` exercising the send paths.

## Related pages

- [Heartbeat Flow](../heartbeat/index.md)
- [Offline & Sync Behavior](../operations/offline-sync.md)
- [Editor Integration](../editor-integration/index.md)
- [Configuration](../configuration/index.md)