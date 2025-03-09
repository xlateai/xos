use cpal::{
    traits::DeviceTrait,
    Device as CpalDevice, StreamConfig,
};
use std::sync::{Arc, Mutex};

use super::buffer::MultiChannelBuffer;

pub(crate) struct AudioDevice {
    name: String,
    is_output: bool,
    buffer: Arc<Mutex<MultiChannelBuffer>>,
    sample_rate: u32,
    cpal_device: CpalDevice,
    active: bool,
}

impl AudioDevice {
    pub(crate) fn new(cpal_device: CpalDevice, buffer_size: usize) -> Result<Self, String> {
        let is_output = cpal_device.default_output_config().is_ok();
        
        let config = if is_output {
            cpal_device.default_output_config()
        } else {
            cpal_device.default_input_config()
        }.map_err(|e| format!("Error getting device config: {}", e))?;
        
        let sample_rate = config.sample_rate().0;
        let channel_count = config.channels() as usize;
        
        let name = cpal_device
            .name()
            .unwrap_or_else(|_| String::from("Unknown Device"));
            
        let buffer = Arc::new(Mutex::new(
            MultiChannelBuffer::new(channel_count, buffer_size)
        ));
        
        Ok(Self {
            name,
            is_output,
            buffer,
            sample_rate,
            cpal_device,
            active: false,
        })
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn is_output(&self) -> bool {
        self.is_output
    }

    pub(crate) fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub(crate) fn channel_count(&self) -> usize {
        self.buffer.lock().unwrap().channel_count()
    }

    pub(crate) fn get_samples(&self) -> Vec<Vec<f32>> {
        self.buffer.lock().unwrap().get_samples()
    }

    pub(crate) fn start(&mut self) -> Result<(), String> {
        if self.active {
            return Ok(());
        }

        let buffer_clone = Arc::clone(&self.buffer);
        let err_fn = move |err| {
            eprintln!("Error in audio stream: {}", err);
        };

        if self.is_output {
            self.start_output_stream(buffer_clone, err_fn)?;
        } else {
            self.start_input_stream(buffer_clone, err_fn)?;
        }

        self.active = true;
        Ok(())
    }

    pub(crate) fn stop(&mut self) {
        // Implementation would depend on how we manage stream handles
        self.active = false;
    }

    fn start_input_stream<E>(
        &self, 
        buffer: Arc<Mutex<MultiChannelBuffer>>, 
        err_fn: E
    ) -> Result<(), String> 
    where 
        E: FnMut(cpal::StreamError) + Send + 'static,
    {
        let config = StreamConfig {
            channels: self.channel_count() as u16,
            sample_rate: cpal::SampleRate(self.sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let stream = self.cpal_device(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if let Ok(mut buffer) = buffer.lock() {
                    buffer.push_interleaved(data);
                }
            },
            err_fn,
            None,
        ).map_err(|e| format!("Error building input stream: {}", e))?;

        // Start the stream
        stream.play().map_err(|e| format!("Error starting input stream: {}", e))?;

        // Store the stream somewhere (you'll need to modify the struct to hold this)
        // For now, we'll just forget it which keeps it alive but we can't stop it later
        std::mem::forget(stream);

        Ok(())
    }

    fn start_output_stream<E>(
        &self, 
        buffer: Arc<Mutex<MultiChannelBuffer>>, 
        err_fn: E
    ) -> Result<(), String> 
    where 
        E: FnMut(cpal::StreamError) + Send + 'static,
    {
        let config = StreamConfig {
            channels: self.channel_count() as u16,
            sample_rate: cpal::SampleRate(self.sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        // For output devices, we might want to capture the audio that's being played
        // This is more complex and might require hooking into the audio system at a lower level
        // For simplicity, we'll just create a stream that captures silence for now
        
        let stream = self.cpal_device.build_output_stream(
            &config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                // Fill output buffer with silence for now
                for sample in data.iter_mut() {
                    *sample = 0.0;
                }
                
                // In a real implementation, we might copy data from our buffer to the output
                // and/or capture what's being sent to the output
                if let Ok(buffer) = buffer.lock() {
                    // Just reading samples here, not modifying the buffer
                    let _samples = buffer.get_samples();
                }
            },
            err_fn,
            None,
        ).map_err(|e| format!("Error building output stream: {}", e))?;

        // Start the stream
        stream.play().map_err(|e| format!("Error starting output stream: {}", e))?;

        // Store the stream somewhere
        std::mem::forget(stream);

        Ok(())
    }
}