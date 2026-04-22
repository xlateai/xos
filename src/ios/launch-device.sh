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

# macOS extended attributes (Finder info, com.apple.provenance, etc.) cause:
#   codesign: resource fork, Finder information, or similar detritus not allowed
# Strip them from project sources and vendored bits before the build copies them into the .app.
if command -v xattr &>/dev/null; then
  echo "🧹 Stripping extended attributes (codesign)…"
  for path in xos XosModule libs; do
    if [ -e "$SCRIPT_DIR/$path" ]; then
      xattr -cr "$SCRIPT_DIR/$path" 2>/dev/null || true
    fi
  done
fi

# If Xcode’s CodeSign step fails with "resource fork / detritus", the .app is usually built but not
# signed. A Run Script in the project runs *before* "Copy Swift Standard Libraries" and AppIntents
# (which can add more files to the bundle), so stripping in a script phase is too early. Here we
# strip the finished bundle and run `codesign` again.
retry_resign_xos_app() {
  local _root=$1
  local orig_app app xcent id stage

  orig_app=$(find "$_root/build" -name "xos.app" -type d 2>/dev/null | head -1)
  if [ -z "${orig_app:-}" ] || [ ! -d "$orig_app" ]; then
    echo "   (no xos.app in build/ — cannot re-sign)"
    return 1
  fi
  xcent=$(find "$_root/build" -name "xos.app.xcent" 2>/dev/null | head -1)
  if [ -z "${xcent:-}" ] || [ ! -f "$xcent" ]; then
    echo "   (no xos.app.xcent — cannot re-sign)"
    return 1
  fi
  if ! command -v /usr/bin/codesign &>/dev/null; then
    return 1
  fi
  # Recreate the bundle without resource forks / AppleDouble. `xattr -cr` alone is often not
  # enough for codesign; ditto --norsrc is the usual fix for "detritus not allowed".
  stage="${_root}/build/_xos.norsrc.$$.$RANDOM.app"
  rm -rf "$stage"
  export COPYFILE_DISABLE=1
  if ! /usr/bin/ditto --norsrc --nocache "$orig_app" "$stage" 2>/dev/null; then
    if ! /usr/bin/ditto --norsrc "$orig_app" "$stage" 2>/dev/null; then
      echo "   (ditto --norsrc failed — cannot clone .app without resource forks)"
      rm -rf "$stage" 2>/dev/null || true
      return 1
    fi
  fi
  app="$stage"
  if command -v /usr/bin/xattr &>/dev/null; then
    /usr/bin/xattr -cr "$app" 2>/dev/null || true
  fi
  find "$app" -name '._*' -delete 2>/dev/null || true
  find "$app" -name '.DS_Store' -delete 2>/dev/null || true
  if command -v /usr/bin/dot_clean &>/dev/null; then
    /usr/bin/dot_clean -m "$app" 2>/dev/null || true
  fi

  id=$(
    (cd "$_root" && xcodebuild -workspace xos.xcworkspace -scheme xos -configuration Debug \
      -destination "generic/platform=iOS" -showBuildSettings 2>/dev/null) \
      | sed -n 's/^[[:space:]]*EXPANDED_CODE_SIGN_IDENTITY[[:space:]]*=[[:space:]]*//p' | head -1 | tr -d '\r'
  )
  if [ -z "$id" ] || [ "$id" = "-" ]; then
    id=$(
      (cd "$_root" && xcodebuild -workspace xos.xcworkspace -scheme xos -configuration Debug \
        -destination "generic/platform=iOS" -showBuildSettings 2>/dev/null) \
        | sed -n 's/^[[:space:]]*CODE_SIGN_IDENTITY[[:space:]]*=[[:space:]]*//p' | head -1 | tr -d '\r'
    )
  fi
  id="${id#"${id%%[![:space:]]*}"}"
  id="${id%"${id##*[![:space:]]}"}"
  if [ -z "$id" ] || [ "$id" = "-" ] || [ "$id" = "Sign to Run Locally" ]; then
    echo "   (could not read signing identity from xcodebuild -showBuildSettings)"
    return 1
  fi

  while IFS= read -r fw; do
    [ -n "$fw" ] && [ -d "$fw" ] && /usr/bin/codesign -f -s "$id" --timestamp=none "$fw" 2>/dev/null || true
  done < <(find "$app" -name "*.framework" -type d 2>/dev/null)
  while IFS= read -r df; do
    [ -n "$df" ] && [ -f "$df" ] && /usr/bin/codesign -f -s "$id" --timestamp=none "$df" 2>/dev/null || true
  done < <(find "$app" -name "*.dylib" 2>/dev/null)
  if /usr/bin/codesign -f -s "$id" --entitlements "$xcent" --generate-entitlement-der --timestamp=none "$app"; then
    rm -rf "$orig_app"
    if ! /bin/mv "$app" "$orig_app" 2>/dev/null; then
      echo "   (codesign ok but could not replace $orig_app — left at $app)"
      return 1
    fi
    echo "✅ Re-signed the app after ditto --norsrc + xattr (codesign retried once)."
    return 0
  fi
  rm -rf "$stage" 2>/dev/null || true
  echo "   (manual codesign still failed after ditto --norsrc — try moving the repo off iCloud Desktop, or: COPYFILE_DISABLE=1 xcodebuild …)"
  return 1
}

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
# Avoid copy metadata / resource forks from build inputs into the .app
COPYFILE_DISABLE=1 xcodebuild -workspace xos.xcworkspace \
    -scheme xos \
    -configuration Debug \
    -destination "id=$DEVICE_UDID" \
    -derivedDataPath build \
    -allowProvisioningUpdates \
    CODE_SIGN_STYLE=Automatic \
    XOS_DEFAULT_APP="$APP_NAME" \
    build 2>&1 | tee "$BUILD_OUTPUT"
BUILD_STATUS=${PIPESTATUS[0]}
set -e  # Re-enable exit on error

if [ $BUILD_STATUS -ne 0 ]; then
    if grep -qE "resource fork|detritus not allowed" "$BUILD_OUTPUT" 2>/dev/null; then
        echo ""
        echo "🔧 The app bundle was built, but CodeSign hit extended-attribute metadata. Stripping the built .app and re-signing…"
        if retry_resign_xos_app "$SCRIPT_DIR"; then
            BUILD_STATUS=0
        fi
    fi
fi

if [ $BUILD_STATUS -ne 0 ]; then
    # Check if the error is related to signing
    if grep -q "No Account for Team\|No profiles for\|requires a development team" "$BUILD_OUTPUT" 2>/dev/null; then
        echo ""
        echo "❌ Code signing is not configured."
        echo ""
        echo "📋 Please set up code signing in Xcode:"
        echo ""
        echo "   You can open the workspace with:"
        echo "      xed src/ios/"
        echo "   Or:"
        echo "      open src/ios/xos.xcworkspace"
        echo ""
        echo "   In Xcode:"
        echo "   1. Select the 'xos' project in the left navigator"
        echo "   2. Select the 'xos' target"
        echo "   3. Go to the 'Signing & Capabilities' tab"
        echo "   4. Check 'Automatically manage signing'"
        echo "   5. Select your Team (your Apple ID)"
        echo ""
        echo "   If you don't have a team:"
        echo "   - Xcode > Settings > Accounts"
        echo "   - Click '+' to add your Apple ID"
        echo "   - Then go back to Signing & Capabilities and select it"
        echo ""
        
        # Offer to open Xcode
        if command -v xed &> /dev/null; then
            echo "   Would you like me to open the workspace in Xcode? (Y/n): "
            read -r response
            if [[ -z "$response" || "$response" =~ ^[Yy] ]]; then
                echo "   Opening workspace in Xcode..."
                xed src/ios/ 2>/dev/null || open src/ios/xos.xcworkspace 2>/dev/null
            fi
        fi
        
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
    # Try to install and launch, but don't fail if launch doesn't work
    if ios-deploy --bundle "$APP_BUNDLE" --justlaunch 2>&1 | tee /tmp/ios-deploy-output.log; then
        echo "✅ App launched!"
    else
        # Check if install succeeded but launch failed
        if grep -q "InstallComplete\|Installed package" /tmp/ios-deploy-output.log 2>/dev/null; then
            echo "✅ App installed successfully!"
            echo "⚠️  Launch failed (this is a known ios-deploy issue with newer iOS versions)"
            echo "   The app is installed on your device - please launch it manually from the home screen."
            echo ""
            echo "   To launch from command line, try:"
            echo "   xcrun devicectl device process launch --device $DEVICE_UDID com.xlate.xos"
        else
            echo "⚠️  Installation may have failed. Check the output above."
        fi
        rm -f /tmp/ios-deploy-output.log
    fi
elif command -v xcrun &> /dev/null && xcrun devicectl &> /dev/null 2>&1; then
    echo "📲 Installing app using xcrun devicectl..."
    # Modern iOS deployment (Xcode 15+)
    if xcrun devicectl device install app --device "$DEVICE_UDID" "$APP_BUNDLE"; then
        echo "✅ App installed!"
        echo "📲 Attempting to launch app..."
        if xcrun devicectl device process launch --device "$DEVICE_UDID" com.xlate.xos 2>/dev/null; then
            echo "✅ App launched!"
        else
            echo "⚠️  App installed but couldn't launch automatically."
            echo "   Please launch it manually from your device's home screen."
        fi
    else
        echo "⚠️  Installation failed. Check the error messages above."
    fi
else
    echo "⚠️  ios-deploy not found. App is built but not installed."
    echo "   Install with: brew install ios-deploy"
    echo "   Or use Xcode to build and run on device."
    echo "   App bundle location: $APP_BUNDLE"
fi

