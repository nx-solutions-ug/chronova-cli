---
type: guide
title: Editor Integration
description: Pointing WakaTime editor plugins at the Chronova CLI and API for VS Code, JetBrains IDEs, Vim, Sublime, Emacs, and Zed.
tags: [editor, vscode, jetbrains, vim, plugin, wakatime]
---

# Editor Integration

Chronova CLI is a drop-in replacement for `wakatime-cli`. In practice you do not invoke it by hand — a WakaTime editor plugin spawns the CLI on every save/cursor move, and the CLI sends the heartbeat. Pointing a plugin at Chronova needs exactly three things: the CLI path, the API URL, and a Chronova API key.

## How the wiring works

The plugin invokes the binary (directly or via a `wakatime-cli` symlink the installer creates) with WakaTime flags. Chronova implements the same flags, the same `--extra-heartbeats` STDIN protocol, and the same auth fallback chain, so unmodified plugins work. See [API Compatibility](../api-compatibility/index.md) for the payload and auth details.

The installers already create WakaTime-compatible symlinks (`~/.local/bin/wakatime-cli`), so plugins that search the PATH for `wakatime-cli` find Chronova without any config change. See [Installation](../operations/installation.md).

## VS Code

1. Install the WakaTime extension from the Extensions marketplace.
2. In Settings (`Ctrl+,`), search "WakaTime" and set:

```json
{
  "wakatime.apiKey": "your_chronova_api_key",
  "wakatime.baseApiUrl": "https://chronova.dev/api/v1",
  "wakatime.useCli": true,
  "wakatime.cliPath": "~/.local/bin/chronova-cli"
}
```

3. Restart VS Code.

The extension drives `--today` with `--output json` and parses the `{"text": ...}` statusbar response; Chronova's statusbar endpoint matches that shape.

## JetBrains IDEs (IntelliJ, WebStorm, PyCharm, ...)

1. Settings → Plugins → install **WakaTime**, restart the IDE.
2. Settings → Tools → WakaTime:
   - **API Key**: your Chronova key.
   - **API URL**: `https://chronova.dev/api/v1`.
   - Enable **Use custom CLI** and set the CLI path to the `chronova-cli` binary.

## Vim / Neovim

Install with your plugin manager:

```vim
Plug 'wakatime/vim-wakatime'
```

Then in `.vimrc` / `init.vim`:

```vim
let g:wakatime_ApiKey = 'your_chronova_api_key'
let g:wakatime_ApiUrl = 'https://chronova.dev/api/v1'
let g:wakatime_UseCli = 1
let g:wakatime_CliPath = '/path/to/chronova-cli'
```

## Sublime Text

1. Install the WakaTime package via Package Control.
2. Preferences → Package Settings → WakaTime → Settings - User:

```json
{
  "api_key": "your_chronova_api_key",
  "api_url": "https://chronova.dev/api/v1",
  "use_cli": true,
  "cli_path": "/path/to/chronova-cli"
}
```

## Emacs

```elisp
(package-install 'wakatime-mode)
(require 'wakatime-mode)
(setq wakatime-api-key "your_chronova_api_key")
(setq wakatime-url "https://chronova.dev/api/v1")
(setq wakatime-use-cli t)
(setq wakatime-cli-path "/path/to/chronova-cli")
(global-wakatime-mode 1)
```

## Zed and other editors

Any plugin that accepts a custom `wakatime-cli` path and API URL works the same way: point the CLI path at `chronova-cli` (or leave the default `wakatime-cli` symlink in place) and set the API URL + key. The plugin string passed via `--plugin` is embedded into the User-Agent as-is.

## Flags plugins commonly use

The CLI mirrors the WakaTime flag set the plugins send — see the full flag table in [Heartbeat Flow](../heartbeat/heartbeat-flow.md). The ones that matter for editor wiring:

| Flag | Plugin usage |
| --- | --- |
| `--entity`, `--entity-type` | The edited file / URL / domain / app. |
| `--plugin` | `ide/version plugin/version` string for the User-Agent. |
| `--lineno`, `--cursorpos`, `--lines` | Cursor position info. |
| `--write` | Set when the file was saved. |
| `--alternate-project` | Fallback project when none is detected. |
| `--extra-heartbeats` | Batched heartbeats over STDIN (JSON array). Missing `id` fields are filled with generated UUIDs, `type` defaults to `file`. |
| `--output json` / `--output raw-json` | For `--today`: machine-readable statusbar text; suppresses stdout logging so the JSON stays clean. |
| `--today-hide-categories` | Omits the category breakdown from `--today`. |

## Troubleshooting

1. **Plugin not sending data** — verify the CLI path is correct and executable (`chronova-cli --version`), and that the API key is set (`chronova-cli --config-read api_key`).
2. **Auth errors** — the CLI tries Bearer, Basic, and `X-API-Key` in order; a rejection on all three means the key or API URL is wrong. See [API Compatibility](../api-compatibility/index.md).
3. **Check connectivity manually** — `chronova-cli --entity /tmp/test.txt --language text` returns quickly; inspect `~/.chronova.log` with `--verbose` for the send path.
4. **Offline work** — heartbeats queue to `~/.chronova/queue.db` when the network is down and drain on the next successful sync. Inspect with `--offline-count`; see [Offline & Sync Behavior](../operations/offline-sync.md).

## Related pages

- [Installation](../operations/installation.md)
- [Configuration](../configuration/index.md)
- [API Compatibility](../api-compatibility/index.md)
- [Quickstart](../quickstart.md)