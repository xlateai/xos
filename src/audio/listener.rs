use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use std::sync::{Arc, Mutex};
use std::collections::VecDeque;
use std::time::Instant;

use super::device::AudioDevice;

/// Buffer to store audio samples, separated by channel
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

    // /// Add samples to the buffer (one sample per channel)
    // fn push_sample_batch(&self, samples: &[f32]) {
    //     let mut channel_buffers = self.channel_samples.lock().unwrap();
        
    //     // Check if we have the right number of samples
    //     if samples.len() != channel_buffers.len() {
    //         // Handle error case - incomplete batch of samples
    //         // For now, just return without updating
    //         return;
    //     }
        
    //     // Add each sample to its corresponding channel buffer
    //     for (channel_idx, &sample) in samples.iter().enumerate() {
    //         let buffer = &mut channel_buffers[channel_idx];
            
    //         // If buffer is at capacity, remove oldest sample
    //         if buffer.len() >= self.capacity {
    //             buffer.pop_front();
    //         }
            
    //         // Add new sample
    //         buffer.push_back(sample);
    //     }
        
    //     // Update last access time
    //     *self.last_access.lock().unwrap() = Instant::now();
    // }
    
    /// Add samples to specific channel range
    /// Used by MultiDeviceAudioListener to add samples from a specific device
    fn push_sample_batch_to_channels(&self, samples: &[f32], start_channel: usize) {
        let mut channel_buffers = self.channel_samples.lock().unwrap();
        
        // Check if we have enough channels for this operation
        if start_channel + samples.len() > channel_buffers.len() {
            // Handle error case - out of range
            return;
        }
        
        // Add each sample to its corresponding channel buffer
        for (i, &sample) in samples.iter().enumerate() {
            let channel_idx = start_channel + i;
            let buffer = &mut channel_buffers[channel_idx];
            
            // If buffer is at capacity, remove oldest sample
            if buffer.len() >= self.capacity {
                buffer.pop_front();
            }
            
            // Add new sample
            buffer.push_back(sample);
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

/// Information about a device being listened to
#[derive(Clone)]
struct DeviceInfo {
    /// The name of the device
    name: String,
    /// The number of channels for this device
    channels: u16,
    /// The device type (input or output)
    is_input: bool,
    /// The sample rate
    sample_rate: u32,
    /// The stream handling this device
    stream: Arc<Stream>,
    /// The starting channel index in the combined buffer
    start_channel_idx: usize,
}

/// Audio listener to capture audio from multiple devices
#[derive(Clone)]
pub struct MultiDeviceAudioListener {
    /// The shared audio buffer containing all channels from all devices
    buffer: AudioBuffer,
    /// Information about all devices being monitored
    devices: Vec<DeviceInfo>,
    /// Total number of channels across all devices
    total_channels: u16,
    /// Whether the listener is currently recording
    is_recording: Arc<Mutex<bool>>,
}

impl MultiDeviceAudioListener {
    /// Create a new multi-device listener
    pub fn new(devices: &[AudioDevice], buffer_duration_secs: f32) -> Result<Self, String> {
        if devices.is_empty() {
            return Err("No devices provided".to_string());
        }
        
        // Calculate total number of channels and determine the highest sample rate
        let mut total_channels: usize = 0;
        let mut highest_sample_rate: u32 = 0;
        let mut device_channels: Vec<(usize, u16)> = Vec::with_capacity(devices.len());
        
        for device in devices {
            let config = if device.is_input {
                device.device_cpal.default_input_config()
            } else {
                device.device_cpal.default_output_config()
            };
            
            let config = match config {
                Ok(config) => config,
                Err(e) => return Err(format!("Failed to get device config: {}", e)),
            };
            
            let device_sample_rate = config.sample_rate().0;
            let channels = config.channels();
            
            highest_sample_rate = highest_sample_rate.max(device_sample_rate);
            device_channels.push((total_channels, channels));
            total_channels += channels as usize;
        }
        
        // Create a buffer that can hold all channels
        let capacity = (buffer_duration_secs * highest_sample_rate as f32) as usize;
        let buffer = AudioBuffer::new(capacity, highest_sample_rate, total_channels as u16);
        
        // Create streams for all devices
        let mut device_infos = Vec::with_capacity(devices.len());
        let is_recording = Arc::new(Mutex::new(false));
        
        for (i, device) in devices.iter().enumerate() {
            let (start_channel_idx, channels) = device_channels[i];
            let cpal_device = &device.device_cpal;
            
            // Get device configuration
            let config = if device.is_input {
                cpal_device.default_input_config()
            } else {
                cpal_device.default_output_config()
            };
            
            let config = match config {
                Ok(config) => config,
                Err(e) => return Err(format!("Failed to get device config: {}", e)),
            };
            
            // Get device name - using a clone so we can use it after the move to the closure
            let device_name = match cpal_device.name() {
                Ok(name) => name.clone(), // Clone the name here
                Err(_) => return Err("Could not get device name".to_string()),
            };
            
            let sample_rate = config.sample_rate().0;
            let channels_count = channels as usize;
            let buffer_clone = buffer.clone();
            let start_idx = start_channel_idx;
            let is_recording_clone = is_recording.clone();
            
            // Set up error callback with a cloned device name
            let device_name_for_callback = device_name.clone(); // Clone again for the closure
            let err_fn = move |err| eprintln!("Error in audio stream for device {}: {}", device_name_for_callback, err);
            
            // Create the stream based on sample format and device type
            let stream = if device.is_input {
                match config.sample_format() {
                    SampleFormat::F32 => {
                        let buffer_clone = buffer_clone.clone();
                        let is_rec = is_recording_clone.clone();
                        
                        cpal_device.build_input_stream(
                            &config.into(),
                            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                                // Skip if not recording
                                if !*is_rec.lock().unwrap() {
                                    return;
                                }
                                
                                // Process input data in chunks of channels_count
                                for chunk in data.chunks(channels_count) {
                                    if chunk.len() == channels_count {
                                        buffer_clone.push_sample_batch_to_channels(chunk, start_idx);
                                    }
                                }
                            },
                            err_fn.clone(),
                            None,
                        )
                    },
                    SampleFormat::I16 => {
                        let buffer_clone = buffer_clone.clone();
                        let is_rec = is_recording_clone.clone();
                        
                        cpal_device.build_input_stream(
                            &config.into(),
                            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                                // Skip if not recording
                                if !*is_rec.lock().unwrap() {
                                    return;
                                }
                                
                                // Convert and process in channel chunks
                                let mut float_chunk = vec![0.0; channels_count];
                                
                                for chunk in data.chunks(channels_count) {
                                    if chunk.len() == channels_count {
                                        // Convert each i16 to float
                                        for (i, &sample) in chunk.iter().enumerate() {
                                            float_chunk[i] = sample as f32 / i16::MAX as f32;
                                        }
                                        buffer_clone.push_sample_batch_to_channels(&float_chunk, start_idx);
                                    }
                                }
                            },
                            err_fn.clone(),
                            None,
                        )
                    },
                    SampleFormat::U16 => {
                        let buffer_clone = buffer_clone.clone();
                        let is_rec = is_recording_clone.clone();
                        
                        cpal_device.build_input_stream(
                            &config.into(),
                            move |data: &[u16], _: &cpal::InputCallbackInfo| {
                                // Skip if not recording
                                if !*is_rec.lock().unwrap() {
                                    return;
                                }
                                
                                // Convert and process in channel chunks
                                let mut float_chunk = vec![0.0; channels_count];
                                
                                for chunk in data.chunks(channels_count) {
                                    if chunk.len() == channels_count {
                                        // Convert each u16 to float
                                        for (i, &sample) in chunk.iter().enumerate() {
                                            float_chunk[i] = (sample as f32 / u16::MAX as f32) * 2.0 - 1.0;
                                        }
                                        buffer_clone.push_sample_batch_to_channels(&float_chunk, start_idx);
                                    }
                                }
                            },
                            err_fn.clone(),
                            None,
                        )
                    },
                    _ => return Err("Unsupported sample format".to_string()),
                }
            } else { // Output device loopback
                match config.sample_format() {
                    SampleFormat::F32 => {
                        let buffer_clone = buffer_clone.clone();
                        let is_rec = is_recording_clone.clone();
                        
                        cpal_device.build_output_stream(
                            &config.into(),
                            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                                // Skip if not recording
                                if !*is_rec.lock().unwrap() {
                                    return;
                                }
                                
                                // We're monitoring output, so capture it before it's played
                                // Process output data in chunks of channels_count
                                for chunk in data.chunks(channels_count) {
                                    if chunk.len() == channels_count {
                                        buffer_clone.push_sample_batch_to_channels(chunk, start_idx);
                                    }
                                }
                            },
                            err_fn.clone(),
                            None,
                        )
                    },
                    SampleFormat::I16 => {
                        let buffer_clone = buffer_clone.clone();
                        let is_rec = is_recording_clone.clone();
                        
                        cpal_device.build_output_stream(
                            &config.into(),
                            move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                                // Skip if not recording
                                if !*is_rec.lock().unwrap() {
                                    return;
                                }
                                
                                // Convert and process in channel chunks
                                let mut float_chunk = vec![0.0; channels_count];
                                
                                for chunk in data.chunks(channels_count) {
                                    if chunk.len() == channels_count {
                                        // Convert each i16 to float
                                        for (i, &sample) in chunk.iter().enumerate() {
                                            float_chunk[i] = sample as f32 / i16::MAX as f32;
                                        }
                                        buffer_clone.push_sample_batch_to_channels(&float_chunk, start_idx);
                                    }
                                }
                            },
                            err_fn.clone(),
                            None,
                        )
                    },
                    SampleFormat::U16 => {
                        let buffer_clone = buffer_clone.clone();
                        let is_rec = is_recording_clone.clone();
                        
                        cpal_device.build_output_stream(
                            &config.into(),
                            move |data: &mut [u16], _: &cpal::OutputCallbackInfo| {
                                // Skip if not recording
                                if !*is_rec.lock().unwrap() {
                                    return;
                                }
                                
                                // Convert and process in channel chunks
                                let mut float_chunk = vec![0.0; channels_count];
                                
                                for chunk in data.chunks(channels_count) {
                                    if chunk.len() == channels_count {
                                        // Convert each u16 to float
                                        for (i, &sample) in chunk.iter().enumerate() {
                                            float_chunk[i] = (sample as f32 / u16::MAX as f32) * 2.0 - 1.0;
                                        }
                                        buffer_clone.push_sample_batch_to_channels(&float_chunk, start_idx);
                                    }
                                }
                            },
                            err_fn.clone(),
                            None,
                        )
                    },
                    _ => return Err("Unsupported sample format".to_string()),
                }
            };
            
            let stream = match stream {
                Ok(stream) => stream,
                Err(e) => return Err(format!("Failed to create audio stream for device {}: {}", device_name, e)),
            };
            
            // Start the stream
            if let Err(e) = stream.play() {
                return Err(format!("Failed to start audio stream for device {}: {}", device_name, e));
            }
            
            // Add device info
            device_infos.push(DeviceInfo {
                name: device_name,
                channels,
                is_input: device.is_input,
                sample_rate,
                stream: Arc::new(stream),
                start_channel_idx,
            });
        }
        
        Ok(Self {
            buffer,
            devices: device_infos,
            total_channels: total_channels as u16,
            is_recording,
        })
    }
    
    /// Get a reference to the audio buffer
    pub fn buffer(&self) -> &AudioBuffer {
        &self.buffer
    }
    
    /// Get information about all devices
    pub fn get_device_info(&self) -> Vec<(String, u16, bool, usize)> {
        self.devices.iter()
            .map(|dev| (dev.name.clone(), dev.channels, dev.is_input, dev.start_channel_idx))
            .collect()
    }
    
    /// Pause all audio streams
    pub fn pause(&self) -> Result<(), String> {
        // Update recording flag first
        *self.is_recording.lock().unwrap() = false;
        
        // Pause each stream
        for device in &self.devices {
            if let Err(e) = device.stream.pause() {
                return Err(format!("Failed to pause stream for device {}: {}", device.name, e));
            }
        }
        
        Ok(())
    }
    
    /// Resume all audio streams
    pub fn record(&self) -> Result<(), String> {
        // Start each stream first
        for device in &self.devices {
            if let Err(e) = device.stream.play() {
                return Err(format!("Failed to resume stream for device {}: {}", device.name, e));
            }
        }
        
        // Update recording flag last
        *self.is_recording.lock().unwrap() = true;
        
        Ok(())
    }
    
    /// Get total number of channels across all devices
    pub fn total_channels(&self) -> u16 {
        self.total_channels
    }
    
    /// Get samples separated by channel
    pub fn get_samples_by_channel(&self) -> Vec<Vec<f32>> {
        self.buffer.get_samples_by_channel()
    }
}

/// For backward compatibility, alias to MultiDeviceAudioListener
pub type AudioListener = MultiDeviceAudioListener;