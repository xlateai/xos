/// Cross-platform clipboard operations
/// 
/// Provides a simple interface for clipboard get/set operations
/// that works on macOS, iOS, and other platforms.

use std::process::Command;

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
        // iOS requires UIPasteboard access through Swift/Objective-C
        // For now, use pbpaste as fallback (works in simulator)
        // TODO: Add proper iOS clipboard support via FFI
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
        // iOS requires UIPasteboard access through Swift/Objective-C
        // For now, use pbcopy as fallback (works in simulator)
        // TODO: Add proper iOS clipboard support via FFI
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
    
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    {
        // For other platforms, do nothing
        // TODO: Add Linux/Windows clipboard support
        Ok(())
    }
}

