#!/bin/bash
# Script to detect and save development team for code signing
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

CONFIG_FILE="$SCRIPT_DIR/.xcode-signing-config"

# Function to detect development team from Xcode
detect_team() {
    # Try to get team from existing project if it exists
    if [ -f "xos.xcodeproj/project.pbxproj" ]; then
        # Extract team from project file
        TEAM=$(grep -A 5 "DEVELOPMENT_TEAM" xos.xcodeproj/project.pbxproj | grep -v "DEVELOPMENT_TEAM = \"\";" | grep "DEVELOPMENT_TEAM" | head -1 | sed -E 's/.*DEVELOPMENT_TEAM = "([^"]+)";.*/\1/' || echo "")
        if [ -n "$TEAM" ] && [ "$TEAM" != "\$(DEVELOPMENT_TEAM)" ]; then
            echo "$TEAM"
            return 0
        fi
    fi
    
    # Try to get from xcodebuild
    if command -v xcodebuild &> /dev/null; then
        # Get team from accounts
        TEAM=$(xcodebuild -showBuildSettings -project xos.xcodeproj -target xos 2>/dev/null | grep "DEVELOPMENT_TEAM" | head -1 | sed -E 's/.*= ([^ ]+).*/\1/' || echo "")
        if [ -n "$TEAM" ] && [ "$TEAM" != "\$(DEVELOPMENT_TEAM)" ]; then
            echo "$TEAM"
            return 0
        fi
    fi
    
    # Try to get from security (certificate)
    TEAM=$(security find-identity -v -p codesigning 2>/dev/null | grep "Apple Development" | head -1 | sed -E 's/.*\(([A-Z0-9]+)\).*/\1/' || echo "")
    if [ -n "$TEAM" ]; then
        echo "$TEAM"
        return 0
    fi
    
    return 1
}

# Load saved team if exists
if [ -f "$CONFIG_FILE" ]; then
    source "$CONFIG_FILE"
fi

# Try to detect team
if [ -z "$DEVELOPMENT_TEAM" ]; then
    echo "🔍 Detecting development team..."
    DETECTED_TEAM=$(detect_team || echo "")
    
    if [ -n "$DETECTED_TEAM" ]; then
        DEVELOPMENT_TEAM="$DETECTED_TEAM"
        echo "✅ Found team: $DEVELOPMENT_TEAM"
        # Save it
        echo "DEVELOPMENT_TEAM=\"$DEVELOPMENT_TEAM\"" > "$CONFIG_FILE"
    else
        echo "⚠️  Could not automatically detect development team."
        echo ""
        echo "Please set it manually:"
        echo "  1. Open xos.xcworkspace in Xcode"
        echo "  2. Select 'xos' project > 'xos' target"
        echo "  3. Go to 'Signing & Capabilities'"
        echo "  4. Select your Team"
        echo ""
        echo "Or set it via environment variable:"
        echo "  export DEVELOPMENT_TEAM=\"YOUR_TEAM_ID\""
        echo ""
        echo "Then run this script again or regenerate the project."
        exit 1
    fi
else
    echo "✅ Using saved development team: $DEVELOPMENT_TEAM"
fi

# Export for use in project generation
export DEVELOPMENT_TEAM

# Regenerate project with team
if command -v xcodegen &> /dev/null; then
    echo "🔄 Regenerating Xcode project with code signing configuration..."
    xcodegen generate
    echo "✅ Project regenerated with development team: $DEVELOPMENT_TEAM"
else
    echo "⚠️  xcodegen not found. Install with: brew install xcodegen"
    echo "   Team will be set when you regenerate the project."
fi

