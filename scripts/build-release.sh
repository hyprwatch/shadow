#!/bin/bash
# Build shadow release binaries for multiple platforms
# Outputs to target/release/

set -e

cd "$(dirname "$0")/.."

VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
OUTPUT_DIR="target/releases"

echo "Building shadow v${VERSION}..."
mkdir -p "$OUTPUT_DIR"

# Build for Linux x86_64 (native or cross-compile)
build_linux_x86_64() {
    echo "Building for linux-x86_64..."
    
    if [[ "$(uname -m)" == "x86_64" ]] && [[ "$(uname -s)" == "Linux" ]]; then
        # Native build
        cargo build --release
        cp target/release/shadow "$OUTPUT_DIR/shadow-linux-x86_64"
    else
        # Cross-compile
        if ! command -v cross &> /dev/null; then
            echo "Installing cross..."
            cargo install cross
        fi
        cross build --release --target x86_64-unknown-linux-gnu
        cp target/x86_64-unknown-linux-gnu/release/shadow "$OUTPUT_DIR/shadow-linux-x86_64"
    fi
    
    echo "Built: $OUTPUT_DIR/shadow-linux-x86_64"
}

# Build for Linux aarch64
build_linux_aarch64() {
    echo "Building for linux-aarch64..."
    
    if [[ "$(uname -m)" == "aarch64" ]] && [[ "$(uname -s)" == "Linux" ]]; then
        # Native build
        cargo build --release
        cp target/release/shadow "$OUTPUT_DIR/shadow-linux-aarch64"
    else
        # Cross-compile
        if ! command -v cross &> /dev/null; then
            echo "Installing cross..."
            cargo install cross
        fi
        cross build --release --target aarch64-unknown-linux-gnu
        cp target/aarch64-unknown-linux-gnu/release/shadow "$OUTPUT_DIR/shadow-linux-aarch64"
    fi
    
    echo "Built: $OUTPUT_DIR/shadow-linux-aarch64"
}

# Build all platforms
build_all() {
    build_linux_x86_64
    build_linux_aarch64
    
    echo ""
    echo "Release builds complete!"
    echo "Binaries are in: $OUTPUT_DIR/"
    ls -lh "$OUTPUT_DIR"/shadow-*
}

# Build specific target or all
case "${1:-all}" in
    linux-x86_64)
        build_linux_x86_64
        ;;
    linux-aarch64)
        build_linux_aarch64
        ;;
    all)
        build_all
        ;;
    *)
        echo "Usage: $0 [linux-x86_64|linux-aarch64|all]"
        exit 1
        ;;
esac
