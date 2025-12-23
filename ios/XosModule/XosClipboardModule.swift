import Foundation
import UIKit

/// Get clipboard contents from iOS UIPasteboard
/// This function is called from Rust via FFI
@_cdecl("xos_clipboard_get_contents_ios")
public func xosClipboardGetContentsIOS() -> UnsafeMutablePointer<CChar>? {
    guard let pasteboardString = UIPasteboard.general.string else {
        return nil
    }
    
    // Don't return empty strings
    if pasteboardString.isEmpty {
        return nil
    }
    
    // Convert Swift string to C string and copy it
    // The Rust side must free this memory
    let cString = (pasteboardString as NSString).utf8String
    guard let cString = cString else {
        return nil
    }
    
    // Use strdup to allocate and copy the string
    return strdup(cString)
}

/// Set clipboard contents to iOS UIPasteboard
/// This function is called from Rust via FFI
@_cdecl("xos_clipboard_set_contents_ios")
public func xosClipboardSetContentsIOS(_ text: UnsafePointer<CChar>?) -> Int32 {
    guard let text = text else {
        return 1 // Error: null pointer
    }
    
    let string = String(cString: text)
    UIPasteboard.general.string = string
    
    return 0 // Success
}

