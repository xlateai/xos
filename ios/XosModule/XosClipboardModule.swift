import Foundation
import UIKit

// C-compatible function declarations for Rust clipboard functions
@_silgen_name("xos_clipboard_get_contents")
func xos_clipboard_get_contents_rust() -> UnsafeMutablePointer<CChar>?

@_silgen_name("xos_clipboard_set_contents")
func xos_clipboard_set_contents_rust(_ text: UnsafePointer<CChar>?) -> Int32

@_silgen_name("xos_clipboard_free_string")
func xos_clipboard_free_string(_ ptr: UnsafeMutablePointer<CChar>?)

// Swift implementation that bridges to UIPasteboard
@_cdecl("xos_clipboard_get_contents")
public func xosClipboardGetContents() -> UnsafeMutablePointer<CChar>? {
    guard let pasteboardString = UIPasteboard.general.string else {
        return nil
    }
    
    // Convert Swift string to C string
    guard let cString = pasteboardString.cString(using: .utf8) else {
        return nil
    }
    
    // Allocate memory for the C string and copy
    let length = cString.count
    let buffer = UnsafeMutablePointer<CChar>.allocate(capacity: length)
    buffer.initialize(from: cString, count: length)
    
    return buffer
}

@_cdecl("xos_clipboard_set_contents")
public func xosClipboardSetContents(_ text: UnsafePointer<CChar>?) -> Int32 {
    guard let text = text else {
        return 1 // Error: null pointer
    }
    
    let string = String(cString: text)
    UIPasteboard.general.string = string
    
    return 0 // Success
}

/// Swift wrapper for getting clipboard contents
public func xosClipboardGetContentsSwift() -> String? {
    return UIPasteboard.general.string
}

/// Swift wrapper for setting clipboard contents
public func xosClipboardSetContentsSwift(_ text: String) {
    UIPasteboard.general.string = text
}

