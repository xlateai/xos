use std::cell::RefCell;
use std::sync::Mutex;

use crate::print;

// Global mutex to ensure thread-safe camera access
// Note: iOS FFI calls are typically from the main thread, but we use a mutex for safety
static CAMERA_LOCK: Mutex<()> = Mutex::new(());

/// Initializes the camera (must be called before using `get_frame` or `get_resolution`)
/// Note: This is now async and non-blocking. The camera will initialize in the background.
pub fn init_camera() {
    let _lock = CAMERA_LOCK.lock().unwrap();
    
    print("[Webcam] Starting camera initialization (async)...");
    // Init returns immediately - actual setup happens async in Swift
    unsafe { xos_webcam_init() };
}

/// Gets the current camera resolution
/// Returns (0, 0) if camera is not initialized or not ready
pub fn get_resolution() -> (u32, u32) {
    let _lock = CAMERA_LOCK.lock().unwrap();
    
    let mut width: u32 = 0;
    let mut height: u32 = 0;
    
    let result = unsafe {
        xos_webcam_get_resolution(&mut width, &mut height)
    };
    
    if result == 0 && width > 0 && height > 0 {
        (width, height)
    } else {
        (0, 0)
    }
}

/// Captures the latest frame from the camera
/// Returns empty vector if camera is not initialized or no frame is available
pub fn get_frame() -> Vec<u8> {
    let _lock = CAMERA_LOCK.lock().unwrap();
    
    // First, get the resolution to know how much data we need
    let mut width: u32 = 0;
    let mut height: u32 = 0;
    
    let res_result = unsafe {
        xos_webcam_get_resolution(&mut width, &mut height)
    };
    
    // If resolution is 0 or error, camera is not ready yet
    if res_result != 0 || width == 0 || height == 0 {
        return vec![];
    }
    
    let buffer_size = (width * height * 3) as usize; // RGB format
    let mut buffer = vec![0u8; buffer_size];
    
    let result = unsafe {
        xos_webcam_get_frame(
            buffer.as_mut_ptr(),
            buffer_size,
        )
    };
    
    if result > 0 {
        // Result contains the actual number of bytes written
        // Truncate if necessary (shouldn't happen, but be safe)
        if (result as usize) < buffer_size {
            buffer.truncate(result as usize);
        }
        buffer
    } else {
        // Error or no frame available, return empty buffer
        vec![]
    }
}

/// Gets the number of available cameras
pub fn get_camera_count() -> usize {
    let _lock = CAMERA_LOCK.lock().unwrap();
    unsafe { xos_webcam_get_camera_count() as usize }
}

/// Gets the name of a camera at the given index
pub fn get_camera_name(index: usize) -> Option<String> {
    let _lock = CAMERA_LOCK.lock().unwrap();
    
    let mut buffer = vec![0u8; 256];
    let result = unsafe {
        xos_webcam_get_camera_name(
            index as i32,
            buffer.as_mut_ptr() as *mut i8,
            buffer.len() as i32,
        )
    };
    
    if result > 0 {
        // Find null terminator
        let len = buffer.iter().position(|&b| b == 0).unwrap_or(buffer.len());
        String::from_utf8(buffer[..len].to_vec()).ok()
    } else {
        None
    }
}

/// Switches to a different camera by index
pub fn switch_camera(index: usize) -> bool {
    let _lock = CAMERA_LOCK.lock().unwrap();
    unsafe { xos_webcam_switch_camera(index as i32) == 0 }
}

/// Gets the current camera index
pub fn get_current_camera_index() -> usize {
    let _lock = CAMERA_LOCK.lock().unwrap();
    unsafe { xos_webcam_get_current_camera_index() as usize }
}

// FFI declarations for iOS webcam functions
#[cfg(target_os = "ios")]
extern "C" {
    /// Initialize the camera
    /// Returns 0 on success, non-zero on error
    fn xos_webcam_init() -> std::os::raw::c_int;
    
    /// Get the camera resolution
    /// Returns 0 on success, non-zero on error
    /// width and height are filled in by the function
    fn xos_webcam_get_resolution(
        width: *mut u32,
        height: *mut u32,
    ) -> std::os::raw::c_int;
    
    /// Get the latest frame from the camera
    /// buffer: Pointer to buffer to fill with RGB data
    /// buffer_size: Size of the buffer in bytes
    /// Returns the number of bytes written on success, 0 or negative on error
    fn xos_webcam_get_frame(
        buffer: *mut u8,
        buffer_size: usize,
    ) -> std::os::raw::c_int;
    
    /// Get the number of available cameras
    /// Returns the count of available cameras
    fn xos_webcam_get_camera_count() -> std::os::raw::c_int;
    
    /// Get the name of a camera at the given index
    /// index: Camera index
    /// buffer: Buffer to write the camera name (null-terminated C string)
    /// bufferSize: Size of the buffer
    /// Returns the number of bytes written (excluding null terminator)
    fn xos_webcam_get_camera_name(
        index: std::os::raw::c_int,
        buffer: *mut std::os::raw::c_char,
        bufferSize: std::os::raw::c_int,
    ) -> std::os::raw::c_int;
    
    /// Switch to a different camera by index
    /// index: Camera index to switch to
    /// Returns 0 on success, non-zero on error
    fn xos_webcam_switch_camera(index: std::os::raw::c_int) -> std::os::raw::c_int;
    
    /// Get the current camera index
    /// Returns the current camera index
    fn xos_webcam_get_current_camera_index() -> std::os::raw::c_int;
}

