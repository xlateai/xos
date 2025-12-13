use std::sync::{Arc, Mutex};
use std::collections::VecDeque;
use std::time::Instant;

use crate::audio::ios_device::AudioDevice;

/// Buffer to store audio samples, separated by channel (iOS version)
/// Same API as native_listener::AudioBuffer
#[derive(Clone)]
pub struct AudioBuffer {
    /// Raw audio samples stored per channel: Vec[channel_idx] -> samples for that channel
    channel_samples: Arc<Mutex<Vec<VecDeque<f32>>>>,
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
        // Create a vector of empty VecDeques, one for each channel
        let mut channel_buffers = Vec::with_capacity(channels as usize);
        for _ in 0..channels {
            channel_buffers.push(VecDeque::with_capacity(capacity));
        }
        
        Self {
            channel_samples: Arc::new(Mutex::new(channel_buffers)),
            capacity,
            sample_rate,
            channels,
            last_access: Arc::new(Mutex::new(Instant::now())),
        }
    }

    /// Add samples to the buffer (one sample per channel)
    /// Called from iOS FFI callback
    fn push_sample_batch_ffi(&self, samples: *const f32, count: usize) {
        if samples.is_null() || count == 0 {
            return;
        }
        
        let mut channel_buffers = self.channel_samples.lock().unwrap();
        let channels = channel_buffers.len();
        
        if count % channels != 0 {
            // Incomplete batch
            return;
        }
        
        let sample_slice = unsafe { std::slice::from_raw_parts(samples, count) };
        
        // Process samples in chunks of channels
        for chunk in sample_slice.chunks(channels) {
            if chunk.len() == channels {
                for (channel_idx, &sample) in chunk.iter().enumerate() {
                    let buffer = &mut channel_buffers[channel_idx];
                    
                    // If buffer is at capacity, remove oldest sample
                    if buffer.len() >= self.capacity {
                        buffer.pop_front();
                    }
                    
                    // Add new sample
                    buffer.push_back(sample);
                }
            }
        }
        
        // Update last access time
        *self.last_access.lock().unwrap() = Instant::now();
    }
    
    /// Get a copy of all samples for each channel
    pub fn get_samples_by_channel(&self) -> Vec<Vec<f32>> {
        let channel_buffers = self.channel_samples.lock().unwrap();
        
        // Convert each channel's VecDeque to a Vec
        channel_buffers.iter()
            .map(|buffer| buffer.iter().cloned().collect())
            .collect()
    }
    
    /// Get average value for each channel
    pub fn get_average_by_channel(&self) -> Vec<f32> {
        let channel_buffers = self.channel_samples.lock().unwrap();
        
        channel_buffers.iter()
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
        let channel_buffers = self.channel_samples.lock().unwrap();
        
        channel_buffers.iter()
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
        let channel_buffers = self.channel_samples.lock().unwrap();
        
        channel_buffers.iter()
            .map(|buffer| {
                buffer.iter().map(|s| s.abs()).fold(0.0, f32::max)
            })
            .collect()
    }
    
    /// Clear all samples from all channels
    pub fn clear(&self) {
        let mut channel_buffers = self.channel_samples.lock().unwrap();
        for buffer in channel_buffers.iter_mut() {
            buffer.clear();
        }
        *self.last_access.lock().unwrap() = Instant::now();
    }
    
    /// Get the number of samples in the first channel (assume all channels have same number)
    pub fn len(&self) -> usize {
        let channel_buffers = self.channel_samples.lock().unwrap();
        if channel_buffers.is_empty() {
            0
        } else {
            channel_buffers[0].len()
        }
    }
    
    /// Check if all channels are empty
    pub fn is_empty(&self) -> bool {
        let channel_buffers = self.channel_samples.lock().unwrap();
        channel_buffers.is_empty() || channel_buffers[0].is_empty()
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
#[derive(Clone)]
pub struct AudioListener {
    /// The audio buffer
    buffer: AudioBuffer,
    /// The listener ID for iOS FFI
    listener_id: u32,
    /// The device being listened to
    device_name: String,
    /// Pointer to the buffer stored in heap for callback (must be freed on drop)
    buffer_ptr: *mut std::ffi::c_void,
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
            
            // Create buffer
            let buffer = AudioBuffer::new(capacity, sample_rate as u32, channels as u16);
            
            // Create the listener first
            let mut listener = Self {
                buffer,
                listener_id,
                device_name: audio_device.name.clone(),
                buffer_ptr: std::ptr::null_mut(),
            };
            
            // Register the buffer callback with iOS (pass pointer to buffer in listener)
            let buffer_ptr = &listener.buffer as *const AudioBuffer as *mut std::ffi::c_void;
            listener.buffer_ptr = buffer_ptr;
            unsafe {
                xos_audio_listener_set_callback(
                    listener_id,
                    Some(audio_callback),
                    buffer_ptr,
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
}

impl Drop for AudioListener {
    fn drop(&mut self) {
        #[cfg(target_os = "ios")]
        {
            unsafe {
                // Clear callback before destroying
                xos_audio_listener_set_callback(self.listener_id, None, std::ptr::null_mut());
                xos_audio_listener_destroy(self.listener_id);
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


