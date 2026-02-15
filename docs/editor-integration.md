# Editor Integration Guide

This guide explains how to integrate Chronova CLI with various code editors and IDEs that support WakaTime plugins.

## VS Code Integration

### Installation

1. Install the WakaTime extension for VS Code:
   - Open VS Code
   - Go to Extensions (Ctrl+Shift+X)
   - Search for "WakaTime"
   - Install the official WakaTime extension

2. Configure the extension to use Chronova CLI:
   - Open VS Code settings (Ctrl+,)
   - Search for "WakaTime"
   - Set the following configuration:

```json
{
  "wakatime.apiKey": "your_chronova_api_key",
  "wakatime.baseApiUrl": "https://chronova.dev/api/v1",
  "wakatime.useCli": true,
  "wakatime.cliPath": "/path/to/chronova-cli"
}
```

3. Restart VS Code to apply the changes.

### Configuration Options

- **API Key**: Your Chronova API key from your dashboard
- **Base API URL**: The Chronova API endpoint (default: https://chronova.dev/api/v1)
- **Use CLI**: Must be set to `true` to use Chronova CLI
- **CLI Path**: Full path to the chronova-cli binary

## IntelliJ IDEA / WebStorm / PyCharm Integration

### Installation

1. Install the WakaTime plugin:
   - Open your JetBrains IDE
   - Go to File → Settings → Plugins
   - Search for "WakaTime"
   - Install and restart the IDE

2. Configure the plugin:
   - Go to File → Settings → Tools → WakaTime
   - Set your API Key to your Chronova API key
   - Set the API URL to: `https://chronova.dev/api/v1`
   - Check "Use custom CLI" and set the CLI path to your chronova-cli binary

### Configuration Options

- **API Key**: Your Chronova API key
- **API URL**: `https://chronova.dev/api/v1`
- **Use custom CLI**: Enable this option
- **CLI Path**: Path to chronova-cli binary

## Vim / Neovim Integration

### Installation

1. Install the WakaTime plugin for Vim:
   - Using vim-plug:
     ```
     Plug 'wakatime/vim-wakatime'
     ```
   - Using Vundle:
     ```
     Plugin 'wakatime/vim-wakatime'
     ```

2. Configure the plugin by adding to your `.vimrc` or `init.vim`:
   ```vim
   " Set your Chronova API key
   let g:wakatime_ApiKey = 'your_chronova_api_key'
   
   " Set the API URL
   let g:wakatime_ApiUrl = 'https://chronova.dev/api/v1'
   
   " Use custom CLI
   let g:wakatime_UseCli = 1
   
   " Set path to chronova-cli
   let g:wakatime_CliPath = '/path/to/chronova-cli'
   ```

## Sublime Text Integration

### Installation

1. Install Package Control if you haven't already
2. Install the WakaTime package:
   - Ctrl+Shift+P → Package Control: Install Package
   - Search for "WakaTime"

3. Configure the plugin:
   - Preferences → Package Settings → WakaTime → Settings - User
   - Add your configuration:

```json
{
  "api_key": "your_chronova_api_key",
  "api_url": "https://chronova.dev/api/v1",
  "use_cli": true,
  "cli_path": "/path/to/chronova-cli"
}
```

## Atom Integration

### Installation

1. Install the WakaTime package:
   - Settings → Install → Search for "wakatime"

2. Configure the plugin:
   - Settings → Packages → wakatime → Settings
   - Enter your Chronova API key
   - Set API URL to: `https://chronova.dev/api/v1`
   - Enable "Use CLI" and set the CLI path

## Emacs Integration

### Installation

1. Install the WakaTime package:
   ```elisp
   (package-install 'wakatime-mode)
   ```

2. Add to your Emacs configuration:
   ```elisp
   (require 'wakatime-mode)
   
   ;; Set your Chronova API key
   (setq wakatime-api-key "your_chronova_api_key")
   
   ;; Set the API URL
   (setq wakatime-url "https://chronova.dev/api/v1")
   
   ;; Use custom CLI
   (setq wakatime-use-cli t)
   
   ;; Set path to chronova-cli
   (setq wakatime-cli-path "/path/to/chronova-cli")
   
   ;; Enable globally
   (global-wakatime-mode)
   ```

## Troubleshooting

### Common Issues

1. **Plugin not sending data**:
   - Check that your API key is correct
   - Verify the CLI path is correct and executable
   - Check the log file at `~/.wakatime.log` for errors

2. **Authentication errors**:
   - Ensure your API key is valid and active
   - Check that you're using the correct API URL

3. **Performance issues**:
   - The Chronova CLI is significantly faster than the Python-based WakaTime CLI
   - If you experience issues, check system resources and file permissions

### Log Files

Chronova CLI logs to `~/.wakatime.log` by default. Enable debug logging in your configuration for more detailed information:

```ini
[settings]
api_key = your_chronova_api_key
debug = true
```

### Offline Support

Chronova CLI automatically queues heartbeats when offline and sends them when connectivity is restored. The queue is stored in a SQLite database at `~/.wakatime/db.sqlite`.

## Performance Benefits

Switching to Chronova CLI provides significant performance improvements:

- **Startup Time**: ~10x faster than Python-based WakaTime CLI
- **Memory Usage**: ~5x lower memory footprint
- **CPU Usage**: Minimal impact during operation
- **Reliability**: Rust's memory safety prevents crashes

## Support

For issues with editor integration, please:
1. Check the plugin documentation for your specific editor
2. Verify your Chronova CLI installation and configuration
3. Review the log file at `~/.wakatime.log`
4. Contact Chronova support if issues persist