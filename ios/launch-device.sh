#!/bin/bash
set -e

# Get app name from environment or use default
APP_NAME="${XOS_APP_NAME:-blank}"

echo "📱 Launching XOS iOS app ($APP_NAME) on device..."

# Check if device is connected
DEVICES=$(xcrun xctrace list devices 2>/dev/null | grep -i "iphone\|ipad" | grep -v "Simulator" | head -1)

if [ -z "$DEVICES" ]; then
    echo "❌ No iOS device found. Please connect your device via USB."
    echo "   Make sure Developer Mode is enabled on your device."
    exit 1
fi

# Extract device UDID (first device found)
DEVICE_UDID=$(echo "$DEVICES" | grep -oE '[0-9A-F]{8}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{12}' | head -1)
DEVICE_NAME=$(echo "$DEVICES" | sed 's/.*\(iPhone\|iPad\).*/\1/' | head -1)

if [ -z "$DEVICE_UDID" ]; then
    echo "❌ Could not detect device UDID. Trying alternative method..."
    # Alternative: use ios-deploy if available
    if command -v ios-deploy &> /dev/null; then
        echo "✅ Using ios-deploy..."
        cd "$(dirname "$0")"
        xcodebuild -workspace xos.xcworkspace \
            -scheme xos \
            -configuration Debug \
            -destination 'generic/platform=iOS' \
            build
        
        ios-deploy --bundle build/Debug-iphoneos/xos.app
        exit 0
    else
        echo "❌ Please install ios-deploy: brew install ios-deploy"
        exit 1
    fi
fi

echo "📱 Found device: $DEVICE_NAME ($DEVICE_UDID)"

# Get the script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Check if workspace exists
if [ ! -f "xos.xcworkspace/contents.xcworkspacedata" ]; then
    echo "❌ xos.xcworkspace not found. Please run 'pod install' first."
    exit 1
fi

# Build for device
echo "🔨 Building app for device..."
xcodebuild -workspace xos.xcworkspace \
    -scheme xos \
    -configuration Debug \
    -destination "id=$DEVICE_UDID" \
    -derivedDataPath build \
    CODE_SIGN_IDENTITY="Apple Development" \
    DEVELOPMENT_TEAM="" \
    build

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

