//! Logging infrastructure for the Coder app terminal
//! Captures Rust/Swift logs and displays them in the Python terminal

use std::sync::{Arc, Mutex, OnceLock};

/// Global log buffer that the coder terminal reads from
static CODER_LOG_BUFFER: OnceLock<Arc<Mutex<String>>> = OnceLock::new();

/// Get or initialize the global log buffer
fn get_log_buffer() -> &'static Arc<Mutex<String>> {
    CODER_LOG_BUFFER.get_or_init(|| Arc::new(Mutex::new(String::new())))
}

/// Enable logging to the coder terminal
pub fn enable_coder_logging() {
    // Initialize the buffer
    let _ = get_log_buffer();
}

/// Disable logging to the coder terminal
pub fn disable_coder_logging() {
    // Clear the buffer
    if let Some(buffer) = CODER_LOG_BUFFER.get() {
        if let Ok(mut buf) = buffer.lock() {
            buf.clear();
        }
    }
}

/// Write a log message to the coder terminal
pub fn log_to_coder(message: &str) {
    if let Some(buffer) = CODER_LOG_BUFFER.get() {
        if let Ok(mut buf) = buffer.lock() {
            buf.push_str(message);
            if !message.ends_with('\n') {
                buf.push('\n');
            }
        }
    }
}

/// Read and clear pending logs from the buffer
pub fn read_pending_logs() -> String {
    if let Some(buffer) = CODER_LOG_BUFFER.get() {
        if let Ok(mut buf) = buffer.lock() {
            let logs = buf.clone();
            buf.clear();
            logs
        } else {
            String::new()
        }
    } else {
        String::new()
    }
}

/// Check if there are pending logs
pub fn has_pending_logs() -> bool {
    if let Some(buffer) = CODER_LOG_BUFFER.get() {
        if let Ok(buf) = buffer.lock() {
            !buf.is_empty()
        } else {
            false
        }
    } else {
        false
    }
}

