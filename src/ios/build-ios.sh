#!/bin/bash
set -e

echo "🦀 Building Rust library for iOS..."

# Get absolute paths
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

# Install iOS targets if not already installed
# This adds the aarch64-apple-ios target to rustup, which is required for
# cross-compiling Rust code to run on iOS devices (iPhone/iPad)
echo "📦 Installing iOS targets..."
rustup target add aarch64-apple-ios || true

# Create output directory where the compiled library will be placed
# This directory is referenced by the iOS Xcode project and CocoaPods
OUTPUT_DIR="$SCRIPT_DIR/ios/libs"
mkdir -p "$OUTPUT_DIR"

# Set iOS deployment target to match podspec (15.1)
# This ensures the compiled library is compatible with iOS 15.1 and later
# The deployment target must match what's specified in the Xos.podspec file
export IPHONEOS_DEPLOYMENT_TARGET=15.1

# Build for iOS device (arm64)
# This cross-compiles the Rust code for iOS devices (not simulators)
# - target: aarch64-apple-ios (64-bit ARM architecture used by modern iPhones/iPads)
# - release: Optimized build for production
# - --lib: Build as a library (not an executable)
# - --crate-type staticlib: Produce a static library (.a file) that can be linked into Swift code
# - link-arg: Sets minimum iOS version for the linker
echo "🔨 Building for iOS device (aarch64-apple-ios)..."
# Whisper pulls a desktop-only optional crate; iOS lib build uses default features off.
cargo rustc --target aarch64-apple-ios --release --lib --crate-type staticlib --no-default-features -- -C link-arg=-miphoneos-version-min=15.1

# Verify the build succeeded by checking for the output file
# The static library will be named libxos.a (following Rust's naming convention)
if [ ! -f "target/aarch64-apple-ios/release/libxos.a" ]; then
    echo "❌ Error: libxos.a not found after build"
    echo "   Found files:"
    ls -la target/aarch64-apple-ios/release/libxos.* 2>/dev/null || echo "   No libxos.* files found"
    exit 1
fi

# Copy the device library to the output directory
# This makes it available for the iOS Xcode project to link against
cp target/aarch64-apple-ios/release/libxos.a "$OUTPUT_DIR/libxos-device.a"

# Verify the architecture of the compiled library
# This confirms we built for the correct target (arm64 for iOS devices)
DEVICE_ARCH=$(lipo -info "$OUTPUT_DIR/libxos-device.a" | awk '{print $NF}')
echo "Device library arch: $DEVICE_ARCH"

# Create universal library
# Currently, we only build for device (arm64), so we use the device library as universal
# In the future, this could be extended to create a universal binary that includes
# both device (arm64) and simulator (x86_64/arm64) architectures using lipo
echo "📦 Using device library as universal..."
cp "$OUTPUT_DIR/libxos-device.a" "$OUTPUT_DIR/libxos.a"

echo "✅ Build complete! Library at: $OUTPUT_DIR/libxos.a"

