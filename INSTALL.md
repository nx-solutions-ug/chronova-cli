# Chronova CLI Installation

Quick one-liner installers for Chronova CLI - the drop-in replacement for wakatime-cli.

## Quick Install

### Linux (GNU libc)

```bash
curl -fsSL https://raw.githubusercontent.com/nx-solutions-ug/chronova-cli/main/install-linux.sh | bash
```

### Linux (musl libc - Alpine, etc.)

```bash
CHRONOVA_CLI_MUSL=true curl -fsSL https://raw.githubusercontent.com/nx-solutions-ug/chronova-cli/main/install-linux.sh | bash
```

### macOS

```bash
curl -fsSL https://raw.githubusercontent.com/nx-solutions-ug/chronova-cli/main/install-macos.sh | bash
```

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/nx-solutions-ug/chronova-cli/main/install-windows.ps1 | iex
```

Or with explicit execution policy:

```powershell
powershell -ExecutionPolicy Bypass -Command "& {irm https://raw.githubusercontent.com/nx-solutions-ug/chronova-cli/main/install-windows.ps1 | iex}"
```

## Manual Download

Visit the [releases page](https://github.com/nx-solutions-ug/chronova-cli/releases) to download binaries manually.

Available binaries:

| Platform | Architecture | File |
|----------|--------------|------|
| Linux (glibc) | x86_64 | `chronova-cli-v{VERSION}-x86_64-unknown-linux-gnu.tar.gz` |
| Linux (glibc) | arm64 | `chronova-cli-v{VERSION}-aarch64-unknown-linux-gnu.tar.gz` |
| Linux (musl) | x86_64 | `chronova-cli-v{VERSION}-x86_64-unknown-linux-musl.tar.gz` |
| Linux (musl) | arm64 | `chronova-cli-v{VERSION}-aarch64-unknown-linux-musl.tar.gz` |
| macOS | Intel (x86_64) | `chronova-cli-v{VERSION}-x86_64-apple-darwin.tar.gz` |
| macOS | Apple Silicon (arm64) | `chronova-cli-v{VERSION}-aarch64-apple-darwin.tar.gz` |
| Windows | x86_64 | `chronova-cli-v{VERSION}-x86_64-pc-windows-msvc.zip` |
| Windows | arm64 | `chronova-cli-v{VERSION}-aarch64-pc-windows-msvc.zip` |

## What the Installer Does

1. **Requirements Check** - Verifies curl/wget, tar/unzip, and sed are available
2. **Architecture Detection** - Automatically detects your system architecture
3. **Backup** - Backs up your existing `~/.wakatime` folder and `~/.wakatime.cfg`
4. **Download** - Downloads the correct binary for your platform from GitHub releases
5. **Installation** - Installs to `~/.chronova/` and creates necessary symlinks
6. **Configuration** - Creates `~/.chronova.cfg` with Chronova settings
7. **WakaTime Compatibility** - Creates symlinks so VSCode extensions work automatically
8. **API Key Prompt** - Interactive prompt to enter your API key from https://chronova.dev/settings

## Post-Installation

### Add to PATH (Linux/macOS)

The installer creates binaries in `~/.local/bin/`. Add this to your shell profile:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

Then reload your shell:
```bash
source ~/.bashrc  # or ~/.zshrc
```

### Configuration Files

- **Main config:** `~/.chronova.cfg`
- **WakaTime compatible:** `~/.wakatime.cfg` (symlinked to above)
- **Logs:** `~/.chronova/chronova.log`

### API Key

Get your API key from https://chronova.dev/settings and add it to your config:

```ini
[settings]
api_key = your-api-key-here
api_url = https://chronova.dev/api/v1
```

Or run the installer again - it will prompt you for the key.

## Verification

Test your installation:

```bash
chronova-cli --version
```

## VSCode Extension

The installer sets up WakaTime compatibility automatically. The VSCode extension will use Chronova CLI without any configuration changes.

## Uninstall

To remove Chronova CLI:

```bash
# Linux/macOS
rm -rf ~/.chronova ~/.chronova.cfg
rm ~/.local/bin/chronova-cli ~/.local/bin/wakatime-cli
rm ~/.wakatime/wakatime-cli*

# Windows
Remove-Item -Recurse -Force ~\.chronova
Remove-Item -Force ~\.chronova.cfg
Remove-Item -Force ~\.local\bin\chronova-cli.exe
Remove-Item -Force ~\.local\bin\wakatime-cli.exe
Remove-Item -Force ~\.wakatime\wakatime-cli*.exe
```

## Troubleshooting

### Permission Denied

If you get permission errors, make sure the binary is executable:

```bash
chmod +x ~/.chronova/chronova-cli
```

### Command Not Found

Ensure `~/.local/bin` is in your PATH:

```bash
echo $PATH | grep -q ".local/bin" || echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc
```

### Windows Execution Policy

If you get execution policy errors on Windows, run PowerShell as Administrator and execute:

```powershell
Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope CurrentUser
```

## Support

For issues or questions:
- 📖 Documentation: https://chronova.dev/docs
- 🐛 Issues: https://github.com/nx-solutions-ug/chronova-cli/issues
- 💬 Support: https://chronova.dev/support
