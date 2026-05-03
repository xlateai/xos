#!/bin/bash
# Don't use set -e here - we want to handle build errors gracefully
set -u  # Only fail on undefined variables

# Get app name from environment or use default
APP_NAME="${XOS_APP_NAME:-blank}"

echo "📱 Launching XOS iOS app ($APP_NAME) on device..."

# Get the script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Project trees under iCloud (Desktop / Documents) often get extended attributes on *build* outputs;
# the final `codesign` of the .app then fails with "resource fork / detritus".
#
# Default DerivedData under src/ios/build/DerivedData (gitignored): /tmp-based paths are often cleared
# mid-build; mixed Swift + Obj-C Pods then fail with missing ExplicitPrecompiledModules (*.pcm).
# Override: XOS_IOS_DERIVED_DATA=/path/to/DerivedData
DERIVED_DATA_PATH="${XOS_IOS_DERIVED_DATA:-$SCRIPT_DIR/build/DerivedData}"
mkdir -p "$DERIVED_DATA_PATH" 2>/dev/null || {
  echo "❌ Could not create DerivedData directory: $DERIVED_DATA_PATH"
  exit 1
}

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
strip_bundle_metadata() {
  local app=$1
  if command -v /usr/bin/xattr &>/dev/null; then
    /usr/bin/xattr -cr "$app" 2>/dev/null || true
    # Per-item clear: some volumes still leave com.apple.provenance / FinderInfo until each inode is cleared.
    while IFS= read -r -d '' p; do
      /usr/bin/xattr -c "$p" 2>/dev/null || true
    done < <(find "$app" -print0 2>/dev/null)
  fi
  find "$app" -name '._*' -delete 2>/dev/null || true
  find "$app" -name '.DS_Store' -delete 2>/dev/null || true
  if command -v /usr/bin/dot_clean &>/dev/null; then
    /usr/bin/dot_clean -m "$app" 2>/dev/null || true
  fi
}

retry_resign_xos_app() {
  local _root=$1
  local _derived=$2
  local orig_app app xcent id stage_dir

  orig_app=$(find "$_derived" -name "xos.app" -type d 2>/dev/null | head -1)
  if [ -z "${orig_app:-}" ] || [ ! -d "$orig_app" ]; then
    echo "   (no xos.app under DerivedData — cannot re-sign: $_derived)"
    return 1
  fi
  xcent=$(find "$_derived" -name "xos.app.xcent" 2>/dev/null | head -1)
  if [ -z "${xcent:-}" ] || [ ! -f "$xcent" ]; then
    echo "   (no xos.app.xcent — cannot re-sign)"
    return 1
  fi
  if ! command -v /usr/bin/codesign &>/dev/null; then
    return 1
  fi
  # Staging *outside* the project build dir avoids iCloud Desktop / some APFS + sync volumes
  # re-applying xattrs that break codesign. Sign in /private/tmp, then ditto the sealed bundle back.
  stage_dir=$(/usr/bin/mktemp -d /private/tmp/xos.resign.XXXXXX 2>/dev/null) || {
    echo "   (mktemp in /private/tmp failed — cannot stage bundle off the build tree)"
    return 1
  }
  app="${stage_dir}/xos.app"
  export COPYFILE_DISABLE=1
  if ! /usr/bin/ditto --norsrc --nocache "$orig_app" "$app" 2>/dev/null; then
    if ! /usr/bin/ditto --norsrc "$orig_app" "$app" 2>/dev/null; then
      echo "   (ditto --norsrc failed — cannot clone .app without resource forks)"
      rm -rf "$stage_dir" 2>/dev/null || true
      return 1
    fi
  fi
  strip_bundle_metadata "$app"

  # Drop partial / stale signatures before re-signing (Xcode can leave a half-updated state).
  # For a bundle, this removes the main executable and nested code signatures in one go.
  /usr/bin/codesign --remove-signature "$app" 2>/dev/null || true
  strip_bundle_metadata "$app"

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

  _sign_err=0
  while IFS= read -r fw; do
    if [ -n "$fw" ] && [ -d "$fw" ]; then
      if ! /usr/bin/codesign -f -s "$id" --timestamp=none "$fw" 2>&1; then
        echo "   ⚠️  codesign failed for framework: $fw" >&2
        _sign_err=1
      fi
    fi
  done < <(find "$app" -name "*.framework" -type d 2>/dev/null)
  while IFS= read -r df; do
    if [ -n "$df" ] && [ -f "$df" ]; then
      if ! /usr/bin/codesign -f -s "$id" --timestamp=none "$df" 2>&1; then
        echo "   ⚠️  codesign failed for dylib: $df" >&2
        _sign_err=1
      fi
    fi
  done < <(find "$app" -name "*.dylib" -type f 2>/dev/null)
  if [ "$_sign_err" -ne 0 ]; then
    echo "   (inner sign had failures — install may be unstable; see above)"
  fi
  if /usr/bin/codesign -f -s "$id" --entitlements "$xcent" --generate-entitlement-der --timestamp=none "$app"; then
    if ! /usr/bin/codesign -v --verbose=4 "$app" 2>&1; then
      echo "   ⚠️  codesign --verify failed on re-signed .app" >&2
    fi
    rm -rf "$orig_app"
    if ! COPYFILE_DISABLE=1 /usr/bin/ditto --norsrc --nocache "$app" "$orig_app" 2>/dev/null; then
      if ! COPYFILE_DISABLE=1 /usr/bin/ditto --norsrc "$app" "$orig_app" 2>/dev/null; then
        echo "   (codesign ok in /private/tmp but could not copy back to: $orig_app — bundle left in $stage_dir)" >&2
        return 1
      fi
    fi
    rm -rf "$stage_dir" 2>/dev/null || true
    echo "✅ Re-signed the app after staging in /private/tmp (ditto --norsrc + xattr) and copied back."
    return 0
  fi
  echo "   (verbose codesign — last attempt, for diagnosis:)"
  /usr/bin/codesign -f -s "$id" --entitlements "$xcent" --generate-entitlement-der --timestamp=none --verbose=4 "$app" 2>&1 || true
  if command -v /usr/bin/xattr &>/dev/null; then
    echo "   (sample xattrs on bundle — if non-empty, something still has metadata:)"
    /usr/bin/xattr -lr "$app" 2>&1 | head -40
  fi
  rm -rf "$stage_dir" 2>/dev/null || true
  echo "   (manual codesign still failed — try cloning the repo to ~/Developer (not Desktop/iCloud) or: rm -rf build && set COPYFILE_DISABLE=1 for xcodebuild)"
  return 1
}

# Build for device
echo "🔨 Building app for device..."

# Build with automatic code signing and allow provisioning updates
# This allows xcodebuild to automatically create provisioning profiles
# We don't force a DEVELOPMENT_TEAM - let the project settings handle it
echo "📝 Using project's signing configuration..."
echo "📂 DerivedData: $DERIVED_DATA_PATH"
echo ""

# Try to build - if signing fails, provide helpful instructions
BUILD_OUTPUT=$(mktemp)
set +e  # Temporarily disable exit on error to capture output

# Drop stale explicit Clang modules so ObjC Pod sources don't reference missing .pcm from a half build.
if [ "${XOS_IOS_SCRUB_MODULES:-1}" != "0" ] && [ -n "$DERIVED_DATA_PATH" ]; then
    rm -rf "$DERIVED_DATA_PATH/Build/Intermediates.noindex/ExplicitPrecompiledModules" 2>/dev/null || true
    rm -rf "$DERIVED_DATA_PATH/ModuleCache.noindex" 2>/dev/null || true
fi

# Avoid copy metadata / resource forks from build inputs into the .app
COPYFILE_DISABLE=1 xcodebuild -workspace xos.xcworkspace \
    -scheme xos \
    -configuration Debug \
    -destination "id=$DEVICE_UDID" \
    -derivedDataPath "$DERIVED_DATA_PATH" \
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
        if retry_resign_xos_app "$SCRIPT_DIR" "$DERIVED_DATA_PATH"; then
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

    # Stale Clang explicit modules → missing .pcm (common after /tmp clean or Xcode upgrade).
    if grep -qE "ExplicitPrecompiledModules/.*\\.pcm.*not found|module file .*\\.pcm.*not found" "$BUILD_OUTPUT" 2>/dev/null; then
        echo ""
        echo "🔧 Clang module cache mismatch (missing .pcm). Try a clean iOS DerivedData:"
        echo "     rm -rf \"$DERIVED_DATA_PATH\""
        echo "   Or scrub only explicit modules next run is automatic; for a full wipe also:"
        echo "     rm -rf \"$SCRIPT_DIR/build/DerivedData\""
        echo "   Override location: export XOS_IOS_DERIVED_DATA=/path/you/control"
        echo ""
    fi
    
    # Other build errors
    echo ""
    echo "❌ Build failed. Check the error messages above."
    rm -f "$BUILD_OUTPUT"
    exit 1
fi

rm -f "$BUILD_OUTPUT"

# Find the .app bundle
APP_BUNDLE=$(find "$DERIVED_DATA_PATH" -name "xos.app" -type d 2>/dev/null | head -1)

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

