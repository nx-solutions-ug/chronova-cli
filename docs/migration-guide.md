# Migration Guide: From WakaTime to Chronova

This guide helps you migrate from WakaTime to Chronova while maintaining compatibility with your existing editor plugins.

## Why Migrate to Chronova?

Chronova offers several advantages over WakaTime:

1. **Performance**: Chronova CLI is built in Rust, providing 10x faster startup and 5x lower memory usage
2. **Privacy**: Complete control over your data with self-hosting options
3. **Features**: Enhanced analytics, team dashboards, and advanced reporting
4. **Compatibility**: Full drop-in replacement for WakaTime CLI and plugins
5. **Cost**: Competitive pricing with a generous free tier

## Migration Steps

### 1. Get Your Chronova API Key

1. Sign up at [chronova.dev](https://chronova.dev)
2. Navigate to your account settings
3. Copy your API key

### 2. Install Chronova CLI

#### Option A: Download Pre-built Binary

Download the latest release for your platform from the [Releases page](https://github.com/chronova/chronova-cli/releases).

#### Option B: Build from Source

```bash
git clone https://github.com/chronova/chronova-cli.git
cd chronova-cli
cargo build --release
```

The binary will be available at `target/release/chronova-cli`.

### 3. Update Your Configuration

Chronova CLI uses the same configuration file format as WakaTime. Update your `~/.chronova.cfg`:

```ini
[settings]
api_key = your_chronova_api_key_here
api_url = https://chronova.dev/api/v1
debug = false
```

### 4. Configure Your Editor Plugin

Most WakaTime plugins support custom CLI paths. Point your plugin to the Chronova CLI binary instead of the default WakaTime CLI.

See the [Editor Integration Guide](editor-integration.md) for detailed instructions for your specific editor.

### 5. Verify the Migration

Test that everything is working correctly:

```bash
# Test the CLI directly
chronova-cli --entity /path/to/your/file.rs --plugin "vscode/1.0.0 chronova/1.0.0" --verbose

# Check your Chronova dashboard for activity
```

## Configuration Changes

### API URL

Change from WakaTime's API URL to Chronova's:

```ini
# WakaTime
api_url = https://api.wakatime.com/api/v1

# Chronova
api_url = https://chronova.dev/api/v1
```

### Configuration File Location

Chronova CLI looks for configuration in the same locations as WakaTime:
- `~/.chronova.cfg` (default)
- Custom path specified with `--config` flag

## Data Migration

### Existing WakaTime Data

Chronova does not automatically import your WakaTime data. If you want to preserve your historical data:

1. Export your data from WakaTime (if available in your plan)
2. Contact Chronova support for import options

### New Data

All new coding activity will be tracked by Chronova once the migration is complete.

## Editor Plugin Compatibility

Chronova CLI is fully compatible with all WakaTime plugins:

| Editor | Plugin | Compatibility |
|--------|--------|---------------|
| VS Code | WakaTime | ✅ Full |
| IntelliJ IDEA | WakaTime | ✅ Full |
| Vim/Neovim | vim-wakatime | ✅ Full |
| Sublime Text | WakaTime | ✅ Full |
| Atom | wakatime | ✅ Full |
| Emacs | wakatime-mode | ✅ Full |
| Other Editors | WakaTime plugins | ✅ Full |

## Troubleshooting

### Common Issues

1. **Authentication Errors**:
   - Verify your API key is correct
   - Ensure you're using the Chronova API URL

2. **Plugin Not Sending Data**:
   - Check that the CLI path is correct
   - Verify the CLI binary has execute permissions
   - Check the log file at `~/.wakatime.log`

3. **Performance Issues**:
   - Chronova CLI should be faster than WakaTime CLI
   - If experiencing issues, check system resources

### Log Files

Chronova CLI logs to `~/.wakatime.log` by default. Enable debug logging for troubleshooting:

```ini
[settings]
api_key = your_chronova_api_key
debug = true
```

## Support

For migration assistance:

1. Check the documentation and this guide
2. Review the log file at `~/.wakatime.log`
3. Contact Chronova support at support@chronova.dev
4. Join our community Discord for real-time help

## Rollback to WakaTime

If you need to revert to WakaTime:

1. Update your editor plugin configuration to use WakaTime CLI
2. Restore your WakaTime API key and URL in the configuration
3. Restart your editor

## Feedback

We'd love to hear about your migration experience! Please share your feedback with us at feedback@chronova.dev.