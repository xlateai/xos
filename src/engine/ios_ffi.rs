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
use crate::engine::{
    tick_fps_overlay, tick_frame_delta, Application, EngineState, FpsOverlay, FrameState,
    KeyboardState, MouseState, SafeRegionBoundingRectangle,
};
#[cfg(target_os = "ios")]
use crate::engine::engine::CursorStyleSetter;

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
    last_tick_instant: Option<std::time::Instant>,
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

    let safe_region = SafeRegionBoundingRectangle::ios_iphone_16_pro();
    let mut engine_state = EngineState {
        frame: FrameState::new(width, height, safe_region),
        mouse: MouseState {
            x: 0.0,
            y: 0.0,
            dx: 0.0,
            dy: 0.0,
            is_left_clicking: false,
            is_right_clicking: false,
            style: CursorStyleSetter::new(),
        },
        keyboard: KeyboardState {
            onscreen: crate::text::onscreen_keyboard::OnScreenKeyboard::new(),
        },
        fps_overlay: FpsOverlay::new(),
        delta_time_seconds: 1.0 / 60.0,
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
        last_tick_instant: None,
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
        // Run app tick first with panic handling
        // We use AssertUnwindSafe because we know the FFI boundary is safe
        // and we're catching panics to prevent them from crossing the boundary unsafely
        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            tick_frame_delta(
                &mut ios_state.engine_state,
                &mut ios_state.last_tick_instant,
            );
            ios_state.app.tick(&mut ios_state.engine_state);
        }));
        
        // Check for panic first
        if let Err(_) = result {
            return 2; // Panic occurred
        }
        
        // Then draw the keyboard on top (handles positioning, rendering, and key repeats)
        {
            let width = ios_state.width;
            let height = ios_state.height;
            let mouse_x = ios_state.engine_state.mouse.x;
            let mouse_y = ios_state.engine_state.mouse.y;
            let safe_region = ios_state.engine_state.frame.safe_region_boundaries.clone();
            // Split borrows: get buffer and keyboard separately
            let (buffer, keyboard) = {
                let buffer_ptr = ios_state.engine_state.frame.buffer_mut() as *mut [u8];
                let keyboard_ptr: *mut crate::text::onscreen_keyboard::OnScreenKeyboard = &mut ios_state.engine_state.keyboard.onscreen;
                (unsafe { &mut *buffer_ptr }, unsafe { &mut *keyboard_ptr })
            };
            keyboard.tick(buffer, width, height, mouse_x, mouse_y, &safe_region);
        }

        tick_fps_overlay(&mut ios_state.engine_state);
        
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
        let data = ios_state.engine_state.frame.tensor.data();
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
        ios_state.engine_state.frame.tensor.data().len()
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
        // Notify app of screen size change
        ios_state.app.on_screen_size_change(&mut ios_state.engine_state, width, height);
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

// ===== Magnetometer FFI =====

#[cfg(target_os = "ios")]
struct MagnetometerWrapper(crate::sensors::Magnetometer);

// Safe because iOS FFI is single-threaded on main thread
#[cfg(target_os = "ios")]
unsafe impl Send for MagnetometerWrapper {}

#[cfg(target_os = "ios")]
static MAGNETOMETER: Mutex<Option<MagnetometerWrapper>> = Mutex::new(None);

/// Initialize magnetometer sensor
/// Returns 0 on success, non-zero on error
#[cfg(target_os = "ios")]
#[no_mangle]
pub extern "C" fn xos_magnetometer_init() -> i32 {
    let mut mag = match MAGNETOMETER.lock() {
        Ok(m) => m,
        Err(_) => return 1,
    };
    
    match crate::sensors::Magnetometer::new() {
        Ok(m) => {
            *mag = Some(MagnetometerWrapper(m));
            0
        }
        Err(e) => {
            log_to_ios(&format!("Failed to initialize magnetometer: {}", e));
            2
        }
    }
}

/// Get latest magnetometer reading
/// Returns 0 on success with values written to out parameters, 1 if no data available, 2 on error
#[cfg(target_os = "ios")]
#[no_mangle]
pub extern "C" fn xos_magnetometer_get_latest(x: *mut f64, y: *mut f64, z: *mut f64) -> i32 {
    let mut mag = match MAGNETOMETER.lock() {
        Ok(m) => m,
        Err(_) => return 2,
    };
    
    if let Some(ref mut wrapper) = *mag {
        if let Some(reading) = wrapper.0.get_latest_reading() {
            unsafe {
                if !x.is_null() { *x = reading.x; }
                if !y.is_null() { *y = reading.y; }
                if !z.is_null() { *z = reading.z; }
            }
            0
        } else {
            1 // No data available
        }
    } else {
        2 // Not initialized
    }
}

/// Drain all magnetometer readings since last call
/// Returns number of readings, or -1 on error
/// Readings are written to the arrays (must be pre-allocated with sufficient size)
#[cfg(target_os = "ios")]
#[no_mangle]
pub extern "C" fn xos_magnetometer_drain_readings(
    x_array: *mut f64,
    y_array: *mut f64,
    z_array: *mut f64,
    max_count: usize,
) -> i32 {
    let mut mag = match MAGNETOMETER.lock() {
        Ok(m) => m,
        Err(_) => return -1,
    };
    
    if let Some(ref mut wrapper) = *mag {
        let readings = wrapper.0.drain_readings();
        let count = readings.len().min(max_count);
        
        unsafe {
            for (i, reading) in readings.iter().take(count).enumerate() {
                if !x_array.is_null() { *x_array.add(i) = reading.x; }
                if !y_array.is_null() { *y_array.add(i) = reading.y; }
                if !z_array.is_null() { *z_array.add(i) = reading.z; }
            }
        }
        
        count as i32
    } else {
        -1
    }
}

/// Get total number of readings received
#[cfg(target_os = "ios")]
#[no_mangle]
pub extern "C" fn xos_magnetometer_get_total_readings() -> u64 {
    let mag = match MAGNETOMETER.lock() {
        Ok(m) => m,
        Err(_) => return 0,
    };
    
    if let Some(ref wrapper) = *mag {
        wrapper.0.get_total_readings()
    } else {
        0
    }
}

/// Cleanup magnetometer
#[cfg(target_os = "ios")]
#[no_mangle]
pub extern "C" fn xos_magnetometer_cleanup() {
    let mut mag = match MAGNETOMETER.lock() {
        Ok(m) => m,
        Err(_) => return,
    };
    
    *mag = None;
}

// ===== Clipboard FFI =====
// Clipboard functions are implemented in Swift (XosClipboardModule.swift)
// and imported directly by xos/src/clipboard.rs

// ===== Application List FFI =====

/// Get the number of available applications
#[cfg(target_os = "ios")]
#[no_mangle]
pub extern "C" fn xos_list_applications_count() -> usize {
    apps::list_apps().len()
}

/// Get the name of an application by index
/// Returns a pointer to a C string that must be freed with xos_list_applications_free_name
#[cfg(target_os = "ios")]
#[no_mangle]
pub extern "C" fn xos_list_applications_get_name(index: usize) -> *mut c_char {
    let apps = apps::list_apps();
    
    if index >= apps.len() {
        return ptr::null_mut();
    }
    
    let app_name = apps[index];
    match CString::new(app_name) {
        Ok(c_str) => c_str.into_raw(),
        Err(_) => ptr::null_mut(),
    }
}

/// Free a string returned by xos_list_applications_get_name
#[cfg(target_os = "ios")]
#[no_mangle]
pub extern "C" fn xos_list_applications_free_name(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            let _ = CString::from_raw(ptr);
        }
    }
}

