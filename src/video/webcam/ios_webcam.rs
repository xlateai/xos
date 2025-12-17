use std::cell::RefCell;
use std::sync::Mutex;

use crate::print;

// Thread-local storage for camera state
thread_local! {
    static CAMERA_INITIALIZED: RefCell<bool> = RefCell::new(false);
}

// Global mutex to ensure thread-safe camera access
// Note: iOS FFI calls are typically from the main thread, but we use a mutex for safety
static CAMERA_LOCK: Mutex<()> = Mutex::new(());

/// Initializes the camera (must be called before using `get_frame` or `get_resolution`)
pub fn init_camera() {
    let _lock = CAMERA_LOCK.lock().unwrap();
    
    print("[Webcam] Initializing camera...");
    let result = unsafe { xos_webcam_init() };
    if result == 0 {
        CAMERA_INITIALIZED.with(|cell| {
            *cell.borrow_mut() = true;
        });
        print("[Webcam] Camera initialized successfully");
    } else {
        panic!("Failed to initialize camera on iOS");
    }
}

/// Gets the current camera resolution
pub fn get_resolution() -> (u32, u32) {
    let _lock = CAMERA_LOCK.lock().unwrap();
    
    let is_initialized = CAMERA_INITIALIZED.with(|cell| {
        *cell.borrow()
    });
    
    if !is_initialized {
        return (0, 0);
    }
    
    let mut width: u32 = 0;
    let mut height: u32 = 0;
    
    let result = unsafe {
        xos_webcam_get_resolution(&mut width, &mut height)
    };
    
    if result == 0 {
        (width, height)
    } else {
        (0, 0)
    }
}

/// Captures the latest frame from the camera
pub fn get_frame() -> Vec<u8> {
    let _lock = CAMERA_LOCK.lock().unwrap();
    
    let is_initialized = CAMERA_INITIALIZED.with(|cell| {
        *cell.borrow()
    });
    
    if !is_initialized {
        return vec![];
    }
    
    // First, get the resolution to know how much data we need
    // We need to get resolution without locking again (we already have the lock)
    let mut width: u32 = 0;
    let mut height: u32 = 0;
    
    let res_result = unsafe {
        xos_webcam_get_resolution(&mut width, &mut height)
    };
    
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
}

