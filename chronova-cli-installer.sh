#!/bin/sh

set -e

echo "##########################"
echo "# CHRONOVA CLI INSTALLER #"
echo "##########################"

# GitHub release settings
REPO="nx-solutions-ug/chronova-cli"
API_URL="https://api.github.com/repos/${REPO}"

# Parse command line arguments
API_KEY=""

# Platform detection - map to Rust target triples
detect_platform() {
    case "$(uname -s)" in
        Linux*)     echo "linux" ;;
        Darwin*)    echo "darwin" ;;
        MINGW*|MSYS*|CYGWIN*) echo "windows" ;;
        *)          echo "unknown" ;;
    esac
}

detect_architecture() {
    case "$(uname -m)" in
        x86_64)     echo "x86_64" ;;
        aarch64)    echo "aarch64" ;;
        arm64)      echo "aarch64" ;;
        armv7l)     echo "armv7" ;;
        i386|i686)  echo "i686" ;;
        *)          echo "unknown" ;;
    esac
}

PLATFORM=$(detect_platform)
ARCH=$(detect_architecture)
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

echo "Detected platform: $PLATFORM"
echo "Detected architecture: $ARCH"

if [ "$PLATFORM" = "unknown" ] || [ "$ARCH" = "unknown" ]; then
    echo "Error: Unsupported platform or architecture" >&2
    echo "Platform: $(uname -s), Architecture: $(uname -m)" >&2
    exit 1
fi

# Map platform and architecture to Rust target triple
get_target_triple() {
    local platform=$1
    local arch=$2
    local use_musl=${3:-false}

    case "$platform" in
        linux)
            if [ "$use_musl" = "true" ]; then
                echo "${arch}-unknown-linux-musl"
            else
                echo "${arch}-unknown-linux-gnu"
            fi
            ;;
        darwin)
            echo "${arch}-apple-darwin"
            ;;
        windows)
            echo "${arch}-pc-windows-msvc"
            ;;
        *)
            echo "unknown"
            ;;
    esac
}

# Detect from environment if CHRONOVA_CLI_MUSL is set for musl builds
USE_MUSL=false
if [ -n "$CHRONOVA_CLI_MUSL" ]; then
    USE_MUSL=true
    echo "Using musl libc variant (CHRONOVA_CLI_MUSL is set)"
fi

TARGET=$(get_target_triple "$PLATFORM" "$ARCH" "$USE_MUSL")

# Get the latest version or use provided version
if [ -n "$CHRONOVA_CLI_VERSION" ]; then
    VERSION="$CHRONOVA_CLI_VERSION"
    echo "Using specified version: $VERSION"
else
    # Fetch latest version from GitHub API
    echo "Fetching latest release version..."
    if command -v curl >/dev/null 2>&1; then
        VERSION=$(curl -s "${API_URL}/releases/latest" | grep -o '"tag_name": "[^"]*"' | cut -d'"' -f4)
    elif command -v wget >/dev/null 2>&1; then
        VERSION=$(wget -qO- "${API_URL}/releases/latest" | grep -o '"tag_name": "[^"]*"' | cut -d'"' -f4)
    fi
    
    if [ -z "$VERSION" ]; then
        echo "Error: Could not determine latest version" >&2
        exit 1
    fi
    echo "Latest version: $VERSION"
fi

# Set archive extension and extract command
if [ "$PLATFORM" = "windows" ]; then
    ARCHIVE_EXT=".zip"
else
    ARCHIVE_EXT=".tar.gz"
fi

# Set binary extension
if [ "$PLATFORM" = "windows" ]; then
    BINARY_EXT=".exe"
else
    BINARY_EXT=""
fi

# Backup existing WakaTime data
echo "Creating backups..."
if [ -d "$HOME/.wakatime" ]; then
    BACKUP_DIR="$HOME/.wakatime-backup-$TIMESTAMP"
    cp -R "$HOME/.wakatime" "$BACKUP_DIR" 2>/dev/null || echo "Warning: Could not backup .wakatime directory"
    echo "Backed up ~/.wakatime to $BACKUP_DIR"
fi

if [ -f "$HOME/.wakatime.cfg" ]; then
    BACKUP_CFG="$HOME/.wakatime.cfg.backup-$TIMESTAMP"
    cp "$HOME/.wakatime.cfg" "$BACKUP_CFG" 2>/dev/null || echo "Warning: Could not backup .wakatime.cfg"
    echo "Backed up ~/.wakatime.cfg to $BACKUP_CFG"
fi

# Create directories
echo "Setting up directories..."
mkdir -p "$HOME/.chronova"
mkdir -p "$HOME/.wakatime"
mkdir -p "$HOME/.local/bin"

# Download chronova-cli binary from GitHub releases
ARCHIVE_NAME="chronova-cli-${VERSION}-${TARGET}${ARCHIVE_EXT}"
DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${VERSION}/${ARCHIVE_NAME}"

TEMP_DIR=$(mktemp -d)
trap "rm -rf $TEMP_DIR" EXIT

echo "Downloading chronova-cli binary for ${TARGET}..."
echo "URL: $DOWNLOAD_URL"

if command -v curl >/dev/null 2>&1; then
    if curl -s -f -L -o "$TEMP_DIR/$ARCHIVE_NAME" "$DOWNLOAD_URL"; then
        echo "Downloaded using curl"
    else
        echo "Error: Failed to download binary using curl" >&2
        echo "URL: $DOWNLOAD_URL" >&2
        exit 1
    fi
elif command -v wget >/dev/null 2>&1; then
    if wget -q -O "$TEMP_DIR/$ARCHIVE_NAME" "$DOWNLOAD_URL"; then
        echo "Downloaded using wget"
    else
        echo "Error: Failed to download binary using wget" >&2
        echo "URL: $DOWNLOAD_URL" >&2
        exit 1
    fi
else
    echo "Error: Neither curl nor wget found. Please install one of them." >&2
    exit 1
fi

# Extract the archive
echo "Extracting archive..."
LOCAL_BINARY="$HOME/.chronova/chronova-cli${BINARY_EXT}"

cd "$TEMP_DIR"
if [ "$PLATFORM" = "windows" ]; then
    # For Windows, we need unzip
    if command -v unzip >/dev/null 2>&1; then
        unzip -q "$ARCHIVE_NAME"
    else
        echo "Error: unzip not found. Please install unzip." >&2
        exit 1
    fi
else
    tar -xzf "$ARCHIVE_NAME"
fi

# Find the extracted binary
if [ -f "chronova-cli${BINARY_EXT}" ]; then
    mv "chronova-cli${BINARY_EXT}" "$LOCAL_BINARY"
else
    # Try to find it in a subdirectory
    FOUND_BINARY=$(find . -name "chronova-cli${BINARY_EXT}" -type f | head -1)
    if [ -n "$FOUND_BINARY" ]; then
        mv "$FOUND_BINARY" "$LOCAL_BINARY"
    else
        echo "Error: Could not find chronova-cli binary in archive" >&2
        ls -la "$TEMP_DIR" >&2
        exit 1
    fi
fi

# Make binary executable
if [ "$PLATFORM" != "windows" ]; then
    chmod +x "$LOCAL_BINARY"
fi

echo "Binary installed to: $LOCAL_BINARY"

# Create symlinks for global/local bin
echo "Creating symlinks..."
if [ "$PLATFORM" = "windows" ]; then
    # Windows uses copy instead of symlinks for better compatibility
    cp "$LOCAL_BINARY" "$HOME/.local/bin/chronova-cli.exe" 2>/dev/null || true
    cp "$LOCAL_BINARY" "$HOME/.local/bin/wakatime-cli.exe" 2>/dev/null || true
else
    ln -sf "$LOCAL_BINARY" "$HOME/.local/bin/chronova-cli"
    ln -sf "$LOCAL_BINARY" "$HOME/.local/bin/wakatime-cli"
fi

# Create symlinks in .wakatime directory for extension compatibility
WAKATIME_CLI_NAME="wakatime-cli-${PLATFORM}-${ARCH}${BINARY_EXT}"
if [ "$PLATFORM" = "windows" ]; then
    cp "$LOCAL_BINARY" "$HOME/.wakatime/$WAKATIME_CLI_NAME" 2>/dev/null || true
    cp "$LOCAL_BINARY" "$HOME/.wakatime/wakatime-cli.exe" 2>/dev/null || true
else
    ln -sf "$LOCAL_BINARY" "$HOME/.wakatime/$WAKATIME_CLI_NAME"
    ln -sf "$LOCAL_BINARY" "$HOME/.wakatime/wakatime-cli"
fi

# Extract existing API key from WakaTime config before creating new config
extract_wakatime_api_key() {
    local config_file="$1"
    if [ -f "$config_file" ]; then
        # Try to extract api_key from various config formats
        local key
        key=$(grep -i "^api_key" "$config_file" 2>/dev/null | head -1 | sed 's/^[[:space:]]*api_key[[:space:]]*=[[:space:]]*//' | tr -d '[:space:]')
        if [ -n "$key" ] && [ "$key" != "your_api_key_here" ]; then
            echo "$key"
            return 0
        fi
    fi
    return 1
}

# Try to get API key from various sources (in order of priority)
# 1. Command line argument
# 2. Existing WakaTime config
# 3. Backup WakaTime config
# 4. Existing Chronova config

if [ -z "$API_KEY" ]; then
    echo "Looking for existing API key..."

    # Check current WakaTime config
    if [ -f "$HOME/.wakatime.cfg" ]; then
        API_KEY=$(extract_wakatime_api_key "$HOME/.wakatime.cfg") || true
        if [ -n "$API_KEY" ]; then
            echo "Found API key in ~/.wakatime.cfg"
        fi
    fi

    # Check backup config
    if [ -z "$API_KEY" ] && [ -f "$BACKUP_CFG" ]; then
        API_KEY=$(extract_wakatime_api_key "$BACKUP_CFG") || true
        if [ -n "$API_KEY" ]; then
            echo "Found API key in backup config"
        fi
    fi

    # Check existing Chronova config
    if [ -z "$API_KEY" ] && [ -f "$HOME/.chronova.cfg" ]; then
        API_KEY=$(extract_wakatime_api_key "$HOME/.chronova.cfg") || true
        if [ -n "$API_KEY" ]; then
            echo "Found API key in existing ~/.chronova.cfg"
        fi
    fi
fi

# Create or update Chronova config file
CHRONOVA_CFG="$HOME/.chronova.cfg"

# Create default config
if [ ! -f "$CHRONOVA_CFG" ]; then
    echo "Creating Chronova configuration file..."
    cat > "$CHRONOVA_CFG" << 'EOF'
[settings]
api_url = https://chronova.dev/api/v1
api_key = your_api_key_here
debug = false
hidefilenames = false
include_only_with_project_file = false
status_bar_enabled = true
EOF
fi

# Update API key if we have one
if [ -n "$API_KEY" ]; then
    echo "Configuring API key..."
    if [ "$PLATFORM" = "darwin" ]; then
        sed -i '' "s/^api_key[[:space:]]*=.*/api_key = $API_KEY/" "$CHRONOVA_CFG"
    else
        sed -i "s/^api_key[[:space:]]*=.*/api_key = $API_KEY/" "$CHRONOVA_CFG"
    fi
    echo "API key configured successfully"
fi

# Create symlink for WakaTime extension config compatibility
if [ -f "$CHRONOVA_CFG" ]; then
    ln -sf "$CHRONOVA_CFG" "$HOME/.wakatime.cfg"
    echo "Created config symlink: ~/.chronova.cfg -> ~/.wakatime.cfg"
fi

# Set appropriate permissions
if [ "$PLATFORM" != "windows" ]; then
    chmod 600 "$CHRONOVA_CFG" 2>/dev/null || true
fi

echo ""
echo "Installation complete!"
echo "• Chronova CLI installed to: $LOCAL_BINARY"
echo "• Symlinks created in: ~/.local/bin/"
echo "• WakaTime compatibility symlinks created in: ~/.wakatime/"
echo "• Configuration file: ~/.chronova.cfg"

if [ -n "$API_KEY" ]; then
    echo "• API key: Configured (migrated from existing config)"
else
    echo "• API key: Not configured"
fi

if [ -z "$API_KEY" ] && [ -t 0 ]; then
    echo ""
    read -p "Would you like to set your API key now? (y/n): " set_api_key
    if [ "$set_api_key" = "y" ] || [ "$set_api_key" = "Y" ]; then
        read -p "Enter your API key: " api_key_input
        if [ -n "$api_key_input" ] && [ -f "$CHRONOVA_CFG" ]; then
            if [ "$PLATFORM" = "darwin" ]; then
                sed -i '' "s/^api_key[[:space:]]*=.*/api_key = $api_key_input/" "$CHRONOVA_CFG"
            else
                sed -i "s/^api_key[[:space:]]*=.*/api_key = $api_key_input/" "$CHRONOVA_CFG"
            fi
            echo "API key updated in ~/.chronova.cfg"
        fi
    else
        echo "You can update your API key later by editing ~/.chronova.cfg"
    fi
elif [ -z "$API_KEY" ]; then
    echo ""
    echo "No API key configured. You can set it later by editing ~/.chronova.cfg"
    echo "To provide an API key in non-interactive mode, set the CHRONOVA_API_KEY environment variable"
fi

echo ""
echo "The WakaTime VSCode extension should now use Chronova CLI automatically!"
echo "You can test by running: chronova-cli --version"
echo ""
echo "For more information, visit: https://chronova.dev/docs"

exit 0
