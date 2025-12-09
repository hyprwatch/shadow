#!/bin/bash
# Build shadow release binaries for multiple platforms
# Outputs to target/releases/
#
# Usage:
#   ./build-release.sh              # Build for current platform
#   ./build-release.sh all          # Build for all platforms (requires cross)
#   ./build-release.sh linux-x86_64 # Build for specific target

set -e

cd "$(dirname "$0")/.."

VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
OUTPUT_DIR="target/releases"

echo "Building shadow v${VERSION}..."
mkdir -p "$OUTPUT_DIR"

# Detect current platform
CURRENT_OS=$(uname -s | tr '[:upper:]' '[:lower:]')
CURRENT_ARCH=$(uname -m)

# Normalize arch names
case "$CURRENT_ARCH" in
    x86_64|amd64) CURRENT_ARCH="x86_64" ;;
    aarch64|arm64) CURRENT_ARCH="aarch64" ;;
esac

# Check if cross is available for cross-compilation
ensure_cross() {
    if ! command -v cross &> /dev/null; then
        echo "Installing cross for cross-compilation..."
        cargo install cross --git https://github.com/cross-rs/cross
    fi
}

# Generate checksum for a binary
generate_checksum() {
    local file="$1"
    if command -v sha256sum &> /dev/null; then
        sha256sum "$file" > "$file.sha256"
    elif command -v shasum &> /dev/null; then
        shasum -a 256 "$file" > "$file.sha256"
    fi
    echo "Checksum: $file.sha256"
}

# Build for Linux x86_64
build_linux_x86_64() {
    echo "Building for linux-x86_64..."
    
    if [[ "$CURRENT_OS" == "linux" ]] && [[ "$CURRENT_ARCH" == "x86_64" ]]; then
        cargo build --release
        cp target/release/shadow "$OUTPUT_DIR/shadow-linux-x86_64"
    else
        ensure_cross
        cross build --release --target x86_64-unknown-linux-gnu
        cp target/x86_64-unknown-linux-gnu/release/shadow "$OUTPUT_DIR/shadow-linux-x86_64"
    fi
    
    chmod +x "$OUTPUT_DIR/shadow-linux-x86_64"
    generate_checksum "$OUTPUT_DIR/shadow-linux-x86_64"
    echo "Built: $OUTPUT_DIR/shadow-linux-x86_64"
}

# Build for Linux aarch64
build_linux_aarch64() {
    echo "Building for linux-aarch64..."
    
    if [[ "$CURRENT_OS" == "linux" ]] && [[ "$CURRENT_ARCH" == "aarch64" ]]; then
        cargo build --release
        cp target/release/shadow "$OUTPUT_DIR/shadow-linux-aarch64"
    else
        ensure_cross
        cross build --release --target aarch64-unknown-linux-gnu
        cp target/aarch64-unknown-linux-gnu/release/shadow "$OUTPUT_DIR/shadow-linux-aarch64"
    fi
    
    chmod +x "$OUTPUT_DIR/shadow-linux-aarch64"
    generate_checksum "$OUTPUT_DIR/shadow-linux-aarch64"
    echo "Built: $OUTPUT_DIR/shadow-linux-aarch64"
}

# Build for macOS x86_64 (Intel)
build_darwin_x86_64() {
    echo "Building for darwin-x86_64..."
    
    if [[ "$CURRENT_OS" == "darwin" ]]; then
        # On macOS, we can cross-compile between Intel and ARM
        rustup target add x86_64-apple-darwin 2>/dev/null || true
        cargo build --release --target x86_64-apple-darwin
        cp target/x86_64-apple-darwin/release/shadow "$OUTPUT_DIR/shadow-darwin-x86_64"
    else
        echo "Skipping darwin-x86_64 (can only build on macOS)"
        return 1
    fi
    
    chmod +x "$OUTPUT_DIR/shadow-darwin-x86_64"
    generate_checksum "$OUTPUT_DIR/shadow-darwin-x86_64"
    echo "Built: $OUTPUT_DIR/shadow-darwin-x86_64"
}

# Build for macOS aarch64 (Apple Silicon)
build_darwin_aarch64() {
    echo "Building for darwin-aarch64..."
    
    if [[ "$CURRENT_OS" == "darwin" ]]; then
        rustup target add aarch64-apple-darwin 2>/dev/null || true
        cargo build --release --target aarch64-apple-darwin
        cp target/aarch64-apple-darwin/release/shadow "$OUTPUT_DIR/shadow-darwin-aarch64"
    else
        echo "Skipping darwin-aarch64 (can only build on macOS)"
        return 1
    fi
    
    chmod +x "$OUTPUT_DIR/shadow-darwin-aarch64"
    generate_checksum "$OUTPUT_DIR/shadow-darwin-aarch64"
    echo "Built: $OUTPUT_DIR/shadow-darwin-aarch64"
}

# Build for current platform only
build_native() {
    echo "Building for current platform ($CURRENT_OS-$CURRENT_ARCH)..."
    cargo build --release
    
    local binary_name="shadow-${CURRENT_OS}-${CURRENT_ARCH}"
    cp target/release/shadow "$OUTPUT_DIR/$binary_name"
    chmod +x "$OUTPUT_DIR/$binary_name"
    generate_checksum "$OUTPUT_DIR/$binary_name"
    echo "Built: $OUTPUT_DIR/$binary_name"
}

# Build all platforms
build_all() {
    build_linux_x86_64
    build_linux_aarch64
    
    if [[ "$CURRENT_OS" == "darwin" ]]; then
        build_darwin_x86_64
        build_darwin_aarch64
    else
        echo ""
        echo "Note: macOS builds skipped (not running on macOS)"
        echo "macOS builds are handled by GitHub Actions CI"
    fi
    
    echo ""
    echo "========================================="
    echo "Release builds complete!"
    echo "========================================="
    echo "Binaries are in: $OUTPUT_DIR/"
    ls -lh "$OUTPUT_DIR"/shadow-*
}

# Print usage
usage() {
    echo "Usage: $0 [target]"
    echo ""
    echo "Targets:"
    echo "  native         Build for current platform (default)"
    echo "  all            Build for all platforms"
    echo "  linux-x86_64   Build for Linux x86_64"
    echo "  linux-aarch64  Build for Linux aarch64"
    echo "  darwin-x86_64  Build for macOS Intel (requires macOS)"
    echo "  darwin-aarch64 Build for macOS Apple Silicon (requires macOS)"
    echo ""
    echo "Examples:"
    echo "  $0              # Build for current platform"
    echo "  $0 all          # Build for all platforms"
}

# Main
case "${1:-native}" in
    native)
        build_native
        ;;
    linux-x86_64)
        build_linux_x86_64
        ;;
    linux-aarch64)
        build_linux_aarch64
        ;;
    darwin-x86_64)
        build_darwin_x86_64
        ;;
    darwin-aarch64)
        build_darwin_aarch64
        ;;
    all)
        build_all
        ;;
    -h|--help|help)
        usage
        ;;
    *)
        echo "Unknown target: $1"
        usage
        exit 1
        ;;
esac
