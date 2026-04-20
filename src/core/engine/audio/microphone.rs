//! Audio **input** capture (module name: `microphone` for history).
//!
//! Handles all platform input endpoints: physical microphones, headsets, and virtual
//! devices that present system/loopback audio as an input (driver-dependent).
//! 
//! This module provides audio input functionality across:
//! - macOS/Linux (native) using CPAL
//! - iOS using AVAudioEngine via Swift FFI
//! - WASM using Web Audio API
//!
//! ## Buffer semantics
//!
//! The `AudioBuffer` is a **single rolling ring per `Microphone` / `AudioListener`** (not one
//! buffer per consumer). `buffer_duration_secs` (Python: `buffer_duration`, alias
//! `max_buffer_duration`) is the **maximum** time depth retained per channel (~`duration *
//! sample_rate` frames); when full, oldest samples drop (FIFO).
//!
//! - `get_samples_by_channel()` **peeks** at the ring (does not remove).
//! - `Microphone.read()` / `drain_samples()` **removes** samples; avoid mixing drains with other
//!   consumers unless you intend to steal audio from the shared ring.
//! - MP3 `xos.audio.recording` reads **incrementally** via an internal frame counter so it does
//!   not clear the ring; safe alongside transcription / waveforms that peek.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::collections::VecDeque;
use std::time::Instant;

// ================================================================================================
// COMMON TYPES
// ================================================================================================

// AudioDevice is defined in mod.rs and re-exported here for convenience
use super::AudioDevice;

// ================================================================================================
// AUDIO BUFFER (Shared across all platforms)
// ================================================================================================

/// Buffer to store audio samples, separated by channel
/// 
/// This is a rolling window buffer that automatically drops old samples when at capacity.
/// It does NOT clear on read - allowing continuous accumulation and smooth data flow.
#[derive(Clone)]
pub struct AudioBuffer {
    /// Raw audio samples stored per channel: Vec[channel_idx] -> samples for that channel
    channel_samples: Arc<Mutex<Vec<VecDeque<f32>>>>,
    /// Maximum buffer capacity per channel
    #[allow(dead_code)]
    capacity: usize,
    /// Sample rate of the audio
    sample_rate: u32,
    /// Number of channels
    channels: u16,
    /// Timestamp when the buffer was last accessed
    last_access: Arc<Mutex<Instant>>,
    /// Monotonic count of multi-channel frames ingested (one interleaved frame = +1).
    frames_ingested: Arc<AtomicU64>,
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
            frames_ingested: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Add samples to the buffer (one sample per channel)
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn push_sample_batch(&self, samples: &[f32]) {
        let mut channel_buffers = self.channel_samples.lock().unwrap();
        
        // Check if we have the right number of samples
        if samples.len() != channel_buffers.len() {
            // Handle error case - incomplete batch of samples
            return;
        }
        
        // Add each sample to its corresponding channel buffer
        for (channel_idx, &sample) in samples.iter().enumerate() {
            let buffer = &mut channel_buffers[channel_idx];
            
            // If buffer is at capacity, remove oldest sample (FIFO)
            if buffer.len() >= self.capacity {
                buffer.pop_front();
            }
            
            // Add new sample
            buffer.push_back(sample);
        }

        self.frames_ingested.fetch_add(1, Ordering::Relaxed);
        
        // Update last access time
        *self.last_access.lock().unwrap() = Instant::now();
    }

    /// Interleaved `channels`-wide PCM (e.g. ScreenCaptureKit).
    #[cfg(target_os = "macos")]
    pub(crate) fn push_interleaved_f32(&self, samples: &[f32], channels: u16) {
        let c = channels as usize;
        if c == 0 {
            return;
        }
        for chunk in samples.chunks(c) {
            if chunk.len() == c {
                self.push_sample_batch(chunk);
            }
        }
    }

    /// Add samples to the buffer from FFI (iOS)
    #[cfg(target_os = "ios")]
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
                self.frames_ingested.fetch_add(1, Ordering::Relaxed);
            }
        }
        
        // Update last access time
        *self.last_access.lock().unwrap() = Instant::now();
    }

    /// Add samples from WASM (mono channel)
    #[cfg(target_arch = "wasm32")]
    fn push(&self, samples: &[f32]) {
        let mut buffers = self.channel_samples.lock().unwrap();
        let buffer = &mut buffers[0];
        for &sample in samples {
            if buffer.len() >= self.capacity {
                buffer.pop_front();
            }
            buffer.push_back(sample);
        }

        self.frames_ingested
            .fetch_add(samples.len() as u64, Ordering::Relaxed);
    }
    
    /// Get a copy of all samples for each channel
    /// 
    /// **IMPORTANT**: This does NOT clear the buffer! The buffer continues to accumulate.
    /// This is intentional for smooth visualization and continuous data flow.
    pub fn get_samples_by_channel(&self) -> Vec<Vec<f32>> {
        let channel_buffers = self.channel_samples.lock().unwrap();
        
        // Convert each channel's VecDeque to a Vec
        channel_buffers.iter()
            .map(|buffer| buffer.iter().cloned().collect())
            .collect()
    }
    
    /// Drain (remove and return) up to `count` samples from each channel
    /// 
    /// This is a FIFO queue operation - removes oldest samples first.
    /// Use this for audio relay where you want to consume samples without repeating them.
    /// 
    /// Returns a vector of channels, each containing up to `count` samples.
    /// If fewer samples are available, returns what's available.
    pub fn drain_samples(&self, count: usize) -> Vec<Vec<f32>> {
        let mut channel_buffers = self.channel_samples.lock().unwrap();
        
        // Drain up to `count` samples from each channel
        channel_buffers.iter_mut()
            .map(|buffer| {
                let drain_count = count.min(buffer.len());
                buffer.drain(0..drain_count).collect()
            })
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
    /// 
    /// **WARNING**: Only use this when explicitly needed (e.g., reset).
    /// Normal reads should NOT clear the buffer to maintain smooth data flow.
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

    /// Monotonic count of multi-channel frames ingested into this ring (for incremental readers).
    pub fn ingested_frame_count(&self) -> u64 {
        self.frames_ingested.load(Ordering::Relaxed)
    }

    /// Peek at the last `frame_count` frames per channel (does not remove). `frame_count` is
    /// clamped per channel to the current length. All channels return the same logical length
    /// when the ring is driven symmetrically.
    pub fn copy_tail_frames(&self, frame_count: usize) -> Vec<Vec<f32>> {
        let channel_buffers = self.channel_samples.lock().unwrap();
        channel_buffers
            .iter()
            .map(|buffer| {
                if frame_count == 0 || buffer.is_empty() {
                    return Vec::new();
                }
                let take = frame_count.min(buffer.len());
                let start = buffer.len() - take;
                buffer.range(start..).cloned().collect()
            })
            .collect()
    }
}

// ================================================================================================
// NATIVE (macOS/Windows) IMPLEMENTATION using CPAL
// ================================================================================================

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios"), any(target_os = "macos", target_os = "windows")))]
mod native {
    use super::*;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::{SampleFormat, Stream};
    use std::sync::Arc;
    #[cfg(target_os = "macos")]
    use screencapturekit::stream::sc_stream::SCStream;

    /// Audio listener to capture audio from a device
    pub struct AudioListener {
        buffer: AudioBuffer,
        /// CPAL capture, or `None` when using macOS ScreenCaptureKit system audio.
        cpal_stream: Option<Arc<Stream>>,
        #[cfg(target_os = "macos")]
        sck_stream: Option<Arc<SCStream>>,
        device_name: String,
    }

    impl AudioListener {
        /// Create a new listener for the specified device.
        ///
        /// `buffer_duration_secs` is the **maximum** ring depth per channel (rolling window).
        pub fn new(audio_device: &AudioDevice, buffer_duration_secs: f32) -> Result<Self, String> {
            #[cfg(target_os = "macos")]
            if audio_device.macos_sck_system_audio {
                let sample_rate = 48_000u32;
                let channels = 2u16;
                let capacity = (buffer_duration_secs * sample_rate as f32) as usize;
                let buffer = AudioBuffer::new(capacity, sample_rate, channels);
                let sck = crate::engine::audio::macos_sck::build_system_audio_stream(buffer.clone())
                    .map_err(|e| {
                        format!(
                            "{e} — enable Screen Recording for this app in System Settings → Privacy & Security."
                        )
                    })?;
                return Ok(Self {
                    buffer,
                    cpal_stream: None,
                    sck_stream: Some(Arc::new(sck)),
                    device_name: audio_device.name.clone(),
                });
            }

            let device = &audio_device.device_cpal;

            // Get device name
            let device_name = match device.name() {
                Ok(name) => name,
                Err(_) => return Err("Could not get device name".to_string()),
            };
            
            // Get default config: normal inputs use capture defaults; Windows WASAPI loopback uses
            // the **output** mix format on a render device (cpal sets AUDCLNT_STREAMFLAGS_LOOPBACK).
            let default_config = if audio_device.wasapi_loopback {
                device
                    .default_output_config()
                    .map_err(|e| format!("Failed to get output config for system audio capture: {e}"))?
            } else if audio_device.is_input {
                match device.default_input_config() {
                    Ok(config) => config,
                    Err(_) => {
                        // Fallback: try to get any supported input config
                        println!("[xos] Device '{}' doesn't support default_input_config(), trying supported configs...", device_name);
                        let mut supported_configs = device.supported_input_configs()
                            .map_err(|e| format!("Failed to get supported input configs: {}", e))?;
                        
                        // Pick the first supported config
                        supported_configs.next()
                            .ok_or_else(|| "No supported input configs found".to_string())?
                            .with_max_sample_rate()
                    }
                }
            } else {
                return Err("Device is not an input device".to_string());
            };

            // Calculate buffer capacity based on duration
            let sample_rate = default_config.sample_rate().0;
            let channels = default_config.channels();
            let capacity = (buffer_duration_secs * sample_rate as f32) as usize;
            
            // Create buffer
            let buffer = AudioBuffer::new(capacity, sample_rate, channels);
            
            // Set up the stream and error callback
            let err_fn = |err| eprintln!("Error in audio stream: {}", err);
            
            // Create the stream based on sample format
            let stream = match default_config.sample_format() {
                SampleFormat::F32 => {
                    let buffer_clone = buffer.clone();
                    let channels_count = channels as usize;
                    
                    device.build_input_stream(
                        &default_config.into(),
                        move |data: &[f32], _: &cpal::InputCallbackInfo| {
                            // Process input data in chunks of channels_count
                            for chunk in data.chunks(channels_count) {
                                if chunk.len() == channels_count {
                                    buffer_clone.push_sample_batch(chunk);
                                }
                            }
                        },
                        err_fn,
                        None,
                    )
                },
                SampleFormat::I16 => {
                    let buffer_clone = buffer.clone();
                    let channels_count = channels as usize;
                    
                    device.build_input_stream(
                        &default_config.into(),
                        move |data: &[i16], _: &cpal::InputCallbackInfo| {
                            // Convert and process in channel chunks
                            let mut float_chunk = vec![0.0; channels_count];
                            
                            for chunk in data.chunks(channels_count) {
                                if chunk.len() == channels_count {
                                    // Convert each i16 to float
                                    for (i, &sample) in chunk.iter().enumerate() {
                                        float_chunk[i] = sample as f32 / i16::MAX as f32;
                                    }
                                    buffer_clone.push_sample_batch(&float_chunk);
                                }
                            }
                        },
                        err_fn,
                        None,
                    )
                },
                SampleFormat::U16 => {
                    let buffer_clone = buffer.clone();
                    let channels_count = channels as usize;
                    
                    device.build_input_stream(
                        &default_config.into(),
                        move |data: &[u16], _: &cpal::InputCallbackInfo| {
                            // Convert and process in channel chunks
                            let mut float_chunk = vec![0.0; channels_count];
                            
                            for chunk in data.chunks(channels_count) {
                                if chunk.len() == channels_count {
                                    // Convert each u16 to float
                                    for (i, &sample) in chunk.iter().enumerate() {
                                        float_chunk[i] = (sample as f32 / u16::MAX as f32) * 2.0 - 1.0;
                                    }
                                    buffer_clone.push_sample_batch(&float_chunk);
                                }
                            }
                        },
                        err_fn,
                        None,
                    )
                },
                _ => return Err("Unsupported sample format".to_string()),
            };
            
            let stream = match stream {
                Ok(stream) => stream,
                Err(e) => return Err(format!("Failed to create audio stream: {}", e)),
            };
            
            // DON'T start the stream automatically - let it stay paused until record() is called
            // This ensures the mic light stays OFF by default
            
            Ok(Self {
                buffer,
                cpal_stream: Some(Arc::new(stream)),
                #[cfg(target_os = "macos")]
                sck_stream: None,
                device_name,
            })
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
            #[cfg(target_os = "macos")]
            if let Some(s) = &self.sck_stream {
                return s
                    .stop_capture()
                    .map_err(|e| format!("ScreenCaptureKit: {e:?}"));
            }
            if let Some(s) = &self.cpal_stream {
                return s.pause().map_err(|e| format!("Failed to pause stream: {}", e));
            }
            Err("No audio capture stream".to_string())
        }
        
        /// Resume/start the audio stream
        pub fn record(&self) -> Result<(), String> {
            #[cfg(target_os = "macos")]
            if let Some(s) = &self.sck_stream {
                return s
                    .start_capture()
                    .map_err(|e| format!("ScreenCaptureKit: {e:?}"));
            }
            if let Some(s) = &self.cpal_stream {
                return s.play().map_err(|e| format!("Failed to resume stream: {}", e));
            }
            Err("No audio capture stream".to_string())
        }
        
        /// Get samples separated by channel
        pub fn get_samples_by_channel(&self) -> Vec<Vec<f32>> {
            self.buffer.get_samples_by_channel()
        }
    }

    /// Get the default input device
    pub fn default_input() -> Option<AudioDevice> {
        let host = cpal::default_host();
        let device = host.default_input_device()?;
        let name = device.name().ok()?;
        Some(AudioDevice {
            name,
            is_input: true,
            is_output: false,
            wasapi_loopback: false,
            macos_sck_system_audio: false,
            device_cpal: device,
        })
    }

    /// Get all available input devices from the system
    pub fn all_input_devices() -> Vec<AudioDevice> {
        let host = cpal::default_host();
        let mut audio_devices = Vec::new();
        
        // Get input devices
        if let Ok(input_devices) = host.input_devices() {
            for device in input_devices {
                if let Ok(name) = device.name() {
                    audio_devices.push(AudioDevice {
                        name,
                        is_input: true,
                        is_output: false,
                        wasapi_loopback: false,
                        macos_sck_system_audio: false,
                        device_cpal: device,
                    });
                }
            }
        }

        // macOS 13+: system mix via ScreenCaptureKit (listed as "System audio").
        #[cfg(target_os = "macos")]
        if let Some(device) = host.default_input_device() {
            audio_devices.push(AudioDevice {
                name: "System audio".to_string(),
                is_input: true,
                is_output: false,
                wasapi_loopback: false,
                macos_sck_system_audio: true,
                device_cpal: device,
            });
        }

        // Windows: each output device can be opened as a loopback capture stream (system audio).
        #[cfg(target_os = "windows")]
        if let Ok(output_devices) = host.output_devices() {
            for device in output_devices {
                if let Ok(name) = device.name() {
                    audio_devices.push(AudioDevice {
                        name: format!("{name} (system audio)"),
                        is_input: true,
                        is_output: false,
                        wasapi_loopback: true,
                        macos_sck_system_audio: false,
                        device_cpal: device,
                    });
                }
            }
        }
        
        audio_devices
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios"), any(target_os = "macos", target_os = "windows")))]
pub use native::{AudioListener, default_input, all_input_devices};

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios"), target_os = "linux"))]
mod linux_no_audio {
    use super::*;

    pub struct AudioListener {
        buffer: AudioBuffer,
        device_name: String,
    }

    impl AudioListener {
        pub fn new(audio_device: &AudioDevice, buffer_duration_secs: f32) -> Result<Self, String> {
            let sample_rate = 48_000u32;
            let channels = 1u16;
            let capacity = (buffer_duration_secs.max(0.1) * sample_rate as f32) as usize;
            let _listener = Self {
                buffer: AudioBuffer::new(capacity.max(1), sample_rate, channels),
                device_name: audio_device.name.clone(),
            };
            Err("native audio input is disabled on this Linux build".to_string())
        }

        pub fn buffer(&self) -> &AudioBuffer {
            &self.buffer
        }

        pub fn device_name(&self) -> &str {
            &self.device_name
        }

        pub fn pause(&self) -> Result<(), String> {
            Ok(())
        }

        pub fn record(&self) -> Result<(), String> {
            Err("native audio input is disabled on this Linux build".to_string())
        }

        pub fn get_samples_by_channel(&self) -> Vec<Vec<f32>> {
            self.buffer.get_samples_by_channel()
        }
    }

    pub fn default_input() -> Option<AudioDevice> {
        None
    }

    pub fn all_input_devices() -> Vec<AudioDevice> {
        Vec::new()
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios"), target_os = "linux"))]
pub use linux_no_audio::{AudioListener, all_input_devices, default_input};

// ================================================================================================
// iOS IMPLEMENTATION using AVAudioEngine via Swift FFI
// ================================================================================================

#[cfg(target_os = "ios")]
mod ios {
    use super::*;

    /// Audio listener to capture audio from a device (iOS version)
    pub struct AudioListener {
        /// The audio buffer (shared via Arc for safe access)
        buffer: AudioBuffer,
        /// The listener ID for iOS FFI
        listener_id: u32,
        /// The device being listened to
        device_name: String,
        /// Raw pointer to boxed buffer for FFI (must be manually freed on drop)
        buffer_ptr: *mut AudioBuffer,
        /// Flag to indicate if the iOS listener has been destroyed
        destroyed: std::sync::atomic::AtomicBool,
    }

    impl AudioListener {
        /// Create a new listener for the specified device.
        ///
        /// `buffer_duration_secs` is the **maximum** ring depth per channel (rolling window).
        pub fn new(audio_device: &AudioDevice, buffer_duration_secs: f32) -> Result<Self, String> {
            if !audio_device.is_input {
                return Err("Device is not an input device".to_string());
            }
            
            // Request a reasonable default, but iOS will use actual hardware rate
            let requested_sample_rate: f64 = 48000.0;
            let channels: u32 = 1; // Mono for now
            
            let listener_id = unsafe {
                xos_audio_listener_init(
                    audio_device.device_id,
                    requested_sample_rate,
                    channels,
                    buffer_duration_secs as f64,
                )
            };
            
            if listener_id == u32::MAX {
                return Err("Failed to initialize audio listener".to_string());
            }
            
            // CRITICAL: Get the ACTUAL sample rate that iOS is using
            let actual_sample_rate = unsafe { xos_audio_listener_get_sample_rate(listener_id) };
            println!("📍 iOS AudioListener: requested={} Hz, actual={} Hz", requested_sample_rate, actual_sample_rate);
            
            // Calculate buffer capacity based on ACTUAL sample rate
            let capacity = (buffer_duration_secs * actual_sample_rate as f32) as usize;
            
            // Create buffer on heap for stable pointer across moves
            let buffer = AudioBuffer::new(capacity, actual_sample_rate as u32, channels as u16);
            let buffer_box = Box::new(buffer.clone());
            let buffer_ptr = Box::into_raw(buffer_box);
            
            // Create the listener (store the raw pointer, we'll manage it manually)
            let listener = Self {
                buffer,
                listener_id,
                device_name: audio_device.name.clone(),
                buffer_ptr,
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
            let result = unsafe { xos_audio_listener_pause(self.listener_id) };
            if result == 0 {
                Ok(())
            } else {
                Err("Failed to pause audio stream".to_string())
            }
        }
        
        /// Resume the audio stream
        pub fn record(&self) -> Result<(), String> {
            let result = unsafe { xos_audio_listener_start(self.listener_id) };
            if result == 0 {
                Ok(())
            } else {
                Err("Failed to resume audio stream".to_string())
            }
        }
        
        /// Get samples separated by channel
        pub fn get_samples_by_channel(&self) -> Vec<Vec<f32>> {
            self.buffer.get_samples_by_channel()
        }
        
        /// Get the listener ID (for direct cleanup)
        pub fn listener_id(&self) -> u32 {
            self.listener_id
        }
        
        /// Mark as destroyed and destroy immediately (for fast cleanup)
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

    // FFI callback function called from Swift
    extern "C" fn audio_callback(samples: *const f32, count: usize, user_data: *mut std::ffi::c_void) {
        if user_data.is_null() || samples.is_null() || count == 0 {
            return;
        }
        
        let buffer = unsafe { &*(user_data as *const AudioBuffer) };
        buffer.push_sample_batch_ffi(samples, count);
    }

    // FFI declarations for iOS audio listener functions
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
        
        fn xos_audio_listener_get_sample_rate(listener_id: u32) -> f64;
    }

    /// Get the default input device (iOS version)
    pub fn default_input() -> Option<AudioDevice> {
        // On iOS, device_id 0 is typically the default input
        let devices = all_input_devices();
        devices.into_iter().find(|d| d.is_input)
    }

    /// Get all available input devices from the system (iOS version)
    pub fn all_input_devices() -> Vec<AudioDevice> {
        // Call Swift to get device count
        let device_count = unsafe { xos_audio_get_device_count() };
        
        let mut audio_devices = Vec::new();
        
        for i in 0..device_count {
            // Get device name from Swift
            let name_ptr = unsafe { xos_audio_get_device_name(i) };
            if name_ptr.is_null() {
                continue;
            }
            
            let name = unsafe {
                let c_str = std::ffi::CStr::from_ptr(name_ptr);
                match c_str.to_str() {
                    Ok(s) => s.to_string(),
                    Err(_) => {
                        xos_audio_free_string(name_ptr);
                        continue;
                    }
                }
            };
            
            // Free the C string
            unsafe { xos_audio_free_string(name_ptr); }
            
            // Get device capabilities
            let is_input = unsafe { xos_audio_device_is_input(i) != 0 };
            
            // Only add input devices
            if is_input {
                audio_devices.push(AudioDevice {
                    name,
                    is_input: true,
                    is_output: false,
                    device_id: i,
                });
            }
        }
        
        audio_devices
    }

    // FFI declarations for iOS audio device functions
    extern "C" {
        fn xos_audio_get_device_count() -> u32;
        fn xos_audio_get_device_name(device_id: u32) -> *const std::os::raw::c_char;
        fn xos_audio_device_is_input(device_id: u32) -> std::os::raw::c_int;
        fn xos_audio_free_string(ptr: *const std::os::raw::c_char);
    }
}

#[cfg(target_os = "ios")]
pub use ios::{AudioListener, default_input, all_input_devices};

// ================================================================================================
// WASM IMPLEMENTATION using Web Audio API
// ================================================================================================

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;
    use web_sys::{AudioContext, AudioProcessingEvent, MediaStream, window};
    use once_cell::unsync::OnceCell;
    use once_cell::sync::OnceCell as SyncOnceCell;

    thread_local! {
        static BUFFER: std::cell::RefCell<Option<Arc<AudioBuffer>>> = std::cell::RefCell::new(None);
        static AUDIO_CONTEXT: OnceCell<AudioContext> = OnceCell::new();
    }

    // Global one-time mic initializer
    static AUDIO_INIT: SyncOnceCell<()> = SyncOnceCell::new();

    #[derive(Clone)]
    pub struct AudioListener {
        buffer: Arc<AudioBuffer>,
    }

    impl AudioListener {
        pub fn new(_device: &AudioDevice, duration_secs: f32) -> Result<Self, String> {
            BUFFER.with(|cell| {
                *cell.borrow_mut() = Some(AudioBuffer::new(
                    (duration_secs * 44100.0) as usize,
                    44100,
                    1,
                ));
            });

            Ok(Self {
                buffer: BUFFER.with(|cell| {
                    cell.borrow().as_ref().unwrap().clone()
                }),
            })
        }

        pub fn record(&self) -> Result<(), String> {
            Ok(())
        }

        pub fn pause(&self) -> Result<(), String> {
            Ok(())
        }

        pub fn get_samples_by_channel(&self) -> Vec<Vec<f32>> {
            self.buffer.get_samples_by_channel()
        }

        pub fn buffer(&self) -> &AudioBuffer {
            &self.buffer
        }

        pub fn device_name(&self) -> &str {
            "Web audio input"
        }

        pub fn duration(&self) -> f32 {
            self.buffer.duration()
        }

        pub fn sample_rate(&self) -> u32 {
            self.buffer.sample_rate
        }
    }

    #[wasm_bindgen]
    pub async fn init_microphone() -> Result<(), JsValue> {
        let window = window().unwrap();
        let navigator = window.navigator();
        let media_devices = navigator.media_devices()?;

        let constraints = js_sys::Object::new();
        js_sys::Reflect::set(
            &constraints,
            &JsValue::from_str("audio"),
            &JsValue::TRUE,
        )?;

        let stream_promise = media_devices.get_user_media_with_constraints(
            constraints.unchecked_ref()
        )?;

        let stream = wasm_bindgen_futures::JsFuture::from(stream_promise).await?;
        let stream: MediaStream = stream.dyn_into()?;

        let context = AudioContext::new()?;
        let source = context.create_media_stream_source(&stream)?;
        let processor = context.create_script_processor_with_buffer_size(1024)?;

        let closure = Closure::<dyn FnMut(_)>::wrap(Box::new(move |event: AudioProcessingEvent| {
            let input_buf = event.input_buffer().unwrap();
            let input = input_buf.get_channel_data(0).unwrap();

            BUFFER.with(|cell| {
                if let Some(buffer) = &*cell.borrow() {
                    buffer.push(&input.to_vec());
                }
            });
        }) as Box<dyn FnMut(_)>);

        processor.set_onaudioprocess(Some(closure.as_ref().unchecked_ref()));
        closure.forget(); // Leak to JS for lifetime safety

        source.connect_with_audio_node(&processor)?;
        processor.connect_with_audio_node(&context.destination())?;

        AUDIO_CONTEXT.with(|ctx| {
            ctx.set(context).ok();
        });

        Ok(())
    }

    /// Returns a fake device that represents the browser mic
    pub fn all_input_devices() -> Vec<AudioDevice> {
        use wasm_bindgen_futures::spawn_local;
        use web_sys::console;

        AUDIO_INIT.get_or_init(|| {
            spawn_local(async {
                match init_microphone().await {
                    Ok(_) => console::log_1(&"🎤 Mic initialized".into()),
                    Err(err) => console::error_1(&format!("❌ Mic init failed: {err:?}").into()),
                }
            });
        });

        vec![AudioDevice {
            name: "Web Mic".to_string(),
            is_input: true,
            is_output: false,
        }]
    }

    pub fn default_input() -> Option<AudioDevice> {
        all_input_devices().into_iter().next()
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::{AudioListener, default_input, all_input_devices};

// ================================================================================================
// CONVENIENCE FUNCTIONS
// ================================================================================================

/// Print information about all available input devices
pub fn print_input_devices() {
    let devices = all_input_devices();
    println!("XOS Audio: {} input device(s) detected", devices.len());
    
    for (i, device) in devices.iter().enumerate() {
        println!("  {}: {}", i+1, device.input_menu_label());
    }
}

