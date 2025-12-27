use std::sync::{Arc, Mutex};
use std::collections::VecDeque;
use std::time::Instant;

use crate::audio::ios_device::AudioDevice;

/// Double-buffered audio storage for lock-free reads on iOS
struct DoubleBuffer {
    /// Write buffer (audio callback writes here)
    write_buffer: Vec<VecDeque<f32>>,
    /// Read buffer (get_samples reads from here)
    read_buffer: Vec<VecDeque<f32>>,
}

/// Buffer to store audio samples, separated by channel (iOS version)
/// Same API as native_listener::AudioBuffer but optimized for iOS
#[derive(Clone)]
pub struct AudioBuffer {
    /// Double-buffered storage to minimize lock contention
    buffers: Arc<Mutex<DoubleBuffer>>,
    /// Maximum buffer capacity per channel
    capacity: usize,
    /// Sample rate of the audio
    sample_rate: u32,
    /// Number of channels
    channels: u16,
    /// Timestamp when the buffer was last accessed
    last_access: Arc<Mutex<Instant>>,
}

impl AudioBuffer {
    fn new(capacity: usize, sample_rate: u32, channels: u16) -> Self {
        // Create double buffers - both write and read start empty
        let mut write_buffer = Vec::with_capacity(channels as usize);
        let mut read_buffer = Vec::with_capacity(channels as usize);
        for _ in 0..channels {
            write_buffer.push(VecDeque::with_capacity(capacity));
            read_buffer.push(VecDeque::with_capacity(capacity));
        }
        
        Self {
            buffers: Arc::new(Mutex::new(DoubleBuffer {
                write_buffer,
                read_buffer,
            })),
            capacity,
            sample_rate,
            channels,
            last_access: Arc::new(Mutex::new(Instant::now())),
        }
    }

    /// Add samples to the buffer (one sample per channel)
    /// Called from iOS FFI callback - optimized to minimize lock time
    fn push_sample_batch_ffi(&self, samples: *const f32, count: usize) {
        if samples.is_null() || count == 0 {
            return;
        }
        
        // Quick lock to add samples to write buffer
        let mut buffers = match self.buffers.try_lock() {
            Ok(b) => b,
            Err(_) => {
                // If we can't get the lock immediately, skip this batch
                // This prevents audio callback from blocking
                return;
            }
        };
        
        let channels = buffers.write_buffer.len();
        
        if count % channels != 0 {
            // Incomplete batch
            return;
        }
        
        let sample_slice = unsafe { std::slice::from_raw_parts(samples, count) };
        
        // Process samples in chunks of channels - write to write_buffer
        for chunk in sample_slice.chunks(channels) {
            if chunk.len() == channels {
                for (channel_idx, &sample) in chunk.iter().enumerate() {
                    let buffer = &mut buffers.write_buffer[channel_idx];
                    
                    // If buffer is at capacity, remove oldest sample
                    if buffer.len() >= self.capacity {
                        buffer.pop_front();
                    }
                    
                    // Add new sample
                    buffer.push_back(sample);
                }
            }
        }
        
        // Lock is released here automatically (minimal hold time)
        drop(buffers);
        
        // Update last access time
        *self.last_access.lock().unwrap() = Instant::now();
    }
    
    /// Get a copy of all samples for each channel
    /// Optimized: swaps buffers quickly to avoid blocking audio callback
    pub fn get_samples_by_channel(&self) -> Vec<Vec<f32>> {
        // Quick check: if write buffer is empty, return empty without swapping
        {
            let buffers = match self.buffers.try_lock() {
                Ok(b) => b,
                Err(_) => {
                    // If we can't get lock, return empty (audio callback is busy)
                    return vec![vec![]; self.channels as usize];
                }
            };
            
            // If write buffer is empty, no need to swap
            if buffers.write_buffer.is_empty() || buffers.write_buffer[0].is_empty() {
                return buffers.read_buffer.iter()
                    .map(|buffer| buffer.iter().cloned().collect())
                    .collect();
            }
        } // Release lock here
        
        // Write buffer has data, do the swap
        let mut buffers = self.buffers.lock().unwrap();
        
        // Swap write and read buffers using raw pointer access to avoid borrow checker issue
        let write_ptr = &mut buffers.write_buffer as *mut Vec<VecDeque<f32>>;
        let read_ptr = &mut buffers.read_buffer as *mut Vec<VecDeque<f32>>;
        unsafe {
            std::ptr::swap(write_ptr, read_ptr);
        }
        
        // Clear the new write buffer (which was the old read buffer)
        for buffer in buffers.write_buffer.iter_mut() {
            buffer.clear();
        }
        
        // Clone the read buffer (copy happens with lock held but should be quick)
        let result: Vec<Vec<f32>> = buffers.read_buffer.iter()
            .map(|buffer| buffer.iter().cloned().collect())
            .collect();
        
        drop(buffers); // Explicitly release lock
        
        result
    }
    
    /// Get average value for each channel
    pub fn get_average_by_channel(&self) -> Vec<f32> {
        let buffers = self.buffers.lock().unwrap();
        
        buffers.read_buffer.iter()
            .map(|buffer| {
                if buffer.is_empty() {
                    0.0
                } else {
                    let sum: f32 = buffer.iter().sum();
                    sum / buffer.len() as f32
                }
            })
            .collect()
    }
    
    /// Get the RMS (root mean square) value for each channel
    pub fn get_rms_by_channel(&self) -> Vec<f32> {
        let buffers = self.buffers.lock().unwrap();
        
        buffers.read_buffer.iter()
            .map(|buffer| {
                if buffer.is_empty() {
                    0.0
                } else {
                    let sum_squares: f32 = buffer.iter().map(|s| s * s).sum();
                    (sum_squares / buffer.len() as f32).sqrt()
                }
            })
            .collect()
    }
    
    /// Get peak value (maximum absolute value) for each channel
    pub fn get_peak_by_channel(&self) -> Vec<f32> {
        let buffers = self.buffers.lock().unwrap();
        
        buffers.read_buffer.iter()
            .map(|buffer| {
                buffer.iter().map(|s| s.abs()).fold(0.0, f32::max)
            })
            .collect()
    }
    
    /// Clear all samples from all channels
    pub fn clear(&self) {
        let mut buffers = self.buffers.lock().unwrap();
        for buffer in buffers.write_buffer.iter_mut() {
            buffer.clear();
        }
        for buffer in buffers.read_buffer.iter_mut() {
            buffer.clear();
        }
        *self.last_access.lock().unwrap() = Instant::now();
    }
    
    /// Get the number of samples in the first channel (assume all channels have same number)
    pub fn len(&self) -> usize {
        let buffers = self.buffers.lock().unwrap();
        if buffers.read_buffer.is_empty() {
            0
        } else {
            buffers.read_buffer[0].len()
        }
    }
    
    /// Check if all channels are empty
    pub fn is_empty(&self) -> bool {
        let buffers = self.buffers.lock().unwrap();
        buffers.read_buffer.is_empty() || buffers.read_buffer[0].is_empty()
    }
    
    /// Get the buffer duration in seconds
    pub fn duration(&self) -> f32 {
        let len = self.len();
        len as f32 / self.sample_rate as f32
    }
    
    /// Get the sample rate
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    
    /// Get the number of channels
    pub fn channels(&self) -> u16 {
        self.channels
    }
}

/// Audio listener to capture audio from a device (iOS version)
/// Same API as native_listener::AudioListener
pub struct AudioListener {
    /// The audio buffer (shared via Arc for safe access)
    buffer: AudioBuffer,
    /// The listener ID for iOS FFI
    listener_id: u32,
    /// The device being listened to
    device_name: String,
    /// Raw pointer to boxed buffer for FFI (must be manually freed on drop)
    buffer_ptr: *mut AudioBuffer,
    /// Flag to indicate if the iOS listener has been destroyed (to prevent double-destroy)
    #[cfg(target_os = "ios")]
    destroyed: std::sync::atomic::AtomicBool,
}

impl AudioListener {
    /// Create a new listener for the specified device
    pub fn new(audio_device: &AudioDevice, buffer_duration_secs: f32) -> Result<Self, String> {
        #[cfg(target_os = "ios")]
        {
            if !audio_device.is_input {
                return Err("Device is not an input device".to_string());
            }
            
            // Initialize audio listener on iOS side
            let sample_rate: f64 = 44100.0; // Default iOS sample rate
            let channels: u32 = 1; // Mono for now
            
            let listener_id = unsafe {
                xos_audio_listener_init(
                    audio_device.device_id,
                    sample_rate,
                    channels,
                    buffer_duration_secs as f64,
                )
            };
            
            if listener_id == u32::MAX {
                return Err("Failed to initialize audio listener".to_string());
            }
            
            // Calculate buffer capacity
            let capacity = (buffer_duration_secs * sample_rate as f32) as usize;
            
            // Create buffer on heap for stable pointer across moves
            let buffer = AudioBuffer::new(capacity, sample_rate as u32, channels as u16);
            let buffer_box = Box::new(buffer.clone());
            let buffer_ptr = Box::into_raw(buffer_box);
            
            // Create the listener (store the raw pointer, we'll manage it manually)
            let listener = Self {
                buffer,
                listener_id,
                device_name: audio_device.name.clone(),
                buffer_ptr,
                #[cfg(target_os = "ios")]
                destroyed: std::sync::atomic::AtomicBool::new(false),
            };
            
            // Register the buffer callback with iOS (pass stable heap pointer)
            unsafe {
                xos_audio_listener_set_callback(
                    listener_id,
                    Some(audio_callback),
                    buffer_ptr as *mut std::ffi::c_void,
                );
            }
            
            // Start recording
            let result = unsafe { xos_audio_listener_start(listener_id) };
            if result != 0 {
                unsafe {
                    xos_audio_listener_destroy(listener_id);
                }
                return Err("Failed to start audio listener".to_string());
            }
            
            Ok(listener)
        }
        
        #[cfg(not(target_os = "ios"))]
        {
            Err("iOS audio listener only available on iOS".to_string())
        }
    }
    
    /// Get a reference to the audio buffer
    pub fn buffer(&self) -> &AudioBuffer {
        &self.buffer
    }
    
    /// Get the device name
    pub fn device_name(&self) -> &str {
        &self.device_name
    }
    
    /// Pause the audio stream
    pub fn pause(&self) -> Result<(), String> {
        #[cfg(target_os = "ios")]
        {
            let result = unsafe { xos_audio_listener_pause(self.listener_id) };
            if result == 0 {
                Ok(())
            } else {
                Err("Failed to pause audio stream".to_string())
            }
        }
        #[cfg(not(target_os = "ios"))]
        {
            Err("iOS audio listener only available on iOS".to_string())
        }
    }
    
    /// Resume the audio stream
    pub fn record(&self) -> Result<(), String> {
        #[cfg(target_os = "ios")]
        {
            let result = unsafe { xos_audio_listener_start(self.listener_id) };
            if result == 0 {
                Ok(())
            } else {
                Err("Failed to resume audio stream".to_string())
            }
        }
        #[cfg(not(target_os = "ios"))]
        {
            Err("iOS audio listener only available on iOS".to_string())
        }
    }
    
    /// Get samples separated by channel
    pub fn get_samples_by_channel(&self) -> Vec<Vec<f32>> {
        self.buffer.get_samples_by_channel()
    }
    
    /// Get the listener ID (for direct cleanup)
    #[cfg(target_os = "ios")]
    pub fn listener_id(&self) -> u32 {
        self.listener_id
    }
    
    /// Mark as destroyed and destroy immediately (for fast cleanup)
    #[cfg(target_os = "ios")]
    pub fn destroy_now(&self) {
        if !self.destroyed.swap(true, std::sync::atomic::Ordering::SeqCst) {
            unsafe {
                xos_audio_listener_set_callback(self.listener_id, None, std::ptr::null_mut());
                xos_audio_listener_destroy(self.listener_id);
            }
        }
    }
}

impl Drop for AudioListener {
    fn drop(&mut self) {
        #[cfg(target_os = "ios")]
        {
            // Only destroy if not already destroyed
            if !self.destroyed.swap(true, std::sync::atomic::Ordering::SeqCst) {
                unsafe {
                    // Clear callback before destroying
                    xos_audio_listener_set_callback(self.listener_id, None, std::ptr::null_mut());
                    xos_audio_listener_destroy(self.listener_id);
                }
            }
            
            // Always free the boxed buffer
            unsafe {
                if !self.buffer_ptr.is_null() {
                    let _ = Box::from_raw(self.buffer_ptr);
                    // Box will be dropped here, freeing the heap allocation
                }
            }
        }
    }
}

// FFI callback function called from Swift
extern "C" fn audio_callback(samples: *const f32, count: usize, user_data: *mut std::ffi::c_void) {
    if user_data.is_null() || samples.is_null() || count == 0 {
        return;
    }
    
    let buffer = unsafe { &*(user_data as *const AudioBuffer) };
    buffer.push_sample_batch_ffi(samples, count);
}

// FFI declarations for iOS audio listener functions
#[cfg(target_os = "ios")]
extern "C" {
    fn xos_audio_listener_init(
        device_id: u32,
        sample_rate: f64,
        channels: u32,
        buffer_duration: f64,
    ) -> u32;
    
    fn xos_audio_listener_set_callback(
        listener_id: u32,
        callback: Option<extern "C" fn(*const f32, usize, *mut std::ffi::c_void)>,
        user_data: *mut std::ffi::c_void,
    );
    
    fn xos_audio_listener_start(listener_id: u32) -> std::os::raw::c_int;
    
    fn xos_audio_listener_pause(listener_id: u32) -> std::os::raw::c_int;
    
    fn xos_audio_listener_destroy(listener_id: u32);
}


