#!/bin/bash
# Don't use set -e here - we want to handle build errors gracefully
set -u  # Only fail on undefined variables

# Get app name from environment or use default
APP_NAME="${XOS_APP_NAME:-blank}"

echo "📱 Launching XOS iOS app ($APP_NAME) on device..."

# Get the script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Check if workspace exists
if [ ! -f "xos.xcworkspace/contents.xcworkspacedata" ]; then
    echo "❌ xos.xcworkspace not found. Please run 'pod install' first."
    exit 1
fi

# Use xcodebuild to get the actual available destinations (most reliable)
# This ensures we get the UDID format that xcodebuild actually recognizes
echo "🔍 Detecting connected iOS device..."
DESTINATIONS=$(xcodebuild -workspace xos.xcworkspace -scheme xos -showdestinations 2>/dev/null | grep -E "platform:iOS," | grep -v "Simulator" | grep -v "placeholder" || true)

if [ -z "$DESTINATIONS" ]; then
    echo "❌ No iOS device found. Please:"
    echo "   1. Connect your device via USB"
    echo "   2. Enable Developer Mode on your device (Settings > Privacy & Security > Developer Mode)"
    echo "   3. Trust this computer when prompted"
    echo "   4. Unlock your device"
    exit 1
fi

# Extract device info from xcodebuild output
# Format: { platform:iOS, arch:arm64, id:00008140-000249391180801C, name:iPhone }
DEVICE_UDID=$(echo "$DESTINATIONS" | grep -oE 'id:[0-9A-F-]+' | sed 's/id://' | head -1)
DEVICE_NAME=$(echo "$DESTINATIONS" | grep -oE 'name:[^,}]+' | sed 's/name://' | head -1)

if [ -z "$DEVICE_UDID" ]; then
    echo "❌ Could not extract device UDID from xcodebuild output."
    echo "   Available destinations:"
    echo "$DESTINATIONS"
    exit 1
fi

echo "📱 Found device: ${DEVICE_NAME:-iPhone} ($DEVICE_UDID)"

# Build for device
echo "🔨 Building app for device..."

# Build with automatic code signing and allow provisioning updates
# This allows xcodebuild to automatically create provisioning profiles
# We don't force a DEVELOPMENT_TEAM - let the project settings handle it
echo "📝 Using project's signing configuration..."
echo ""

# Try to build - if signing fails, provide helpful instructions
BUILD_OUTPUT=$(mktemp)
set +e  # Temporarily disable exit on error to capture output
xcodebuild -workspace xos.xcworkspace \
    -scheme xos \
    -configuration Debug \
    -destination "id=$DEVICE_UDID" \
    -derivedDataPath build \
    -allowProvisioningUpdates \
    CODE_SIGN_STYLE=Automatic \
    build 2>&1 | tee "$BUILD_OUTPUT"
BUILD_STATUS=${PIPESTATUS[0]}
set -e  # Re-enable exit on error

if [ $BUILD_STATUS -ne 0 ]; then
    # Check if the error is related to signing
    if grep -q "No Account for Team\|No profiles for\|requires a development team" "$BUILD_OUTPUT" 2>/dev/null; then
        echo ""
        echo "❌ Code signing is not configured."
        echo ""
        echo "📋 Please set up code signing in Xcode:"
        echo "   1. Open the workspace:"
        echo "      open xos.xcworkspace"
        echo ""
        echo "   2. In Xcode:"
        echo "      - Select the 'xos' project in the left navigator"
        echo "      - Select the 'xos' target"
        echo "      - Go to the 'Signing & Capabilities' tab"
        echo "      - Check 'Automatically manage signing'"
        echo "      - Select your Team (your Apple ID)"
        echo ""
        echo "   3. If you don't have a team:"
        echo "      - Xcode > Settings > Accounts"
        echo "      - Click '+' to add your Apple ID"
        echo "      - Then go back to Signing & Capabilities and select it"
        echo ""
        echo "   After setting up signing, run this command again."
        rm -f "$BUILD_OUTPUT"
        exit 1
    fi
    
    # Other build errors
    echo ""
    echo "❌ Build failed. Check the error messages above."
    rm -f "$BUILD_OUTPUT"
    exit 1
fi

rm -f "$BUILD_OUTPUT"

# Find the .app bundle
APP_BUNDLE=$(find build -name "xos.app" -type d | head -1)

if [ -z "$APP_BUNDLE" ]; then
    echo "❌ Could not find app bundle. Build may have failed."
    echo "   Trying alternative location..."
    APP_BUNDLE=$(find ~/Library/Developer/Xcode/DerivedData -name "xos.app" -type d 2>/dev/null | head -1)
    if [ -z "$APP_BUNDLE" ]; then
        echo "❌ App bundle not found. Please check build errors above."
        exit 1
    fi
fi

echo "✅ App built successfully: $APP_BUNDLE"

# Install and launch using ios-deploy if available
if command -v ios-deploy &> /dev/null; then
    echo "📲 Installing and launching app on device..."
    ios-deploy --bundle "$APP_BUNDLE" --justlaunch
    echo "✅ App launched!"
elif command -v xcrun &> /dev/null && xcrun devicectl &> /dev/null; then
    echo "📲 Installing app using xcrun devicectl..."
    # Modern iOS deployment (Xcode 15+)
    xcrun devicectl device install app --device "$DEVICE_UDID" "$APP_BUNDLE"
    echo "✅ App installed! Launch manually from device."
else
    echo "⚠️  ios-deploy not found. App is built but not installed."
    echo "   Install with: brew install ios-deploy"
    echo "   Or use Xcode to build and run on device."
    echo "   App bundle location: $APP_BUNDLE"
fi

