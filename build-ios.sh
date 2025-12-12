#!/bin/bash
set -e

echo "🦀 Building Rust library for iOS..."

# Get the directory of this script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Install iOS targets if not already installed
echo "📦 Installing iOS targets..."
rustup target add aarch64-apple-ios || true

# Create output directory
OUTPUT_DIR="$SCRIPT_DIR/ios/libs"
mkdir -p "$OUTPUT_DIR"

# Set iOS deployment target to match podspec (15.1)
export IPHONEOS_DEPLOYMENT_TARGET=15.1

# Build for iOS device (arm64)
echo "🔨 Building for iOS device (aarch64-apple-ios)..."
cargo build --target aarch64-apple-ios --release --lib
cp target/aarch64-apple-ios/release/libxos.a "$OUTPUT_DIR/libxos-device.a"

# Check architectures
DEVICE_ARCH=$(lipo -info "$OUTPUT_DIR/libxos-device.a" | awk '{print $NF}')

echo "Device library arch: $DEVICE_ARCH"

# Create universal library
echo "📦 Using device library as universal..."
cp "$OUTPUT_DIR/libxos-device.a" "$OUTPUT_DIR/libxos.a"

echo "✅ Build complete! Library at: $OUTPUT_DIR/libxos.a"

