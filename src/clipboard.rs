/// Cross-platform clipboard operations
/// 
/// Provides a simple interface for clipboard get/set operations
/// that works on macOS, iOS, and other platforms.

use std::process::Command;

#[cfg(target_os = "ios")]
use std::ffi::{CString, CStr};
#[cfg(target_os = "ios")]
use std::os::raw::c_char;

#[cfg(target_os = "ios")]
extern "C" {
    fn xos_clipboard_get_contents() -> *mut c_char;
    fn xos_clipboard_set_contents(text: *const c_char) -> i32;
    fn xos_clipboard_free_string(ptr: *mut c_char);
}

/// Get the current clipboard contents
pub fn get_contents() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        Command::new("pbpaste")
            .output()
            .ok()
            .and_then(|output| {
                let text = String::from_utf8(output.stdout).ok()?;
                if text.is_empty() {
                    None
                } else {
                    Some(text)
                }
            })
    }
    
    #[cfg(target_os = "ios")]
    {
        // Use iOS FFI to access UIPasteboard
        // Note: This requires Swift implementation on the iOS side
        unsafe {
            let c_str_ptr = xos_clipboard_get_contents();
            if c_str_ptr.is_null() {
                None
            } else {
                let c_str = CStr::from_ptr(c_str_ptr);
                let result = c_str.to_str().ok().map(|s| s.to_string());
                xos_clipboard_free_string(c_str_ptr);
                result
            }
        }
    }
    
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    {
        // For other platforms, return None
        // TODO: Add Linux/Windows clipboard support
        None
    }
}

/// Set the clipboard contents
pub fn set_contents(text: &str) -> Result<(), std::io::Error> {
    #[cfg(target_os = "macos")]
    {
        use std::io::Write;
        let mut child = Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()?;
        
        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(text.as_bytes())?;
        }
        child.wait()?;
        Ok(())
    }
    
    #[cfg(target_os = "ios")]
    {
        // Use iOS FFI to access UIPasteboard
        // Note: This requires Swift implementation on the iOS side
        let c_text = CString::new(text).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, e)
        })?;
        
        unsafe {
            let result = xos_clipboard_set_contents(c_text.as_ptr());
            if result == 0 {
                Ok(())
            } else {
                Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Failed to set clipboard contents"
                ))
            }
        }
    }
    
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    {
        // For other platforms, do nothing
        // TODO: Add Linux/Windows clipboard support
        Ok(())
    }
}

