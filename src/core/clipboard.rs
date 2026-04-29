/// Cross-platform clipboard operations
/// 
/// Provides a simple interface for clipboard get/set operations
/// that works on macOS, iOS, and other platforms.

#[cfg(target_os = "macos")]
use std::process::Command;

#[cfg(target_os = "ios")]
use std::ffi::{CString, CStr};
#[cfg(target_os = "ios")]
use std::os::raw::c_char;

#[cfg(target_os = "windows")]
use winapi::shared::minwindef::HGLOBAL;
#[cfg(target_os = "windows")]
use winapi::um::winbase::{GlobalAlloc, GlobalFree, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
#[cfg(target_os = "windows")]
use winapi::um::winuser::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
    CF_UNICODETEXT,
};

#[cfg(target_os = "ios")]
extern "C" {
    fn xos_clipboard_get_contents_ios() -> *mut c_char;
    fn xos_clipboard_set_contents_ios(text: *const c_char) -> i32;
    fn free(ptr: *mut c_char);
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
        // Calls Swift function xosClipboardGetContentsIOS
        unsafe {
            let c_str_ptr = xos_clipboard_get_contents_ios();
            if c_str_ptr.is_null() {
                None
            } else {
                let c_str = CStr::from_ptr(c_str_ptr);
                let result = c_str.to_str().ok().map(|s| s.to_string());
                // Free the string allocated by Swift (using strdup)
                free(c_str_ptr);
                result
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        unsafe {
            if OpenClipboard(std::ptr::null_mut()) == 0 {
                return None;
            }

            let handle = GetClipboardData(CF_UNICODETEXT);
            if handle.is_null() {
                CloseClipboard();
                return None;
            }

            let ptr = GlobalLock(handle as HGLOBAL) as *const u16;
            if ptr.is_null() {
                CloseClipboard();
                return None;
            }

            let mut len = 0usize;
            while *ptr.add(len) != 0 {
                len += 1;
            }
            let slice = std::slice::from_raw_parts(ptr, len);
            let mut text = String::from_utf16(slice).ok()?;
            text = text.replace("\r\n", "\n").replace('\r', "\n");

            GlobalUnlock(handle as HGLOBAL);
            CloseClipboard();

            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        }
    }
    
    #[cfg(target_os = "linux")]
    {
        clipboard_linux_get()
    }

    #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "windows", target_os = "linux")))]
    {
        None
    }
}

#[cfg(target_os = "linux")]
fn clipboard_linux_get() -> Option<String> {
    use std::process::Command;

    fn out(args: &[&str]) -> Option<String> {
        let output = Command::new(args[0]).args(&args[1..]).output().ok()?;
        if !output.status.success() {
            return None;
        }
        let s = String::from_utf8(output.stdout).ok()?.trim_end_matches('\n').to_string();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }

    out(&["wl-paste"])
        .or_else(|| out(&["wl-paste", "-t", "text"]))
        .or_else(|| out(&["xclip", "-selection", "clipboard", "-o"]))
        .map(|t| t.replace("\r\n", "\n").replace('\r', "\n"))
}

#[cfg(target_os = "linux")]
fn clipboard_linux_set(text: &str) -> Result<(), std::io::Error> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    fn run(argv: &[&str], stdin_text: &str) -> Result<(), std::io::Error> {
        let mut child = Command::new(argv[0])
            .args(&argv[1..])
            .stdin(Stdio::piped())
            .spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(stdin_text.as_bytes())?;
        }
        let ok = child.wait()?.success();
        if ok {
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "clipboard subprocess failed",
            ))
        }
    }

    run(&["wl-copy"], text).or_else(|_| run(&["xclip", "-selection", "clipboard"], text))
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
        // Calls Swift function xosClipboardSetContentsIOS
        let c_text = CString::new(text).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, e)
        })?;
        
        unsafe {
            let result = xos_clipboard_set_contents_ios(c_text.as_ptr());
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

    #[cfg(target_os = "windows")]
    {
        unsafe {
            let normalized = text.replace("\r\n", "\n").replace('\n', "\r\n");
            let mut utf16: Vec<u16> = normalized.encode_utf16().collect();
            utf16.push(0);
            let bytes = utf16.len() * std::mem::size_of::<u16>();

            let hmem = GlobalAlloc(GMEM_MOVEABLE, bytes);
            if hmem.is_null() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "GlobalAlloc failed for clipboard",
                ));
            }

            let dst = GlobalLock(hmem) as *mut u16;
            if dst.is_null() {
                GlobalFree(hmem);
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "GlobalLock failed for clipboard",
                ));
            }

            std::ptr::copy_nonoverlapping(utf16.as_ptr(), dst, utf16.len());
            GlobalUnlock(hmem);

            if OpenClipboard(std::ptr::null_mut()) == 0 {
                GlobalFree(hmem);
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "OpenClipboard failed",
                ));
            }

            if EmptyClipboard() == 0 {
                CloseClipboard();
                GlobalFree(hmem);
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "EmptyClipboard failed",
                ));
            }

            // Ownership of hmem transfers to the system on success.
            if SetClipboardData(CF_UNICODETEXT, hmem).is_null() {
                CloseClipboard();
                GlobalFree(hmem);
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "SetClipboardData failed",
                ));
            }

            CloseClipboard();
            Ok(())
        }
    }
    
    #[cfg(target_os = "linux")]
    {
        clipboard_linux_set(text)
    }

    #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "windows", target_os = "linux")))]
    {
        let _ = text;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "clipboard not implemented on this target",
        ))
    }
}

