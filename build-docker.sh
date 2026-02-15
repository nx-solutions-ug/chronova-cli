#!/bin/bash

set -euo pipefail

# Default values
TARGETS=()
BUILD_MODE="release"
OUTPUT_DIR="dist"
VERSION=$(grep '^version' Cargo.toml | cut -d '"' -f2)

# Docker image for macOS cross-compilation
DOCKER_IMAGE="x86_64-apple-darwin-cross.local:latest"

# Function to display usage
usage() {
    cat << EOF
Usage: $0 [OPTIONS]

Build chronova-cli for macOS using Docker cross-compilation.

OPTIONS:
    --target <target_triple>    Build for a specific macOS target
    --all                       Build for all macOS targets
    --debug                     Build in debug mode (default: release)
    --release                   Build in release mode (default)
    --output <dir>              Output directory (default: dist/)
    --help                      Show this help message

SUPPORTED MACOS TARGETS:
    x86_64-apple-darwin         # Intel Macs
    aarch64-apple-darwin        # Apple Silicon Macs

EXAMPLES:
    $0 --all                    # Build for all macOS targets in release mode
    $0 --target x86_64-apple-darwin --debug  # Build for Intel Mac in debug mode
    $0 --target aarch64-apple-darwin --output ./build  # Build for Apple Silicon to ./build

PREREQUISITES:
- Docker daemon running
- Local Docker image: $DOCKER_IMAGE
- Build Docker image with: docker build -f Dockerfile.x86_64-apple-darwin-cross -t $DOCKER_IMAGE .
EOF
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --target)
            TARGETS+=("$2")
            shift 2
            ;;
        --all)
            TARGETS=("x86_64-apple-darwin" "aarch64-apple-darwin")
            shift
            ;;
        --debug)
            BUILD_MODE="debug"
            shift
            ;;
        --release)
            BUILD_MODE="release"
            shift
            ;;
        --output)
            OUTPUT_DIR="$2"
            shift 2
            ;;
        --help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            usage
            exit 1
            ;;
    esac
done

# If no targets specified, show usage
if [[ ${#TARGETS[@]} -eq 0 ]]; then
    echo "Error: No targets specified"
    usage
    exit 1
fi

# Validate targets
for target in "${TARGETS[@]}"; do
    if [[ "$target" != "x86_64-apple-darwin" && "$target" != "aarch64-apple-darwin" ]]; then
        echo "Error: Unsupported target '$target'"
        echo "Supported macOS targets: x86_64-apple-darwin aarch64-apple-darwin"
        exit 1
    fi
done

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Function to get binary name for target
get_binary_name() {
    local target="$1"
    echo "chronova-cli-${target}"
}

# Function to build for macOS using Docker
build_macos_target() {
    local target="$1"
    local mode="$2"
    local output_dir="$3"

    echo "Building for macOS target $target using Docker cross-compilation..."

    # Check if Docker image exists
    if ! docker image inspect "$DOCKER_IMAGE" >/dev/null 2>&1; then
        echo "Error: Docker image '$DOCKER_IMAGE' not found."
        echo "Please build it first with:"
        echo "  docker build -f Dockerfile.x86_64-apple-darwin-cross -t $DOCKER_IMAGE ."
        return 1
    fi

    # Check if Docker daemon is running
    if ! docker info >/dev/null 2>&1; then
        echo "Error: Docker daemon is not running. Please start Docker and try again."
        return 1
    fi

    # Determine Rust target name
    local cargo_target="$target"

    # Build command for Docker
    local build_cmd="cargo build"
    if [[ "$mode" == "release" ]]; then
        build_cmd="$build_cmd --release"
    fi
    build_cmd="$build_cmd --target $cargo_target"

    echo "Running Docker build: $build_cmd"

    # Create a temporary script to run in Docker (in workspace so it can be mounted)
    local docker_script="$(pwd)/build_macos_temp_$$.sh"
    cat > "$docker_script" << 'DOCKER_SCRIPT'
#!/bin/bash
set -euo pipefail

cd /workspace

echo "Installing Rust..."
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source ~/.cargo/env

echo "Setting up cross-compilation environment..."
export PATH=/opt/osxcross/bin:$PATH
export CROSS_SYSROOT=/opt/osxcross/SDK/latest/
export BINDGEN_EXTRA_CLANG_ARGS='--sysroot=/opt/osxcross/SDK/latest/ -idirafter/usr/include'

# Add target if not already installed
rustup target add "$CARGO_TARGET"

echo "Creating cargo config..."
mkdir -p ~/.cargo
cat > ~/.cargo/config.toml << 'EOF'
[target.CARGO_TARGET_PLACEHOLDER]
linker = "x86_64-apple-darwin20.4-clang"
rustflags = [
  "-C", "link-arg=--sysroot=/opt/osxcross/SDK/latest/",
  "-C", "link-arg=-L/opt/osxcross/SDK/latest/usr/lib",
]
EOF

# Replace placeholder with actual target
sed -i "s/CARGO_TARGET_PLACEHOLDER/$CARGO_TARGET/g" ~/.cargo/config.toml

echo "Setting up native build environment variables..."
TARGET_VAR=$(echo "$CARGO_TARGET" | tr '-' '_')

# Determine the correct osxcross compiler based on target
if [[ "$CARGO_TARGET" == "aarch64-apple-darwin" ]]; then
    CLANG_TARGET="aarch64-apple-darwin20.4-clang"
    AR_TARGET="aarch64-apple-darwin20.4-ar"
    RANLIB_TARGET="aarch64-apple-darwin20.4-ranlib"
    STRIP_TARGET="aarch64-apple-darwin20.4-strip"
    CXX_TARGET="aarch64-apple-darwin20.4-clang++"
else
    CLANG_TARGET="x86_64-apple-darwin20.4-clang"
    AR_TARGET="x86_64-apple-darwin20.4-ar"
    RANLIB_TARGET="x86_64-apple-darwin20.4-ranlib"
    STRIP_TARGET="x86_64-apple-darwin20.4-strip"
    CXX_TARGET="x86_64-apple-darwin20.4-clang++"
fi

export "CC_${TARGET_VAR}=$CLANG_TARGET"
export "AR_${TARGET_VAR}=$AR_TARGET"
export "RANLIB_${TARGET_VAR}=$RANLIB_TARGET"
export "STRIP_${TARGET_VAR}=$STRIP_TARGET"
export "CXX_${TARGET_VAR}=$CXX_TARGET"
export "CFLAGS_${TARGET_VAR}=--sysroot=/opt/osxcross/SDK/latest/ -idirafter/usr/include"
export "LDFLAGS_${TARGET_VAR}=--sysroot=/opt/osxcross/SDK/latest/ -L/opt/osxcross/SDK/latest/usr/lib"

echo "Building chronova-cli..."
BUILD_CMD="$BUILD_CMD_PLACEHOLDER"
eval "$BUILD_CMD"

echo "Build completed successfully!"
DOCKER_SCRIPT

    # Replace placeholders in script
    sed -i "s/\$CARGO_TARGET/$cargo_target/g" "$docker_script"
    sed -i "s|\$BUILD_CMD_PLACEHOLDER|$build_cmd|g" "$docker_script"
    chmod +x "$docker_script"

    # Run build in Docker container
    if ! docker run --rm \
        -v "$(pwd):/workspace" \
        -v "$docker_script:/build.sh:ro" \
        "$DOCKER_IMAGE" \
        /bin/bash /build.sh; then
        echo "Error: Failed to build for $target in Docker"
        rm -f "$docker_script"
        return 1
    fi

    rm -f "$docker_script"

    # Determine source binary path
    local source_binary="target/$cargo_target/$mode/chronova-cli"
    local dest_binary="$output_dir/$(get_binary_name "$target")"

    # Copy binary
    if [[ -f "$source_binary" ]]; then
        cp "$source_binary" "$dest_binary"
        echo "Copied binary to: $dest_binary"

        # Generate SHA256 checksum
        local checksum_file="$dest_binary.sha256"
        if command -v sha256sum >/dev/null 2>&1; then
            sha256sum "$dest_binary" > "$checksum_file"
            echo "Generated checksum: $checksum_file"
        elif command -v shasum >/dev/null 2>&1; then
            shasum -a 256 "$dest_binary" > "$checksum_file"
            echo "Generated checksum: $checksum_file"
        else
            echo "Warning: Could not generate SHA256 checksum (sha256sum or shasum not found)"
        fi

        # Make binary executable
        chmod +x "$dest_binary"
    else
        echo "Error: Binary not found at $source_binary"
        return 1
    fi
}

# Main build process
echo "Starting macOS build process..."
echo "Version: $VERSION"
echo "Build mode: $BUILD_MODE"
echo "Output directory: $OUTPUT_DIR"
echo "Targets: ${TARGETS[@]}"
echo ""

# Build for each target
for target in "${TARGETS[@]}"; do
    if ! build_macos_target "$target" "$BUILD_MODE" "$OUTPUT_DIR"; then
        echo "Build failed for $target"
        exit 1
    fi
    echo ""
done

echo "Build completed successfully!"
echo "macOS binaries available in: $OUTPUT_DIR"
ls -la "$OUTPUT_DIR"