---
type: guide
title: Installation
description: Platform-specific installation instructions for Chronova CLI, including installers, manual binaries, and PATH setup.
tags: [install, linux, macos, windows, operations]
---

# Installation

Chronova CLI can be installed with one-liner scripts, manually from GitHub releases, or built from source.

## Quick install

### Linux (glibc)

```bash
curl -fsSL https://raw.githubusercontent.com/nx-solutions-ug/chronova-cli/main/install-linux.sh | bash
```

### Linux (musl — Alpine, etc.)

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

Or, with explicit execution policy:

```powershell
powershell -ExecutionPolicy Bypass -Command "& {irm https://raw.githubusercontent.com/nx-solutions-ug/chronova-cli/main/install-windows.ps1 | iex}"
```

## What the installer does

1. Checks for required tools (`curl`/`wget`, `tar`/`unzip`, `sed`).
2. Detects the system architecture.
3. Backs up any existing `~/.wakatime` folder and `~/.wakatime.cfg`.
4. Downloads the correct release archive from GitHub releases.
5. Installs the binary to `~/.chronova/` and creates symlinks in `~/.local/bin/`.
6. Creates `~/.chronova.cfg` with default Chronova settings.
7. Creates WakaTime-compatible symlinks so existing VSCode extensions work.
8. Prompts for an API key from [chronova.dev/settings](https://chronova.dev/settings).

## Manual download

Pre-built binaries are available on the [releases page](https://github.com/nx-solutions-ug/chronova-cli/releases):

| Platform | Architecture | File |
| --- | --- | --- |
| Linux (glibc) | x86_64 | `chronova-cli-v{VERSION}-x86_64-unknown-linux-gnu.tar.gz` |
| Linux (glibc) | arm64 | `chronova-cli-v{VERSION}-aarch64-unknown-linux-gnu.tar.gz` |
| Linux (musl) | x86_64 | `chronova-cli-v{VERSION}-x86_64-unknown-linux-musl.tar.gz` |
| Linux (musl) | arm64 | `chronova-cli-v{VERSION}-aarch64-unknown-linux-musl.tar.gz` |
| macOS | Intel (x86_64) | `chronova-cli-v{VERSION}-x86_64-apple-darwin.tar.gz` |
| macOS | Apple Silicon (arm64) | `chronova-cli-v{VERSION}-aarch64-apple-darwin.tar.gz` |
| Windows | x86_64 | `chronova-cli-v{VERSION}-x86_64-pc-windows-msvc.zip` |
| Windows | arm64 | `chronova-cli-v{VERSION}-aarch64-pc-windows-msvc.zip` |

## Build from source

Requires Rust 1.70+.

```bash
git clone https://github.com/chronova/chronova-cli.git
cd chronova-cli
cargo build --release
```

The binary is produced at `target/release/chronova-cli`.

## PATH setup

The installer creates binaries in `~/.local/bin/`. Add it to your shell profile if it is not already there:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

Then reload:

```bash
source ~/.bashrc  # or ~/.zshrc
```

## Verify installation

```bash
chronova-cli --version
```

Expected output resembles `chronova-cli v1.3.5`.

## Uninstall

### Linux / macOS

```bash
rm -rf ~/.chronova ~/.chronova.cfg
rm ~/.local/bin/chronova-cli ~/.local/bin/wakatime-cli
rm ~/.wakatime/wakatime-cli*
```

### Windows

```powershell
Remove-Item -Recurse -Force ~\.chronova
Remove-Item -Force ~\.chronova.cfg
Remove-Item -Force ~\.local\bin\chronova-cli.exe
Remove-Item -Force ~\.local\bin\wakatime-cli.exe
Remove-Item -Force ~\.wakatime\wakatime-cli*.exe
```

## Related pages

- [Quickstart](../quickstart.md)
- [Configuration](../configuration/index.md)
- [Editor Integration](../editor-integration/index.md)
