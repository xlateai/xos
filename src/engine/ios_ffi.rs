#[cfg(target_os = "ios")]
use std::ffi::CString;
#[cfg(target_os = "ios")]
use std::io::{self, Write};
#[cfg(target_os = "ios")]
use std::os::raw::c_char;
#[cfg(target_os = "ios")]
use std::panic;
#[cfg(target_os = "ios")]
use std::ptr;
#[cfg(target_os = "ios")]
use std::sync::{Mutex, OnceLock};

#[cfg(target_os = "ios")]
use crate::apps;
#[cfg(target_os = "ios")]
use crate::engine::{Application, EngineState, MouseState};
#[cfg(target_os = "ios")]
use crate::engine::engine::CursorStyleSetter;
#[cfg(target_os = "ios")]
use crate::tensor::array::{Array, Device};

// Global engine state for iOS
// Note: We use unsafe Send impl because Application trait objects are not Send,
// but in practice iOS FFI calls are single-threaded from the main thread
#[cfg(target_os = "ios")]
static ENGINE_STATE: Mutex<Option<IosEngineState>> = Mutex::new(None);

#[cfg(target_os = "ios")]
struct IosEngineState {
    app: Box<dyn Application>,
    engine_state: EngineState,
    width: u32,
    height: u32,
}

// Unsafe Send implementation - safe because iOS FFI is called from main thread only
#[cfg(target_os = "ios")]
unsafe impl Send for IosEngineState {}

// FFI function pointer for logging (set by Swift)
#[cfg(target_os = "ios")]
static LOG_CALLBACK: OnceLock<extern "C" fn(*const c_char)> = OnceLock::new();

/// Set the logging callback function (called from Swift)
#[cfg(target_os = "ios")]
#[no_mangle]
pub extern "C" fn xos_set_log_callback(callback: extern "C" fn(*const c_char)) {
    let _ = LOG_CALLBACK.set(callback);
}

/// Helper function to log a message to Swift's console
#[cfg(target_os = "ios")]
pub fn log_to_ios(message: &str) {
    if let Some(callback) = LOG_CALLBACK.get() {
        if let Ok(c_str) = CString::new(message) {
            callback(c_str.as_ptr());
        }
    }
    // Note: We don't also print to stderr here to avoid duplicates
    // The Swift console manager handles all logging
}

/// Custom writer that forwards to Swift's logging system
#[cfg(target_os = "ios")]
struct IosLogWriter {
    buffer: Vec<u8>,
}

#[cfg(target_os = "ios")]
impl IosLogWriter {
    fn new() -> Self {
        Self { buffer: Vec::new() }
    }
}

#[cfg(target_os = "ios")]
impl Write for IosLogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if !self.buffer.is_empty() {
            let text = String::from_utf8_lossy(&self.buffer);
            log_to_ios(&text);
            self.buffer.clear();
        }
        Ok(())
    }
}

/// Initialize stdout/stderr redirection to iOS logging
#[cfg(target_os = "ios")]
fn setup_logging() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    
    INIT.call_once(|| {
        // Set up panic hook to log panics
        std::panic::set_hook(Box::new(|panic_info| {
            let message = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
                format!("Rust panic: {}", s)
            } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
                format!("Rust panic: {}", s)
            } else {
                "Rust panic: <unknown>".to_string()
            };
            log_to_ios(&message);
        }));
    });
}

/// Initialize the engine with an app name
/// Returns error message as C string on failure, null on success
#[cfg(target_os = "ios")]
#[no_mangle]
pub extern "C" fn xos_engine_init(app_name: *const c_char, width: u32, height: u32) -> *mut c_char {
    let app_name_str = unsafe {
        if app_name.is_null() {
            return CString::new("app_name is null").unwrap().into_raw();
        }
        match std::ffi::CStr::from_ptr(app_name).to_str() {
            Ok(s) => s,
            Err(_) => {
                return CString::new("invalid app_name encoding").unwrap().into_raw();
            }
        }
    };

    let mut app = match apps::get_app(app_name_str) {
        Some(a) => a,
        None => {
            return CString::new(format!("App '{}' not found", app_name_str))
                .unwrap()
                .into_raw();
        }
    };

    let shape = vec![height as usize, width as usize, 4];
    let data = vec![0u8; (width * height * 4) as usize];
    let mut engine_state = EngineState {
        frame: Array::new_on_device(data, shape, Device::Cpu),
        mouse: MouseState {
            x: 0.0,
            y: 0.0,
            dx: 0.0,
            dy: 0.0,
            is_left_clicking: false,
            is_right_clicking: false,
            style: CursorStyleSetter::new(),
        },
    };

    // Call setup
    if let Err(e) = app.setup(&mut engine_state) {
        return CString::new(format!("Setup failed: {}", e)).unwrap().into_raw();
    }

    let ios_state = IosEngineState {
        app,
        engine_state,
        width,
        height,
    };

    let mut state = ENGINE_STATE.lock().unwrap();
    *state = Some(ios_state);
    
    // Set up logging redirection
    setup_logging();

    ptr::null_mut()
}

/// Free error message returned by xos_engine_init
#[cfg(target_os = "ios")]
#[no_mangle]
pub extern "C" fn xos_engine_init_free(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            let _ = CString::from_raw(ptr);
        }
    }
}

/// Tick the engine (run one frame)
/// Returns 0 on success, non-zero on error
#[cfg(target_os = "ios")]
#[no_mangle]
pub extern "C" fn xos_engine_tick() -> i32 {
    let mut state = match ENGINE_STATE.lock() {
        Ok(s) => s,
        Err(_) => return 1,
    };

    if let Some(ref mut ios_state) = *state {
        // Clear frame buffer
        ios_state.engine_state.frame_buffer_mut().fill(0);
        
        // Run tick with panic handling
        // We use AssertUnwindSafe because we know the FFI boundary is safe
        // and we're catching panics to prevent them from crossing the boundary unsafely
        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            ios_state.app.tick(&mut ios_state.engine_state);
        }));
        
        match result {
            Ok(_) => {
                // Swap R and B channels in-place for iOS Metal compatibility (RGBA -> BGRA)
                let frame_buffer = ios_state.engine_state.frame_buffer_mut();
                let pixel_count = frame_buffer.len() / 4;
                
                for i in 0..pixel_count {
                    let idx = i * 4;
                    if idx + 3 < frame_buffer.len() {
                        // Swap R (idx+0) and B (idx+2) channels
                        frame_buffer.swap(idx, idx + 2);
                    }
                }
                0
            }
            Err(_) => {
                // Panic occurred - the panic hook already logged it
                // Just return error code - Swift side will detect this and show crash overlay
                1
            }
        }
    } else {
        1
    }
}

/// Get frame buffer data
/// Returns pointer to BGRA data (R/B channels swapped in-place for iOS Metal compatibility)
/// The data is valid until the next tick
#[cfg(target_os = "ios")]
#[no_mangle]
pub extern "C" fn xos_engine_get_frame_buffer() -> *const u8 {
    let state = match ENGINE_STATE.lock() {
        Ok(s) => s,
        Err(_) => return ptr::null(),
    };

    if let Some(ref ios_state) = *state {
        let data = ios_state.engine_state.frame.data();
        if data.is_empty() {
            ptr::null()
        } else {
            data.as_ptr()
        }
    } else {
        ptr::null()
    }
}

/// Get frame buffer size (width * height * 4)
#[cfg(target_os = "ios")]
#[no_mangle]
pub extern "C" fn xos_engine_get_frame_buffer_size() -> usize {
    let state = match ENGINE_STATE.lock() {
        Ok(s) => s,
        Err(_) => return 0,
    };

    if let Some(ref ios_state) = *state {
        ios_state.engine_state.frame.data().len()
    } else {
        0
    }
}

/// Get frame dimensions
#[cfg(target_os = "ios")]
#[no_mangle]
pub extern "C" fn xos_engine_get_frame_size(width: *mut u32, height: *mut u32) -> i32 {
    let state = match ENGINE_STATE.lock() {
        Ok(s) => s,
        Err(_) => return 1,
    };

    if let Some(ref ios_state) = *state {
        unsafe {
            if !width.is_null() {
                *width = ios_state.width;
            }
            if !height.is_null() {
                *height = ios_state.height;
            }
        }
        0
    } else {
        1
    }
}

/// Update mouse position
#[cfg(target_os = "ios")]
#[no_mangle]
pub extern "C" fn xos_engine_update_mouse(x: f32, y: f32) -> i32 {
    let mut state = match ENGINE_STATE.lock() {
        Ok(s) => s,
        Err(_) => return 1,
    };

    if let Some(ref mut ios_state) = *state {
        let prev_x = ios_state.engine_state.mouse.x;
        let prev_y = ios_state.engine_state.mouse.y;
        
        ios_state.engine_state.mouse.x = x;
        ios_state.engine_state.mouse.y = y;
        ios_state.engine_state.mouse.dx = x - prev_x;
        ios_state.engine_state.mouse.dy = y - prev_y;
        
        ios_state.app.on_mouse_move(&mut ios_state.engine_state);
        0
    } else {
        1
    }
}

/// Handle mouse down event
#[cfg(target_os = "ios")]
#[no_mangle]
pub extern "C" fn xos_engine_mouse_down() -> i32 {
    let mut state = match ENGINE_STATE.lock() {
        Ok(s) => s,
        Err(_) => return 1,
    };

    if let Some(ref mut ios_state) = *state {
        ios_state.engine_state.mouse.is_left_clicking = true;
        ios_state.app.on_mouse_down(&mut ios_state.engine_state);
        0
    } else {
        1
    }
}

/// Handle mouse up event
#[cfg(target_os = "ios")]
#[no_mangle]
pub extern "C" fn xos_engine_mouse_up() -> i32 {
    let mut state = match ENGINE_STATE.lock() {
        Ok(s) => s,
        Err(_) => return 1,
    };

    if let Some(ref mut ios_state) = *state {
        ios_state.engine_state.mouse.is_left_clicking = false;
        ios_state.app.on_mouse_up(&mut ios_state.engine_state);
        0
    } else {
        1
    }
}

/// Resize the frame buffer
#[cfg(target_os = "ios")]
#[no_mangle]
pub extern "C" fn xos_engine_resize(width: u32, height: u32) -> i32 {
    let mut state = match ENGINE_STATE.lock() {
        Ok(s) => s,
        Err(_) => return 1,
    };

    if let Some(ref mut ios_state) = *state {
        ios_state.width = width;
        ios_state.height = height;
        ios_state.engine_state.resize_frame(width, height);
        0
    } else {
        1
    }
}

/// Cleanup engine state
#[cfg(target_os = "ios")]
#[no_mangle]
pub extern "C" fn xos_engine_cleanup() {
    let mut state = match ENGINE_STATE.lock() {
        Ok(s) => s,
        Err(_) => return,
    };

    *state = None;
}

