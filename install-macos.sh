#!/bin/bash
#
# Chronova CLI macOS Installer
# Drop-in replacement for wakatime-cli
# https://chronova.dev
#

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
REPO="nx-solutions-ug/chronova-cli"
API_URL="https://api.github.com/repos/${REPO}"
DEFAULT_API_URL="https://chronova.dev/api/v1"

# Script name for cleanup
SCRIPT_NAME="$(basename "$0")"
SCRIPT_PATH="$(readlink -f "$0" 2>/dev/null || realpath "$0" 2>/dev/null || echo "$0")"

echo -e "${GREEN}##########################${NC}"
echo -e "${GREEN}# CHRONOVA CLI INSTALLER #${NC}"
echo -e "${GREEN}#       for macOS        #${NC}"
echo -e "${GREEN}##########################${NC}"
echo ""

# ============================================
# REQUIREMENTS CHECK
# ============================================
echo -e "${BLUE}Checking requirements...${NC}"

MISSING_DEPS=()

# Check for download tool
if ! command -v curl &>/dev/null && ! command -v wget &>/dev/null; then
    MISSING_DEPS+=("curl or wget")
fi

# Check for tar
if ! command -v tar &>/dev/null; then
    MISSING_DEPS+=("tar")
fi

# Check for sed
if ! command -v sed &>/dev/null; then
    MISSING_DEPS+=("sed")
fi

if [ ${#MISSING_DEPS[@]} -ne 0 ]; then
    echo -e "${RED}Error: Missing required dependencies:${NC}"
    printf '  - %s\n' "${MISSING_DEPS[@]}"
    echo ""
    echo "Please install them using Homebrew:"
    echo "  brew install curl tar gnu-sed"
    exit 1
fi

echo -e "${GREEN}All requirements met!${NC}"
echo ""

# ============================================
# PLATFORM DETECTION
# ============================================
echo -e "${BLUE}Detecting architecture...${NC}"

detect_architecture() {
    local arch
    arch=$(uname -m)
    case "$arch" in
        x86_64|amd64)
            echo "x86_64"
            ;;
        arm64)
            echo "aarch64"
            ;;
        *)
            echo "unknown"
            ;;
    esac
}

ARCH=$(detect_architecture)

if [ "$ARCH" = "unknown" ]; then
    echo -e "${RED}Error: Unsupported architecture: $(uname -m)${NC}"
    exit 1
fi

echo -e "  Architecture: ${GREEN}$ARCH${NC}"

# macOS uses aarch64-apple-darwin for both Intel and Apple Silicon
# Apple Silicon reports as arm64 but uses aarch64 in target
echo -e "  Platform: ${GREEN}macOS (Apple Darwin)${NC}"
echo ""

# Get target triple
get_target_triple() {
    local arch=$1
    echo "${arch}-apple-darwin"
}

TARGET=$(get_target_triple "$ARCH")
echo -e "  Target: ${GREEN}$TARGET${NC}"
echo ""

# ============================================
# FETCH LATEST VERSION
# ============================================
echo -e "${BLUE}Fetching latest release version...${NC}"

if [ -n "$CHRONOVA_CLI_VERSION" ]; then
    VERSION="$CHRONOVA_CLI_VERSION"
    echo -e "  Using specified version: ${GREEN}$VERSION${NC}"
else
    if command -v curl &>/dev/null; then
        VERSION=$(curl -sL "${API_URL}/releases/latest" | grep -o '"tag_name": "[^"]*"' | cut -d'"' -f4)
    else
        VERSION=$(wget -qO- "${API_URL}/releases/latest" | grep -o '"tag_name": "[^"]*"' | cut -d'"' -f4)
    fi
    
    if [ -z "$VERSION" ]; then
        echo -e "${RED}Error: Could not determine latest version${NC}"
        exit 1
    fi
    echo -e "  Latest version: ${GREEN}$VERSION${NC}"
fi
echo ""

# ============================================
# BACKUP EXISTING WAKATIME DATA
# ============================================
echo -e "${BLUE}Checking for existing WakaTime installation...${NC}"

TIMESTAMP=$(date +%Y%m%d_%H%M%S)

if [ -d "$HOME/.wakatime" ]; then
    BACKUP_DIR="$HOME/.wakatime-backup-${TIMESTAMP}"
    echo -e "  Backing up ~/.wakatime to ${YELLOW}$BACKUP_DIR${NC}"
    cp -R "$HOME/.wakatime" "$BACKUP_DIR" 2>/dev/null || {
        echo -e "${YELLOW}  Warning: Could not backup .wakatime directory${NC}"
    }
else
    echo -e "  No existing ~/.wakatime directory found"
fi

if [ -f "$HOME/.wakatime.cfg" ]; then
    BACKUP_CFG="$HOME/.wakatime.cfg.backup-${TIMESTAMP}"
    echo -e "  Backing up ~/.wakatime.cfg to ${YELLOW}$BACKUP_CFG${NC}"
    cp "$HOME/.wakatime.cfg" "$BACKUP_CFG" 2>/dev/null || {
        echo -e "${YELLOW}  Warning: Could not backup .wakatime.cfg${NC}"
    }
else
    echo -e "  No existing ~/.wakatime.cfg found"
fi
echo ""

# ============================================
# CREATE DIRECTORIES
# ============================================
echo -e "${BLUE}Creating directories...${NC}"

mkdir -p "$HOME/.chronova"
mkdir -p "$HOME/.wakatime"
mkdir -p "$HOME/.local/bin"

echo -e "  Created: ${GREEN}$HOME/.chronova${NC}"
echo -e "  Created: ${GREEN}$HOME/.wakatime${NC}"
echo -e "  Created: ${GREEN}$HOME/.local/bin${NC}"
echo ""

# ============================================
# DOWNLOAD BINARY
# ============================================
echo -e "${BLUE}Downloading Chronova CLI binary...${NC}"

ARCHIVE_NAME="chronova-cli-${VERSION}-${TARGET}.tar.gz"
DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${VERSION}/${ARCHIVE_NAME}"

echo -e "  URL: ${YELLOW}$DOWNLOAD_URL${NC}"

TEMP_DIR=$(mktemp -d)
trap "rm -rf $TEMP_DIR" EXIT

if command -v curl &>/dev/null; then
    if curl -sLf -o "$TEMP_DIR/$ARCHIVE_NAME" "$DOWNLOAD_URL"; then
        echo -e "  ${GREEN}Downloaded successfully with curl${NC}"
    else
        echo -e "${RED}Error: Failed to download binary${NC}"
        echo "  URL: $DOWNLOAD_URL"
        exit 1
    fi
else
    if wget -q -O "$TEMP_DIR/$ARCHIVE_NAME" "$DOWNLOAD_URL"; then
        echo -e "  ${GREEN}Downloaded successfully with wget${NC}"
    else
        echo -e "${RED}Error: Failed to download binary${NC}"
        echo "  URL: $DOWNLOAD_URL"
        exit 1
    fi
fi

# Extract archive
echo -e "${BLUE}Extracting archive...${NC}"
cd "$TEMP_DIR"
tar -xzf "$ARCHIVE_NAME"

# Find and move binary
LOCAL_BINARY="$HOME/.chronova/chronova-cli"
if [ -f "chronova-cli" ]; then
    mv "chronova-cli" "$LOCAL_BINARY"
else
    FOUND_BINARY=$(find . -name "chronova-cli" -type f | head -1)
    if [ -n "$FOUND_BINARY" ]; then
        mv "$FOUND_BINARY" "$LOCAL_BINARY"
    else
        echo -e "${RED}Error: Could not find chronova-cli binary in archive${NC}"
        exit 1
    fi
fi

chmod +x "$LOCAL_BINARY"
echo -e "  Binary installed to: ${GREEN}$LOCAL_BINARY${NC}"
echo ""

# ============================================
# CREATE SYMLINKS
# ============================================
echo -e "${BLUE}Creating symlinks...${NC}"

# Symlink for global/local bin
ln -sf "$LOCAL_BINARY" "$HOME/.local/bin/chronova-cli"
ln -sf "$LOCAL_BINARY" "$HOME/.local/bin/wakatime-cli"
echo -e "  Created: ${GREEN}~/.local/bin/chronova-cli${NC}"
echo -e "  Created: ${GREEN}~/.local/bin/wakatime-cli${NC}"

# Symlink in .wakatime directory for VSCode extension compatibility
# macOS naming convention: wakatime-cli-darwin-x86_64 or wakatime-cli-darwin-arm64
if [ "$ARCH" = "aarch64" ]; then
    WAKATIME_CLI_NAME="wakatime-cli-darwin-arm64"
else
    WAKATIME_CLI_NAME="wakatime-cli-darwin-${ARCH}"
fi
ln -sf "$LOCAL_BINARY" "$HOME/.wakatime/$WAKATIME_CLI_NAME"
ln -sf "$LOCAL_BINARY" "$HOME/.wakatime/wakatime-cli"
echo -e "  Created: ${GREEN}~/.wakatime/$WAKATIME_CLI_NAME${NC}"
echo -e "  Created: ${GREEN}~/.wakatime/wakatime-cli${NC}"

# Set permissions
chmod 755 "$HOME/.chronova"
chmod 755 "$HOME/.wakatime"
echo ""

# ============================================
# CONFIGURATION
# ============================================
echo -e "${BLUE}Setting up configuration...${NC}"

# Extract existing API key from WakaTime config
extract_api_key() {
    local config_file="$1"
    if [ -f "$config_file" ]; then
        local key
        key=$(grep -i "^api_key" "$config_file" 2>/dev/null | head -1 | sed 's/^[[:space:]]*api_key[[:space:]]*=[[:space:]]*//' | tr -d '[:space:]')
        if [ -n "$key" ] && [ "$key" != "your_api_key_here" ]; then
            echo "$key"
            return 0
        fi
    fi
    return 1
}

API_KEY=""

# Try to get API key from existing configs
echo -e "  Looking for existing API key...${NC}"

if [ -z "$API_KEY" ] && [ -f "$HOME/.wakatime.cfg" ]; then
    API_KEY=$(extract_api_key "$HOME/.wakatime.cfg") || true
    if [ -n "$API_KEY" ]; then
        echo -e "    ${GREEN}Found API key in ~/.wakatime.cfg${NC}"
    fi
fi

if [ -z "$API_KEY" ] && [ -n "$BACKUP_CFG" ] && [ -f "$BACKUP_CFG" ]; then
    API_KEY=$(extract_api_key "$BACKUP_CFG") || true
    if [ -n "$API_KEY" ]; then
        echo -e "    ${GREEN}Found API key in backup config${NC}"
    fi
fi

if [ -z "$API_KEY" ] && [ -f "$HOME/.chronova.cfg" ]; then
    API_KEY=$(extract_api_key "$HOME/.chronova.cfg") || true
    if [ -n "$API_KEY" ]; then
        echo -e "    ${GREEN}Found API key in existing ~/.chronova.cfg${NC}"
    fi
fi

# Create Chronova config
CHRONOVA_CFG="$HOME/.chronova.cfg"

if [ ! -f "$CHRONOVA_CFG" ]; then
    echo "  Creating default configuration..."
    cat > "$CHRONOVA_CFG" << EOF
[settings]
api_url = $DEFAULT_API_URL
api_key = your_api_key_here
debug = false
hidefilenames = false
include_only_with_project_file = false
status_bar_enabled = true
EOF
fi

# Update API URL (use BSD-style sed for macOS)
echo -e "  Setting API URL to ${GREEN}$DEFAULT_API_URL${NC}"
sed -i '' "s|^api_url[[:space:]]*=.*|api_url = $DEFAULT_API_URL|" "$CHRONOVA_CFG"

# Update API key if found (use BSD-style sed for macOS)
if [ -n "$API_KEY" ]; then
    sed -i '' "s/^api_key[[:space:]]*=.*/api_key = $API_KEY/" "$CHRONOVA_CFG"
    echo -e "  ${GREEN}API key configured from existing config${NC}"
fi

# Create symlinks for config
ln -sf "$CHRONOVA_CFG" "$HOME/.wakatime.cfg"
echo -e "  Config symlinked to: ${GREEN}~/.wakatime.cfg${NC}"

# Set permissions
chmod 600 "$CHRONOVA_CFG" 2>/dev/null || true

# Create log file symlink
if [ -f "$HOME/.chronova/chronova.log" ]; then
    ln -sf "$HOME/.chronova/chronova.log" "$HOME/chronova.log" 2>/dev/null || true
    ln -sf "$HOME/.chronova/chronova.log" "$HOME/wakatime.log" 2>/dev/null || true
fi

echo ""

# ============================================
# INTERACTIVE API KEY PROMPT
# ============================================
if [ -z "$API_KEY" ] && [ -t 0 ]; then
    echo -e "${YELLOW}========================================${NC}"
    echo -e "${YELLOW}API Key Configuration${NC}"
    echo -e "${YELLOW}========================================${NC}"
    echo ""
    echo -e "You can get your API key from: ${BLUE}https://chronova.dev/settings${NC}"
    echo -e "(${YELLOW}Command+Click the link above to open in browser${NC})"
    echo ""
    read -p "Would you like to enter your API key now? (y/n): " set_api_key
    
    if [[ "$set_api_key" =~ ^[Yy]$ ]]; then
        echo ""
        echo -e "${YELLOW}Please enter your API key from https://chronova.dev/settings:${NC}"
        read -s api_key_input
        echo ""
        
        if [ -n "$api_key_input" ]; then
            sed -i '' "s/^api_key[[:space:]]*=.*/api_key = $api_key_input/" "$CHRONOVA_CFG"
            echo -e "${GREEN}API key saved to ~/.chronova.cfg${NC}"
            API_KEY="$api_key_input"
        else
            echo -e "${YELLOW}No API key entered. You can configure it later by editing ~/.chronova.cfg${NC}"
        fi
    else
        echo -e "${YELLOW}Skipping API key configuration.${NC}"
        echo "You can add it later by editing ~/.chronova.cfg"
    fi
    echo ""
fi

# ============================================
# INSTALLATION COMPLETE
# ============================================
echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}Installation Complete!${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""
echo -e "${BLUE}Installation Summary:${NC}"
echo -e "  Binary:           ${GREEN}$LOCAL_BINARY${NC}"
echo -e "  Version:          ${GREEN}$VERSION${NC}"
echo -e "  Architecture:     ${GREEN}$ARCH${NC}"
echo -e "  Config file:      ${GREEN}$CHRONOVA_CFG${NC}"
echo -e "  Symlinks:         ${GREEN}~/.local/bin/chronova-cli${NC}"
echo -e "                    ${GREEN}~/.local/bin/wakatime-cli${NC}"
echo -e "                    ${GREEN}~/.wakatime/$WAKATIME_CLI_NAME${NC}"
echo ""

if [ -n "$API_KEY" ]; then
    echo -e "  API Key:          ${GREEN}Configured ✓${NC}"
else
    echo -e "  API Key:          ${YELLOW}Not configured${NC}"
    echo "                    Visit https://chronova.dev/settings to get your key"
    echo "                    Then run: chronova-cli --config to verify"
fi
echo ""

# Test installation
echo -e "${BLUE}Testing installation...${NC}"
if "$LOCAL_BINARY" --version &>/dev/null; then
    VERSION_OUTPUT=$("$LOCAL_BINARY" --version 2>&1)
    echo -e "  ${GREEN}✓ Binary working: $VERSION_OUTPUT${NC}"
else
    echo -e "  ${YELLOW}Warning: Could not verify binary${NC}"
fi
echo ""

echo -e "${GREEN}The WakaTime VSCode extension will now use Chronova CLI automatically!${NC}"
echo ""
echo -e "Next steps:"
echo "  1. ${BLUE}Add ~/.local/bin to your PATH if not already there${NC}"
echo "     Add this to your ~/.zshrc or ~/.bash_profile:"
echo "       export PATH=\"\$HOME/.local/bin:\$PATH\""
echo "  2. ${BLUE}Restart VSCode for changes to take effect${NC}"
echo "  3. ${BLUE}Visit https://chronova.dev/docs for more information${NC}"
echo ""

# ============================================
# CLEANUP - Delete the script
# ============================================
echo -e "${BLUE}Cleaning up installer...${NC}"
if [ -f "$SCRIPT_PATH" ]; then
    rm -f "$SCRIPT_PATH"
    echo -e "  ${GREEN}Installer script removed${NC}"
fi
echo ""

echo -e "${GREEN}Done!${NC}"
exit 0
