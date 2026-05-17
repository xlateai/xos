#[cfg(target_os = "ios")]
use std::ffi::CString;
#[cfg(target_os = "ios")]
use std::os::raw::c_char;
#[cfg(target_os = "ios")]
use std::sync::OnceLock;

#[cfg(target_os = "ios")]
static LOG_CALLBACK: OnceLock<extern "C" fn(*const c_char)> = OnceLock::new();

#[cfg(target_os = "ios")]
#[no_mangle]
pub extern "C" fn xos_set_log_callback(callback: extern "C" fn(*const c_char)) {
    let _ = LOG_CALLBACK.set(callback);
}

#[cfg(target_os = "ios")]
pub fn log_to_ios(message: &str) {
    if let Some(callback) = LOG_CALLBACK.get() {
        if let Ok(c_str) = CString::new(message) {
            callback(c_str.as_ptr());
        }
    }
}

#[cfg(not(target_os = "ios"))]
pub fn log_to_ios(_message: &str) {}
