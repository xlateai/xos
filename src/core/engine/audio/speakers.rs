//! Audio **output** / playback (module name: `speakers` for history).
//! 
//! This module provides audio output functionality across:
//! - macOS/Linux (native) using CPAL
//! - iOS using AVAudioEngine via Swift FFI
//! - WASM (TODO)
//!
//! ## Playback Semantics
//! 
//! The AudioPlayer uses a queue-based system:
//! - Samples are queued for playback via `play_samples()`
//! - The audio thread continuously drains the queue
//! - If the queue runs empty, silence is played
//! - Clear the queue with `clear()` to stop pending audio

use std::sync::{Arc, Mutex};
use std::collections::VecDeque;

// ================================================================================================
// COMMON TYPES
// ================================================================================================

// AudioDevice is defined in mod.rs and re-exported here for convenience
use super::AudioDevice;

// ================================================================================================
// PLAYBACK BUFFER (Shared for Native implementation)
// ================================================================================================

/// Playback buffer to queue audio samples for output, separated by channel
#[derive(Clone)]
pub struct PlaybackBuffer {
    /// Sample queue per channel: Vec[channel_idx] -> samples to play
    channel_queues: Arc<Mutex<Vec<VecDeque<f32>>>>,
    /// Sample rate of the audio
    #[allow(dead_code)]
    sample_rate: u32,
    /// Number of channels
    #[allow(dead_code)]
    channels: u16,
}

impl PlaybackBuffer {
    fn new(sample_rate: u32, channels: u16) -> Self {
        // Create a vector of empty VecDeques, one for each channel
        let mut channel_buffers = Vec::with_capacity(channels as usize);
        for _ in 0..channels {
            channel_buffers.push(VecDeque::new());
        }
        
        Self {
            channel_queues: Arc::new(Mutex::new(channel_buffers)),
            sample_rate,
            channels,
        }
    }

    /// Add samples to the playback queue (interleaved format: [L, R, L, R, ...])
    pub fn queue_samples(&self, samples: &[f32]) {
        let mut channel_queues = self.channel_queues.lock().unwrap();
        let num_channels = channel_queues.len();
        
        if num_channels == 0 || samples.is_empty() {
            return;
        }
        
        // De-interleave samples and add to respective channel queues
        for (i, &sample) in samples.iter().enumerate() {
            let channel_idx = i % num_channels;
            channel_queues[channel_idx].push_back(sample);
        }
    }
    
    /// Pop a frame of samples (one per channel) for playback
    /// Returns None if any channel is empty
    fn pop_frame(&self) -> Option<Vec<f32>> {
        let mut channel_queues = self.channel_queues.lock().unwrap();
        
        // Check if all channels have at least one sample
        if channel_queues.iter().any(|q| q.is_empty()) {
            return None;
        }
        
        // Pop one sample from each channel
        let frame: Vec<f32> = channel_queues
            .iter_mut()
            .map(|q| q.pop_front().unwrap_or(0.0))
            .collect();
        
        Some(frame)
    }
    
    /// Get the current number of samples queued (from first channel)
    pub fn get_queued_count(&self) -> usize {
        let channel_queues = self.channel_queues.lock().unwrap();
        if channel_queues.is_empty() {
            0
        } else {
            channel_queues[0].len()
        }
    }
    
    /// Clear all queued samples
    pub fn clear(&self) {
        let mut channel_queues = self.channel_queues.lock().unwrap();
        for queue in channel_queues.iter_mut() {
            queue.clear();
        }
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

// ================================================================================================
// NATIVE (macOS/Linux) IMPLEMENTATION using CPAL
// ================================================================================================

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
mod native {
    use super::*;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::{SampleFormat, Stream};

    /// Audio player for audio output
    pub struct AudioPlayer {
        /// The playback buffer (shared via Arc for safe access from audio thread)
        buffer: PlaybackBuffer,
        /// The output stream
        stream: Arc<Stream>,
        /// The device being used
        device_name: String,
        /// Sample rate
        sample_rate: u32,
        /// Number of channels
        channels: u16,
    }

    impl AudioPlayer {
        /// Create a new audio player for the specified device
        pub fn new(audio_device: &AudioDevice, sample_rate: u32, channels: u16) -> Result<Self, String> {
            let device = &audio_device.device_cpal;

            // Get device name
            let device_name = match device.name() {
                Ok(name) => name,
                Err(_) => return Err("Could not get device name".to_string()),
            };
            
            // Try to get default output config
            let default_config = match device.default_output_config() {
                Ok(config) => config,
                Err(e) => {
                    // Fallback: try to get any supported output config
                    println!("[xos] Device '{}' doesn't support default_output_config(): {}", device_name, e);
                    println!("[xos] Trying supported_output_configs()...");
                    
                    match device.supported_output_configs() {
                        Ok(mut configs) => {
                            let config = configs.next()
                                .ok_or_else(|| "No supported output configs found".to_string())?
                                .with_max_sample_rate();
                            println!("[xos] Using config: {} Hz, {} channels", config.sample_rate().0, config.channels());
                            config
                        }
                        Err(e) => {
                            println!("[xos] supported_output_configs() also failed: {}", e);
                            return Err(format!("Device supports no output configs: {}", e));
                        }
                    }
                }
            };

            // Use provided sample rate and channels, but fall back to device defaults if needed
            // AirPods and many Bluetooth devices only support stereo (2 channels)
            let actual_sample_rate = sample_rate;
            let actual_channels = if channels == 1 && default_config.channels() >= 2 {
                // Device prefers stereo but we requested mono - use stereo to ensure compatibility
                println!("[xos] Device '{}' prefers stereo, upgrading from mono to stereo for compatibility", device_name);
                2
            } else {
                channels
            };
            
            // Create playback buffer
            let buffer = PlaybackBuffer::new(actual_sample_rate, actual_channels);
            
            // Set up the stream and error callback
            let err_fn = |err| eprintln!("Error in audio output stream: {}", err);
            
            // Create the output stream based on sample format
            let stream = match default_config.sample_format() {
                SampleFormat::F32 => {
                    let buffer_clone = buffer.clone();
                    let channels_count = actual_channels as usize;
                    
                    // Build config with our desired sample rate and channels
                    let config = cpal::StreamConfig {
                        channels: actual_channels,
                        sample_rate: cpal::SampleRate(actual_sample_rate),
                        buffer_size: cpal::BufferSize::Default,
                    };
                    
                    device.build_output_stream(
                        &config,
                        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                            // Fill output buffer with samples from our queue
                            for chunk in data.chunks_mut(channels_count) {
                                if let Some(frame) = buffer_clone.pop_frame() {
                                    // Copy frame to output
                                    for (i, &sample) in frame.iter().enumerate() {
                                        if i < chunk.len() {
                                            chunk[i] = sample;
                                        }
                                    }
                                } else {
                                    // No samples available - output silence
                                    for sample in chunk.iter_mut() {
                                        *sample = 0.0;
                                    }
                                }
                            }
                        },
                        err_fn,
                        None,
                    )
                },
                SampleFormat::I16 => {
                    let buffer_clone = buffer.clone();
                    let channels_count = actual_channels as usize;
                    
                    let config = cpal::StreamConfig {
                        channels: actual_channels,
                        sample_rate: cpal::SampleRate(actual_sample_rate),
                        buffer_size: cpal::BufferSize::Default,
                    };
                    
                    device.build_output_stream(
                        &config,
                        move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                            // Fill output buffer with samples from our queue
                            for chunk in data.chunks_mut(channels_count) {
                                if let Some(frame) = buffer_clone.pop_frame() {
                                    // Convert float to i16 and copy
                                    for (i, &sample) in frame.iter().enumerate() {
                                        if i < chunk.len() {
                                            chunk[i] = (sample * i16::MAX as f32) as i16;
                                        }
                                    }
                                } else {
                                    // No samples available - output silence
                                    for sample in chunk.iter_mut() {
                                        *sample = 0;
                                    }
                                }
                            }
                        },
                        err_fn,
                        None,
                    )
                },
                SampleFormat::U16 => {
                    let buffer_clone = buffer.clone();
                    let channels_count = actual_channels as usize;
                    
                    let config = cpal::StreamConfig {
                        channels: actual_channels,
                        sample_rate: cpal::SampleRate(actual_sample_rate),
                        buffer_size: cpal::BufferSize::Default,
                    };
                    
                    device.build_output_stream(
                        &config,
                        move |data: &mut [u16], _: &cpal::OutputCallbackInfo| {
                            // Fill output buffer with samples from our queue
                            for chunk in data.chunks_mut(channels_count) {
                                if let Some(frame) = buffer_clone.pop_frame() {
                                    // Convert float to u16 and copy
                                    for (i, &sample) in frame.iter().enumerate() {
                                        if i < chunk.len() {
                                            // Map [-1.0, 1.0] to [0, u16::MAX]
                                            chunk[i] = ((sample + 1.0) * 0.5 * u16::MAX as f32) as u16;
                                        }
                                    }
                                } else {
                                    // No samples available - output silence (middle value)
                                    for sample in chunk.iter_mut() {
                                        *sample = u16::MAX / 2;
                                    }
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
                Err(e) => return Err(format!("Failed to create audio output stream: {}", e)),
            };
            
            // Start the stream
            if let Err(e) = stream.play() {
                return Err(format!("Failed to start audio output stream: {}", e));
            }
            
            Ok(Self {
                buffer,
                stream: Arc::new(stream),
                device_name,
                sample_rate: actual_sample_rate,
                channels: actual_channels,
            })
        }
        
        /// Queue samples for playback
        pub fn play_samples(&self, samples: &[f32]) -> Result<(), String> {
            // If device is stereo but we have mono input, duplicate samples to both channels
            if self.buffer.channels() == 2 && samples.len() > 0 {
                // Check if samples are already interleaved stereo or mono
                // Assume mono input, convert to stereo by duplicating each sample
                let mut stereo_samples = Vec::with_capacity(samples.len() * 2);
                for &sample in samples {
                    stereo_samples.push(sample); // Left channel
                    stereo_samples.push(sample); // Right channel
                }
                self.buffer.queue_samples(&stereo_samples);
            } else {
                self.buffer.queue_samples(samples);
            }
            Ok(())
        }
        
        /// Get the current buffer size (number of queued samples)
        pub fn get_buffer_size(&self) -> usize {
            self.buffer.get_queued_count()
        }
        
        /// Get the device name
        pub fn device_name(&self) -> &str {
            &self.device_name
        }
        
        /// Start playback (stream is auto-started in new())
        pub fn start(&self) -> Result<(), String> {
            self.stream.play().map_err(|e| format!("Failed to start playback: {}", e))
        }
        
        /// Stop playback
        #[allow(dead_code)]
        pub fn stop(&self) -> Result<(), String> {
            self.stream.pause().map_err(|e| format!("Failed to stop playback: {}", e))
        }
        
        /// Clear the playback buffer
        pub fn clear(&self) {
            self.buffer.clear();
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

    impl Drop for AudioPlayer {
        fn drop(&mut self) {
            // Stream will be automatically stopped when dropped
            let _ = self.stream.pause();
        }
    }

    /// Get the default output device
    pub fn default_output() -> Option<AudioDevice> {
        let host = cpal::default_host();
        let device = host.default_output_device()?;
        let name = device.name().ok()?;
        Some(AudioDevice {
            name,
            is_input: false,
            is_output: true,
            wasapi_loopback: false,
            macos_sck_system_audio: false,
            device_cpal: device,
        })
    }

    /// Get all available output devices from the system
    pub fn all_output_devices() -> Vec<AudioDevice> {
        let host = cpal::default_host();
        let mut audio_devices = Vec::new();
        
        // Get output devices
        if let Ok(output_devices) = host.output_devices() {
            for device in output_devices {
                if let Ok(name) = device.name() {
                    audio_devices.push(AudioDevice {
                        name,
                        is_input: false,
                        is_output: true,
                        wasapi_loopback: false,
                        macos_sck_system_audio: false,
                        device_cpal: device,
                    });
                }
            }
        }
        
        audio_devices
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
pub use native::{AudioPlayer, default_output, all_output_devices};

// ================================================================================================
// iOS IMPLEMENTATION using AVAudioEngine via Swift FFI
// ================================================================================================

#[cfg(target_os = "ios")]
mod ios {
    use super::*;

    /// Audio player for speaker output (iOS version)
    pub struct AudioPlayer {
        player_id: u32,
        sample_rate: u32,
        channels: u16,
    }

    impl AudioPlayer {
        /// Create a new audio player for the specified device
        pub fn new(audio_device: &AudioDevice, sample_rate: u32, channels: u16) -> Result<Self, String> {
            if !audio_device.is_output {
                return Err("Device is not an output device".to_string());
            }
            
            let player_id = unsafe {
                xos_audio_player_init(audio_device.device_id, sample_rate as f64, channels as u32)
            };
            
            if player_id == u32::MAX {
                return Err("Failed to initialize audio player".to_string());
            }
            
            Ok(Self {
                player_id,
                sample_rate,
                channels,
            })
        }
        
        /// Queue samples for playback
        pub fn play_samples(&self, samples: &[f32]) -> Result<(), String> {
            let result = unsafe {
                xos_audio_player_queue_samples(
                    self.player_id,
                    samples.as_ptr(),
                    samples.len(),
                )
            };
            
            if result != 0 {
                return Err("Failed to queue audio samples".to_string());
            }
            
            Ok(())
        }
        
        /// Get the current buffer size (number of queued samples)
        pub fn get_buffer_size(&self) -> usize {
            unsafe { xos_audio_player_get_buffer_size(self.player_id) as usize }
        }
        
        /// Get the device name (not available in current implementation)
        pub fn device_name(&self) -> &str {
            "iOS Speaker"
        }
        
        /// Start playback
        pub fn start(&self) -> Result<(), String> {
            let result = unsafe { xos_audio_player_start(self.player_id) };
            if result != 0 {
                return Err("Failed to start audio player".to_string());
            }
            Ok(())
        }
        
        /// Stop playback
        #[allow(dead_code)]
        pub fn stop(&self) -> Result<(), String> {
            let result = unsafe { xos_audio_player_stop(self.player_id) };
            if result != 0 {
                return Err("Failed to stop audio player".to_string());
            }
            Ok(())
        }
        
        /// Clear the playback buffer (not implemented in iOS FFI yet)
        #[allow(dead_code)]
        pub fn clear(&self) {
            // TODO: Implement in iOS FFI
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

    impl Drop for AudioPlayer {
        fn drop(&mut self) {
            crate::print(&format!("[AudioPlayer] Rust Drop: Cleaning up player ID={}", self.player_id));
            unsafe {
                // First try to stop gracefully
                let stop_result = xos_audio_player_stop(self.player_id);
                if stop_result != 0 {
                    crate::print(&format!("[AudioPlayer] Warning: Stop returned non-zero: {}", stop_result));
                }
                
                // Then destroy the player
                xos_audio_player_destroy(self.player_id);
            }
            crate::print(&format!("[AudioPlayer] Rust Drop: Player ID={} cleaned up", self.player_id));
        }
    }

    // FFI declarations for iOS audio player functions
    extern "C" {
        fn xos_audio_player_init(
            device_id: u32,
            sample_rate: f64,
            channels: u32,
        ) -> u32;
        
        fn xos_audio_player_queue_samples(
            player_id: u32,
            samples: *const f32,
            count: usize,
        ) -> std::os::raw::c_int;
        
        fn xos_audio_player_get_buffer_size(player_id: u32) -> u32;
        
        fn xos_audio_player_start(player_id: u32) -> std::os::raw::c_int;
        
        fn xos_audio_player_stop(player_id: u32) -> std::os::raw::c_int;
        
        fn xos_audio_player_destroy(player_id: u32);
    }

    /// Get the default output device (iOS version)
    pub fn default_output() -> Option<AudioDevice> {
        // On iOS, find the first output device
        let devices = all_output_devices();
        devices.into_iter().find(|d| d.is_output)
    }

    /// Get all available output devices from the system (iOS version)
    pub fn all_output_devices() -> Vec<AudioDevice> {
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
            let is_output = unsafe { xos_audio_device_is_output(i) != 0 };
            
            // Only add output devices
            if is_output {
                audio_devices.push(AudioDevice {
                    name,
                    is_input: false,
                    is_output: true,
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
        fn xos_audio_device_is_output(device_id: u32) -> std::os::raw::c_int;
        fn xos_audio_free_string(ptr: *const std::os::raw::c_char);
    }
}

#[cfg(target_os = "ios")]
pub use ios::{AudioPlayer, default_output, all_output_devices};

// ================================================================================================
// WASM IMPLEMENTATION (Stub for now)
// ================================================================================================

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;

    pub struct AudioPlayer {
        // TODO: Implement WASM audio player
    }

    impl AudioPlayer {
        pub fn new(_audio_device: &AudioDevice, _sample_rate: u32, _channels: u16) -> Result<Self, String> {
            Err("WASM audio player not yet implemented".to_string())
        }

        pub fn play_samples(&self, _samples: &[f32]) -> Result<(), String> {
            Ok(())
        }

        pub fn get_buffer_size(&self) -> usize {
            0
        }

        pub fn device_name(&self) -> &str {
            "Web Speaker"
        }

        pub fn start(&self) -> Result<(), String> {
            Ok(())
        }

        pub fn stop(&self) -> Result<(), String> {
            Ok(())
        }

        pub fn clear(&self) {
            // No-op
        }

        pub fn sample_rate(&self) -> u32 {
            44100
        }

        pub fn channels(&self) -> u16 {
            2
        }
    }

    pub fn all_output_devices() -> Vec<AudioDevice> {
        vec![AudioDevice {
            name: "Web Speaker".to_string(),
            is_input: false,
            is_output: true,
        }]
    }

    pub fn default_output() -> Option<AudioDevice> {
        all_output_devices().into_iter().next()
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::{AudioPlayer, default_output, all_output_devices};

// ================================================================================================
// CONVENIENCE FUNCTIONS
// ================================================================================================

/// Print information about all available output devices
pub fn print_output_devices() {
    let devices = all_output_devices();
    println!("XOS Audio: {} output device(s) detected", devices.len());
    
    for (i, device) in devices.iter().enumerate() {
        println!("  {}: {}", i+1, device);
    }
}

