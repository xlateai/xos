#!/bin/bash
set -e

echo "📱 Creating Xcode project for iOS app..."

# Create Xcode project using xcodegen or manual creation
# For now, we'll provide instructions and create a basic structure

PROJECT_NAME="xos"
SCHEME_NAME="xos"

echo "⚠️  Automatic Xcode project creation requires Xcode command line tools."
echo "   Please create the project manually in Xcode:"
echo ""
echo "   1. Open Xcode"
echo "   2. File > New > Project"
echo "   3. Choose 'iOS' > 'App'"
echo "   4. Product Name: xos"
echo "   5. Interface: Storyboard (or SwiftUI)"
echo "   6. Language: Swift"
echo "   7. Save to: $(pwd)"
echo "   8. Move the .xcodeproj into the ios/ directory"
echo ""
echo "   Or use this command to create it programmatically:"
echo "   (requires xcodegen: brew install xcodegen)"
echo ""

# Check if xcodegen is available
if command -v xcodegen &> /dev/null; then
    echo "✅ xcodegen found, creating project..."
    # Create project.yml for xcodegen
    cat > project.yml <<EOF
name: $PROJECT_NAME
options:
  bundleIdPrefix: com.xlate
  deploymentTarget:
    iOS: "15.1"
targets:
  $PROJECT_NAME:
    type: application
    platform: iOS
    deploymentTarget: "15.1"
    sources:
      - path: xos
    settings:
      PRODUCT_BUNDLE_IDENTIFIER: com.xlate.xos
      SWIFT_VERSION: "5.9"
      INFOPLIST_FILE: xos/Info.plist
      CODE_SIGN_IDENTITY: "Apple Development"
EOF
    xcodegen generate
    echo "✅ Xcode project created!"
else
    echo "❌ xcodegen not found. Please install it with: brew install xcodegen"
    echo "   Or create the project manually in Xcode as described above."
    exit 1
fi

