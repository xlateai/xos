#!/bin/bash
set -e

echo "🦀 Building Rust library for iOS..."

# Get absolute paths (script lives at src/ios/build-ios.sh → repo root is two levels up)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$PROJECT_ROOT"

# When Rust is invoked via `compile_ios_rust()`, `CARGO_TARGET_DIR` may be `.../target/ios`.
# Builds still produce `aarch64-apple-ios/release/libxos.a` under that root (not `$PROJECT_ROOT/target`).
TARGET_ROOT="${CARGO_TARGET_DIR:-$PROJECT_ROOT/target}"

# Install iOS targets if not already installed
# This adds the aarch64-apple-ios target to rustup, which is required for
# cross-compiling Rust code to run on iOS devices (iPhone/iPad)
echo "📦 Installing iOS targets..."
rustup target add aarch64-apple-ios || true

# Create output directory where the compiled library will be placed
# CocoaPods / Xcode expect libs next to the pod (see Xos.podspec vendored_libraries)
OUTPUT_DIR="$SCRIPT_DIR/libs"
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
# No default features: omit desktop-only stacks (e.g. Silero/ORT, Burn/WGPU). Enable CT2 Whisper
# so `xos.audio.transcription` and `xos.ai.whisper` (backend=ct2) match desktop cache under ~/.xos.
cargo rustc --target aarch64-apple-ios --release --lib --crate-type staticlib --no-default-features --features "whisper,whisper_ct2" -- -C link-arg=-miphoneos-version-min=15.1

# Verify the build succeeded by checking for the output file (use absolute path; cwd must be repo root)
LIB_XOS_A="$TARGET_ROOT/aarch64-apple-ios/release/libxos.a"
if [ ! -f "$LIB_XOS_A" ]; then
    echo "❌ Error: libxos.a not found after build"
    echo "   Expected: $LIB_XOS_A"
    echo "   PROJECT_ROOT=$PROJECT_ROOT"
    echo "   TARGET_ROOT=$TARGET_ROOT"
    echo "   Listing release dir:"
    ls -la "$TARGET_ROOT/aarch64-apple-ios/release/" 2>/dev/null || echo "   (missing)"
    exit 1
fi

# Copy the device library to the output directory
# This makes it available for the iOS Xcode project to link against
cp "$LIB_XOS_A" "$OUTPUT_DIR/libxos-device.a"

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

