use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, Stream};
use std::sync::{Arc, Mutex};
use std::collections::VecDeque;
use std::time::Instant;

/// Buffer to store audio samples
#[derive(Clone)]
pub struct AudioBuffer {
    /// Raw audio samples
    samples: Arc<Mutex<VecDeque<f32>>>,
    /// Maximum buffer capacity
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
        Self {
            samples: Arc::new(Mutex::new(VecDeque::with_capacity(capacity))),
            capacity,
            sample_rate,
            channels,
            last_access: Arc::new(Mutex::new(Instant::now())),
        }
    }

    /// Add a sample to the buffer
    fn push(&self, sample: f32) {
        let mut samples = self.samples.lock().unwrap();
        
        // If buffer is at capacity, remove oldest sample
        if samples.len() >= self.capacity {
            samples.pop_front();
        }
        
        // Add new sample
        samples.push_back(sample);
        
        // Update last access time
        *self.last_access.lock().unwrap() = Instant::now();
    }
    
    /// Get a copy of all samples in the buffer
    pub fn get_samples(&self) -> Vec<f32> {
        let samples = self.samples.lock().unwrap();
        samples.iter().cloned().collect()
    }
    
    /// Get average value of all samples in the buffer
    pub fn get_average(&self) -> f32 {
        let samples = self.samples.lock().unwrap();
        if samples.is_empty() {
            return 0.0;
        }
        
        let sum: f32 = samples.iter().sum();
        sum / samples.len() as f32
    }
    
    /// Get the RMS (root mean square) value of the buffer
    pub fn get_rms(&self) -> f32 {
        let samples = self.samples.lock().unwrap();
        if samples.is_empty() {
            return 0.0;
        }
        
        let sum_squares: f32 = samples.iter().map(|s| s * s).sum();
        (sum_squares / samples.len() as f32).sqrt()
    }
    
    /// Get peak value (maximum absolute value) in the buffer
    pub fn get_peak(&self) -> f32 {
        let samples = self.samples.lock().unwrap();
        samples.iter().map(|s| s.abs()).fold(0.0, f32::max)
    }
    
    /// Clear all samples from the buffer
    pub fn clear(&self) {
        self.samples.lock().unwrap().clear();
        *self.last_access.lock().unwrap() = Instant::now();
    }
    
    /// Get the number of samples in the buffer
    pub fn len(&self) -> usize {
        self.samples.lock().unwrap().len()
    }
    
    /// Check if the buffer is empty
    pub fn is_empty(&self) -> bool {
        self.samples.lock().unwrap().is_empty()
    }
    
    /// Get the buffer duration in seconds
    pub fn duration(&self) -> f32 {
        let len = self.len();
        len as f32 / (self.sample_rate as f32 * self.channels as f32)
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
    pub fn new(device: &Device, buffer_duration_secs: f32) -> Result<Self, String> {
        // Get device name
        let device_name = match device.name() {
            Ok(name) => name,
            Err(_) => return Err("Could not get device name".to_string()),
        };
        
        // Get default config for the device
        let default_config = match device.default_input_config() {
            Ok(config) => config,
            Err(e) => return Err(format!("Failed to get default input config: {}", e)),
        };
        
        // Calculate buffer capacity based on duration
        let sample_rate = default_config.sample_rate().0;
        let channels = default_config.channels();
        let capacity = (buffer_duration_secs * sample_rate as f32 * channels as f32) as usize;
        
        // Create buffer
        let buffer = AudioBuffer::new(capacity, sample_rate, channels);
        
        // Set up the stream and error callback
        let err_fn = |err| eprintln!("Error in audio stream: {}", err);
        
        // Create the stream based on sample format
        let stream = match default_config.sample_format() {
            SampleFormat::F32 => {
                let buffer_clone = buffer.clone();
                device.build_input_stream(
                    &default_config.into(),
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        for &sample in data {
                            buffer_clone.push(sample);
                        }
                    },
                    err_fn,
                    None,
                )
            },
            SampleFormat::I16 => {
                let buffer_clone = buffer.clone();
                device.build_input_stream(
                    &default_config.into(),
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        for &sample in data {
                            buffer_clone.push(sample as f32 / i16::MAX as f32);
                        }
                    },
                    err_fn,
                    None,
                )
            },
            SampleFormat::U16 => {
                let buffer_clone = buffer.clone();
                device.build_input_stream(
                    &default_config.into(),
                    move |data: &[u16], _: &cpal::InputCallbackInfo| {
                        for &sample in data {
                            buffer_clone.push((sample as f32 / u16::MAX as f32) * 2.0 - 1.0);
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

    pub fn get_samples(&self) -> Vec<f32> {
        self.buffer.get_samples()
    }

}

/// Get a device by index
pub fn get_device_by_index(index: usize) -> Option<Device> {
    let host = cpal::default_host();
    let devices = host.input_devices().ok()?;
    let mut devices_vec: Vec<Device> = devices.collect();
    
    if index < devices_vec.len() {
        Some(devices_vec.remove(index))
    } else {
        None
    }
}