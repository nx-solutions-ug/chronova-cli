#!/bin/bash

set -euo pipefail

# Default values
TARGETS=()
BUILD_MODE="release"
OUTPUT_DIR="dist"
VERSION=$(grep '^version' Cargo.toml | cut -d '"' -f2)

# Supported targets
SUPPORTED_TARGETS=(
    "x86_64-unknown-linux-gnu"
    "x86_64-unknown-linux-musl"
    "aarch64-unknown-linux-gnu"
    "x86_64-pc-windows-gnu"
    "x86_64-apple-darwin"
    "aarch64-apple-darwin"
    # Note: macOS cross builds use local Docker images with osxcross toolchain.
    # Ensure Docker daemon is running and local images are built.
)

# Docker image for macOS cross-compilation
DOCKER_IMAGE="x86_64-apple-darwin-cross.local:latest"

# Function to display usage
usage() {
    cat << EOF
Usage: $0 [OPTIONS]

Build chronova-cli for multiple platforms using cross-compilation.

OPTIONS:
    --target <target_triple>    Build for a specific target
    --all                       Build for all supported targets
    --debug                     Build in debug mode (default: release)
    --release                   Build in release mode (default)
    --output <dir>              Output directory (default: dist/)
    --help                      Show this help message

SUPPORTED TARGETS:
    ${SUPPORTED_TARGETS[@]}

EXAMPLES:
    $0 --all                    # Build for all targets in release mode
    $0 --target x86_64-unknown-linux-gnu --debug  # Build for Linux x64 in debug mode
    $0 --target x86_64-pc-windows-gnu --output ./build  # Build for Windows to ./build
    $0 --target x86_64-apple-darwin  # Build for macOS Intel using Docker cross-compilation

NOTE: macOS targets require:
- Docker daemon running
- Local Docker image: $DOCKER_IMAGE
- Build with: docker build -f Dockerfile.x86_64-apple-darwin-cross -t $DOCKER_IMAGE .
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
            TARGETS=("${SUPPORTED_TARGETS[@]}")
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
    if [[ ! " ${SUPPORTED_TARGETS[@]} " =~ " ${target} " ]]; then
        echo "Error: Unsupported target '$target'"
        echo "Supported targets: ${SUPPORTED_TARGETS[@]}"
        exit 1
    fi
done

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Function to get binary extension for target
get_binary_extension() {
    local target="$1"
    if [[ "$target" == *"windows"* ]]; then
        echo ".exe"
    else
        echo ""
    fi
}

# Function to get binary name for target
get_binary_name() {
    local target="$1"
    local ext=$(get_binary_extension "$target")
    echo "chronova-cli-${target}${ext}"
}

# Function to check if target is macOS
is_macos_target() {
    local target="$1"
    [[ "$target" == *"apple-darwin"* ]]
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

    # Determine Rust target name (convert to cargo format)
    local cargo_target="$target"
    if [[ "$target" == "x86_64-apple-darwin" ]]; then
        cargo_target="x86_64-apple-darwin"
    elif [[ "$target" == "aarch64-apple-darwin" ]]; then
        cargo_target="aarch64-apple-darwin"
    fi

    # Build command for Docker
    local build_cmd="cargo build"
    if [[ "$mode" == "release" ]]; then
        build_cmd="$build_cmd --release"
    fi
    build_cmd="$build_cmd --target $cargo_target"

    echo "Running Docker build: $build_cmd"

    # Run build in Docker container
    if ! docker run --rm \
        -v "$(pwd):/workspace" \
        "$DOCKER_IMAGE" \
        /bin/bash -c "
            cd /workspace && 
            source ~/.cargo/env && 
            export PATH=/opt/osxcross/bin:\$PATH &&
            export CROSS_SYSROOT=/opt/osxcross/SDK/latest/ &&
            export BINDGEN_EXTRA_CLANG_ARGS='--sysroot=/opt/osxcross/SDK/latest/ -idirafter/usr/include' &&
            
            # Set up environment for native builds
            local target_var=\$(echo $cargo_target | tr '-' '_')
            export CC_\${target_var}=x86_64-apple-darwin20.4-clang
            export AR_\${target_var}=x86_64-apple-darwin20.4-ar
            export RANLIB_\${target_var}=x86_64-apple-darwin20.4-ranlib
            export STRIP_\${target_var}=x86_64-apple-darwin20.4-strip
            export CXX_\${target_var}=x86_64-apple-darwin20.4-clang++
            export CFLAGS_\${target_var}='--sysroot=/opt/osxcross/SDK/latest/ -idirafter/usr/include'
            export LDFLAGS_\${target_var}='--sysroot=/opt/osxcross/SDK/latest/ -L/opt/osxcross/SDK/latest/usr/lib'
            
            # Create cargo config for the target
            mkdir -p ~/.cargo
            cat > ~/.cargo/config.toml << 'EOF'
[target.$cargo_target]
linker = \"x86_64-apple-darwin20.4-clang\"
rustflags = [
  \"-C\", \"link-arg=--sysroot=/opt/osxcross/SDK/latest/\",
  \"-C\", \"link-arg=-L/opt/osxcross/SDK/latest/usr/lib\",
]
EOF
            
            # Run the build
            $build_cmd
        "; then
        echo "Error: Failed to build for $target in Docker"
        return 1
    fi

    # Determine source binary path
    local source_binary="target/$cargo_target/$mode/chronova-cli$(get_binary_extension "$target")"
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

# Function to build for a specific target
build_target() {
    local target="$1"
    local mode="$2"
    local output_dir="$3"

    echo "Building for $target ($mode mode)..."

    # Use Docker build for macOS targets
    if is_macos_target "$target"; then
        build_macos_target "$target" "$mode" "$output_dir"
        return $?
    fi

    # Build command for other targets
    local build_cmd="cross build"
    if [[ "$mode" == "release" ]]; then
        build_cmd="$build_cmd --release"
    fi
    build_cmd="$build_cmd --target $target"

    echo "Running: $build_cmd"
    if ! $build_cmd; then
        echo "Error: Failed to build for $target"
        return 1
    fi

    # Determine source binary path
    local source_binary="target/$target/$mode/chronova-cli$(get_binary_extension "$target")"
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

        # Make binary executable if not Windows
        if [[ "$target" != *"windows"* ]]; then
            chmod +x "$dest_binary"
        fi
    else
        echo "Error: Binary not found at $source_binary"
        return 1
    fi
}

# Main build process
echo "Starting build process..."
echo "Version: $VERSION"
echo "Build mode: $BUILD_MODE"
echo "Output directory: $OUTPUT_DIR"
echo "Targets: ${TARGETS[@]}"
echo ""

# Check if cross is installed (for non-macOS targets)
for target in "${TARGETS[@]}"; do
    if ! is_macos_target "$target"; then
        if ! command -v cross >/dev/null 2>&1; then
            echo "Error: 'cross' command not found. Please install it with:"
            echo "cargo install cross"
            exit 1
        fi
        break
    fi
done

# Build for each target
for target in "${TARGETS[@]}"; do
    if ! build_target "$target" "$BUILD_MODE" "$OUTPUT_DIR"; then
        echo "Build failed for $target"
        exit 1
    fi
    echo ""
done

echo "Build completed successfully!"
echo "Binaries available in: $OUTPUT_DIR"
ls -la "$OUTPUT_DIR"