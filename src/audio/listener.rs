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

    /// Add samples to the buffer (one sample per channel)
    fn push_sample_batch(&self, samples: &[f32]) {
        let mut channel_buffers = self.channel_samples.lock().unwrap();
        
        // Check if we have the right number of samples
        if samples.len() != channel_buffers.len() {
            // Handle error case - incomplete batch of samples
            // For now, just return without updating
            return;
        }
        
        // Add each sample to its corresponding channel buffer
        for (channel_idx, &sample) in samples.iter().enumerate() {
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

/// Audio listener to capture audio from a device
#[derive(Clone)]
pub struct AudioListener {
    /// The audio buffer
    buffer: AudioBuffer,
    /// The audio stream
    stream: Arc<Stream>,
    /// The device being listened to
    device_name: String,
}

impl AudioListener {
    /// Create a new listener for the specified device
    pub fn new(audio_device: &AudioDevice, buffer_duration_secs: f32) -> Result<Self, String> {
        let device = &audio_device.device_cpal;

        // Get device name
        let device_name = match device.name() {
            Ok(name) => name,
            Err(_) => return Err("Could not get device name".to_string()),
        };
        
        // Get default config for the device
        let dic;
        if audio_device.is_input {
            dic = device.default_input_config();
        } else if audio_device.is_output {
            dic = device.default_output_config();
        } else {
            return Err("Device is neither input nor output".to_string());
        }

        let default_config = match dic {
            Ok(config) => config,
            Err(e) => return Err(format!("Failed to get default input config: {}", e)),
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
        
        // Start the stream
        if let Err(e) = stream.play() {
            return Err(format!("Failed to start audio stream: {}", e));
        }
        
        Ok(Self {
            buffer,
            stream: Arc::new(stream),
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
        self.stream.pause().map_err(|e| format!("Failed to pause stream: {}", e))
    }
    
    /// Resume the audio stream
    pub fn record(&self) -> Result<(), String> {
        self.stream.play().map_err(|e| format!("Failed to resume stream: {}", e))
    }
    
    /// Get samples separated by channel
    pub fn get_samples_by_channel(&self) -> Vec<Vec<f32>> {
        self.buffer.get_samples_by_channel()
    }
}