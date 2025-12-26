use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use std::sync::{Arc, Mutex};
use std::collections::VecDeque;

use crate::audio::native_device::AudioDevice;

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
        
        // Try to get default output config, but if that fails (e.g., for some Bluetooth devices),
        // try to find a supported config instead
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
                        println!("[xos] Trying supported_input_configs() as last resort...");
                        
                        // Last resort: some devices report as input even when they're output
                        let mut configs = device.supported_input_configs()
                            .map_err(|e| format!("Device supports neither input nor output configs: {}", e))?;
                        
                        let config = configs.next()
                            .ok_or_else(|| "No supported configs found at all".to_string())?
                            .with_max_sample_rate();
                        println!("[xos] Using input config as output: {} Hz, {} channels", config.sample_rate().0, config.channels());
                        config
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

