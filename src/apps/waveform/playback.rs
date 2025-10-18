use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Device, SampleFormat, Stream, SupportedStreamConfig,
};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub struct Playback {
    pub output_stream: Option<Stream>,
    pub playback_buffer: Arc<Mutex<VecDeque<f32>>>,
    output_device: Device,
    output_config: SupportedStreamConfig,
    pub max_buffer_size: usize,
}

impl Playback {
    pub fn new(max_buffer_size: usize) -> Result<Self, String> {
        let host = cpal::default_host();
        let output_device = host
            .output_devices()
            .map_err(|e| format!("Failed to get output devices: {}", e))?
            .find(|d| d.name().unwrap_or_default().contains("AirPods"))
            .or_else(|| host.default_output_device())
            .ok_or("No default output device found")?;

        println!(
            "🎧 Using output device: {}",
            output_device.name().unwrap_or_default()
        );

        let output_config = output_device
            .supported_output_configs()
            .map_err(|e| format!("Error getting output configs: {}", e))?
            .find(|c| c.sample_format() == SampleFormat::F32)
            .ok_or("No supported F32 output config found")?
            .with_max_sample_rate();

        Ok(Self {
            output_stream: None,
            playback_buffer: Arc::new(Mutex::new(VecDeque::with_capacity(max_buffer_size))),
            output_device,
            output_config,
            max_buffer_size,
        })
    }

    pub fn start(&mut self) -> Result<(), String> {
        let playback_buffer = Arc::clone(&self.playback_buffer);
        let channels = self.output_config.channels() as usize;

        let stream = self.output_device.build_output_stream(
            &self.output_config.config(),
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let mut buffer = playback_buffer.lock().unwrap();
                for chunk in data.chunks_mut(channels) {
                    if let Some(sample) = buffer.pop_front() {
                        for channel_data in chunk.iter_mut() {
                            *channel_data = sample;
                        }
                    } else {
                        for channel_data in chunk.iter_mut() {
                            *channel_data = 0.0;
                        }
                    }
                }
            },
            |err| eprintln!("Output stream error: {}", err),
            None,
        ).map_err(|e| format!("Failed to build output stream: {}", e))?;

        stream
            .play()
            .map_err(|e| format!("Failed to start output stream: {}", e))?;
        self.output_stream = Some(stream);
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(stream) = self.output_stream.take() {
            let _ = stream.pause();
        }
        self.playback_buffer.lock().unwrap().clear();
    }

    pub fn feed(&self, samples: &[f32]) {
        let mut buffer = self.playback_buffer.lock().unwrap();
        for &sample in samples {
            if buffer.len() < self.max_buffer_size {
                buffer.push_back(sample);
            } else {
                buffer.pop_front();
                buffer.push_back(sample);
            }
        }
    }
}
