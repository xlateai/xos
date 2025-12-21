use std::sync::{Arc, Mutex};

/// Magnetometer reading buffer
#[derive(Clone)]
struct MagnetometerBuffer {
    /// Current reading (x, y, z) in microtesla (μT)
    current_reading: Arc<Mutex<Option<(f64, f64, f64)>>>,
    /// Total number of readings received
    total_readings: Arc<Mutex<u64>>,
}

impl MagnetometerBuffer {
    fn new() -> Self {
        Self {
            current_reading: Arc::new(Mutex::new(None)),
            total_readings: Arc::new(Mutex::new(0)),
        }
    }

    fn push_reading(&self, x: f64, y: f64, z: f64) {
        *self.current_reading.lock().unwrap() = Some((x, y, z));
        *self.total_readings.lock().unwrap() += 1;
    }
}

/// iOS magnetometer sensor
pub struct Magnetometer {
    /// The magnetometer ID for iOS FFI
    magnetometer_id: u32,
    /// The buffer storing readings
    buffer: MagnetometerBuffer,
    /// Pointer to the buffer stored in heap for callback (must be freed on drop)
    buffer_ptr: *mut std::ffi::c_void,
}

impl Magnetometer {
    /// Create a new magnetometer listener
    pub fn new() -> Result<Self, String> {
        #[cfg(target_os = "ios")]
        {
            // Initialize magnetometer on iOS side
            let magnetometer_id = unsafe {
                xos_sensors_magnetometer_init()
            };

            if magnetometer_id == u32::MAX {
                return Err("Failed to initialize magnetometer".to_string());
            }

            // Create buffer
            let buffer = MagnetometerBuffer::new();

            // Create the magnetometer first
            let mut magnetometer = Self {
                magnetometer_id,
                buffer,
                buffer_ptr: std::ptr::null_mut(),
            };

            // Register the buffer callback with iOS (pass pointer to buffer)
            let buffer_ptr = &magnetometer.buffer as *const MagnetometerBuffer as *mut std::ffi::c_void;
            magnetometer.buffer_ptr = buffer_ptr;
            unsafe {
                xos_sensors_magnetometer_set_callback(
                    magnetometer_id,
                    Some(magnetometer_callback),
                    buffer_ptr,
                );
            }

            // Start magnetometer
            let result = unsafe { xos_sensors_magnetometer_start(magnetometer_id) };
            if result != 0 {
                unsafe {
                    xos_sensors_magnetometer_destroy(magnetometer_id);
                }
                return Err("Failed to start magnetometer".to_string());
            }

            Ok(magnetometer)
        }

        #[cfg(not(target_os = "ios"))]
        {
            Err("iOS magnetometer only available on iOS".to_string())
        }
    }

    /// Get the current magnetometer reading in microtesla (μT)
    pub fn get_reading(&self) -> Option<(f64, f64, f64)> {
        *self.buffer.current_reading.lock().unwrap()
    }

    /// Get the total number of readings received
    pub fn get_total_readings(&self) -> u64 {
        *self.buffer.total_readings.lock().unwrap()
    }
}

impl Drop for Magnetometer {
    fn drop(&mut self) {
        #[cfg(target_os = "ios")]
        {
            unsafe {
                // Clear callback before destroying
                xos_sensors_magnetometer_set_callback(self.magnetometer_id, None, std::ptr::null_mut());
                xos_sensors_magnetometer_destroy(self.magnetometer_id);
            }
        }
    }
}

// FFI callback function called from Swift
extern "C" fn magnetometer_callback(x: f64, y: f64, z: f64, user_data: *mut std::ffi::c_void) {
    if user_data.is_null() {
        return;
    }

    let buffer = unsafe { &*(user_data as *const MagnetometerBuffer) };
    buffer.push_reading(x, y, z);
}

// FFI declarations for iOS magnetometer functions
#[cfg(target_os = "ios")]
extern "C" {
    fn xos_sensors_magnetometer_init() -> u32;

    fn xos_sensors_magnetometer_set_callback(
        magnetometer_id: u32,
        callback: Option<extern "C" fn(f64, f64, f64, *mut std::ffi::c_void)>,
        user_data: *mut std::ffi::c_void,
    );

    fn xos_sensors_magnetometer_start(magnetometer_id: u32) -> std::os::raw::c_int;

    fn xos_sensors_magnetometer_destroy(magnetometer_id: u32);
}

