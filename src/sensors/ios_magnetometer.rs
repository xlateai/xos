use ringbuf::{HeapRb, Producer, Consumer};
use std::sync::{Mutex, Arc};
use crate::sensors::MagnetometerReading;

/// Thread-safe wrapper for Producer (needed for FFI callbacks)
struct ProducerWrapper {
    producer: Mutex<Producer<MagnetometerReading, Arc<HeapRb<MagnetometerReading>>>>,
}

/// iOS magnetometer sensor with ring buffer for streaming data
pub struct Magnetometer {
    /// The magnetometer ID for iOS FFI
    magnetometer_id: u32,
    /// Consumer for reading batches from the ring buffer
    consumer: Consumer<MagnetometerReading, Arc<HeapRb<MagnetometerReading>>>,
    /// Pointer to the producer wrapper stored in heap for callback (must be freed on drop)
    producer_wrapper_ptr: *mut std::ffi::c_void,
    /// Total number of readings received (for display)
    total_readings: u64,
}

impl Magnetometer {
    /// Create a new magnetometer listener
    pub fn new() -> Result<Self, String> {
        #[cfg(target_os = "ios")]
        {
            // Initialize magnetometer on iOS side with error handling
            let magnetometer_id = unsafe {
                xos_sensors_magnetometer_init()
            };

            if magnetometer_id == u32::MAX {
                return Err("Failed to initialize magnetometer (iOS returned error)".to_string());
            }

            // Create ring buffer with capacity for ~10 seconds at 10Hz (100 samples)
            // This gives plenty of headroom for batched reads
            let rb = HeapRb::<MagnetometerReading>::new(1024);
            let (producer, consumer) = rb.split();

            // Wrap producer in Mutex for thread-safe FFI access
            let producer_wrapper = Box::new(ProducerWrapper {
                producer: Mutex::new(producer),
            });
            let producer_wrapper_ptr = Box::into_raw(producer_wrapper) as *mut std::ffi::c_void;

            // Create the magnetometer
            let magnetometer = Self {
                magnetometer_id,
                consumer,
                producer_wrapper_ptr,
                total_readings: 0,
            };
            
            unsafe {
                xos_sensors_magnetometer_set_callback(
                    magnetometer_id,
                    Some(magnetometer_callback),
                    producer_wrapper_ptr,
                );
            }

            // Start magnetometer with error handling
            let result = unsafe { xos_sensors_magnetometer_start(magnetometer_id) };
            if result != 0 {
                // Clean up on failure
                unsafe {
                    xos_sensors_magnetometer_set_callback(magnetometer_id, None, std::ptr::null_mut());
                    xos_sensors_magnetometer_destroy(magnetometer_id);
                    // Recover the producer wrapper box to avoid leak
                    let _ = Box::from_raw(producer_wrapper_ptr as *mut ProducerWrapper);
                }
                return Err(format!("Failed to start magnetometer (error code: {})", result));
            }

            Ok(magnetometer)
        }

        #[cfg(not(target_os = "ios"))]
        {
            Err("iOS magnetometer only available on iOS".to_string())
        }
    }

    /// Drain all readings since last call (batch read)
    /// Returns a vector of all readings accumulated since the last drain
    pub fn drain_readings(&mut self) -> Vec<MagnetometerReading> {
        let mut batch = Vec::new();
        while let Some(reading) = self.consumer.pop() {
            batch.push(reading);
            self.total_readings += 1;
        }
        batch
    }

    /// Get the latest reading (most recent, if any)
    /// This peeks at the most recent reading without draining
    pub fn get_latest_reading(&mut self) -> Option<MagnetometerReading> {
        // Drain all but keep the last one
        let mut last = None;
        while let Some(reading) = self.consumer.pop() {
            last = Some(reading);
            self.total_readings += 1;
        }
        last
    }

    /// Get the total number of readings received
    pub fn get_total_readings(&self) -> u64 {
        self.total_readings
    }
}

impl Drop for Magnetometer {
    fn drop(&mut self) {
        #[cfg(target_os = "ios")]
        {
            unsafe {
                // IMPORTANT: Clear callback FIRST to prevent any new callbacks
                // This must happen before destroy to avoid race conditions
                // The Swift side will also clear the callback in stop(), providing double protection
                xos_sensors_magnetometer_set_callback(self.magnetometer_id, None, std::ptr::null_mut());
                
                // Now destroy the listener (this will stop updates on Swift side)
                xos_sensors_magnetometer_destroy(self.magnetometer_id);
                
                // Recover the producer wrapper box to avoid leak
                // Do this last, after ensuring no more callbacks can fire
                if !self.producer_wrapper_ptr.is_null() {
                    let _ = Box::from_raw(self.producer_wrapper_ptr as *mut ProducerWrapper);
                    self.producer_wrapper_ptr = std::ptr::null_mut();
                }
            }
        }
    }
}

// FFI callback function called from Swift
extern "C" fn magnetometer_callback(x: f64, y: f64, z: f64, user_data: *mut std::ffi::c_void) {
    // Early return if user_data is null (callback was cleared)
    if user_data.is_null() {
        return;
    }

    // Wrap in catch_unwind to prevent Swift crashes from propagating
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // Double-check pointer is still valid before dereferencing
        // This is a best-effort check - if the memory was freed, this might still crash
        // but it's better than nothing
        let wrapper = unsafe { &*(user_data as *const ProducerWrapper) };
        let reading = MagnetometerReading { x, y, z };
        
        // Lock producer and push to ring buffer (non-blocking, may fail if full)
        // If buffer is full, we drop the sample (better than blocking)
        // If lock fails (e.g., mutex was poisoned), we drop the sample
        if let Ok(mut producer) = wrapper.producer.try_lock() {
            let _ = producer.push(reading);
        }
        // If lock fails, we drop the sample (callback might be called from different thread)
    }));
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
