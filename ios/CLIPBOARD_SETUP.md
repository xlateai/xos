# iOS Clipboard Setup Instructions

The iOS clipboard functionality has been implemented and should work automatically. If you're experiencing issues, follow these steps:

## 1. Rebuild the iOS App

The clipboard module (`XosClipboardModule.swift`) is in the `XosModule/` directory and should be automatically included via CocoaPods.

```bash
cd xos/ios

# Clean and reinstall pods
rm -rf Pods/ Podfile.lock
pod install

# Rebuild the Rust library
cd ..
./build-ios.sh

# Open workspace and build
cd ios
open xos.xcworkspace
```

## 2. Verify the Swift File is Included

In Xcode:
1. Open `xos.xcworkspace`
2. Check that `XosClipboardModule.swift` appears in the Project Navigator under the Pods or XosModule group
3. If it's missing, manually add it:
   - Right-click on your project
   - Add Files to "xos"...
   - Navigate to `XosModule/XosClipboardModule.swift`
   - Make sure "Copy items if needed" is checked
   - Add to target: xos

## 3. Verify Linker Settings

The clipboard uses `UIPasteboard` which is part of UIKit (already linked by default in iOS apps).

If you get linker errors:
1. Select your project in Xcode
2. Go to Build Phases
3. Check "Link Binary With Libraries"
4. Make sure `UIKit.framework` is present (should be by default)

## 4. Test the Clipboard

The clipboard functions are:
- **Copy**: Select text, tap "copy" button
- **Cut**: Select text, tap "cut" button  
- **Paste**: Position cursor, tap "paste" button

The clipboard integrates with the iOS system clipboard, so you can:
- Copy text in xos and paste in other iOS apps
- Copy text in other iOS apps and paste in xos

## 5. Debugging

If clipboard still doesn't work, check the console logs in Xcode:
1. Run the app in Xcode (Cmd+R)
2. Open the Console (Cmd+Shift+C)
3. Try copying/pasting
4. Look for any errors related to clipboard or FFI

## How It Works

The clipboard bridge works as follows:

1. **Rust side** (`xos/src/clipboard.rs`):
   - Defines FFI functions to call Swift
   - `xos_clipboard_get_contents_ios()` - gets clipboard text
   - `xos_clipboard_set_contents_ios()` - sets clipboard text

2. **Swift side** (`xos/ios/XosModule/XosClipboardModule.swift`):
   - Implements the FFI functions with `@_cdecl` attribute
   - Uses `UIPasteboard.general.string` to access iOS clipboard

3. **No permissions required**:
   - `UIPasteboard` doesn't require special permissions
   - Works automatically on iOS 15.1+

