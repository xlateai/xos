use crate::audio;
use crate::engine::{Application, EngineState};
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::Stream;
use std::sync::{Arc, Mutex};
use std::collections::VecDeque;

pub struct Waveform {
    listener: Option<audio::AudioListener>,
    playback_enabled: bool,
    output_stream: Option<Stream>,
    playback_buffer: Arc<Mutex<VecDeque<f32>>>,
}

impl Waveform {
    pub fn new() -> Self {
        Self {
            listener: None,
            playback_enabled: false,
            output_stream: None,
            playback_buffer: Arc::new(Mutex::new(VecDeque::with_capacity(4096))),
        }
    }

    fn draw_toggle_button(&self, state: &mut EngineState) {
        const BUTTON_SIZE: f32 = 40.0;
        const MARGIN: f32 = 20.0;
        
        let buffer = &mut state.frame.buffer;
        let width = state.frame.width as f32;
        let height = state.frame.height as f32;

        // Position in bottom right corner
        let x_center = width - MARGIN - BUTTON_SIZE / 2.0;
        let y_center = height - MARGIN - BUTTON_SIZE / 2.0;

        let x0 = (x_center - BUTTON_SIZE / 2.0) as usize;
        let x1 = (x_center + BUTTON_SIZE / 2.0) as usize;
        let y0 = (y_center - BUTTON_SIZE / 2.0) as usize;
        let y1 = (y_center + BUTTON_SIZE / 2.0) as usize;

        // Choose color based on toggle state
        let (r, g, b) = if self.playback_enabled {
            (0, 200, 0) // Green when enabled
        } else {
            (60, 60, 60) // Light gray when disabled
        };

        for y in y0..y1.min(state.frame.height as usize) {
            for x in x0..x1.min(state.frame.width as usize) {
                let i = (y * state.frame.width as usize + x) * 4;
                if i + 3 < buffer.len() {
                    buffer[i] = r;
                    buffer[i + 1] = g;
                    buffer[i + 2] = b;
                    buffer[i + 3] = 255;
                }
            }
        }
    }

    fn is_inside_toggle_button(&self, mouse_x: f32, mouse_y: f32, state: &EngineState) -> bool {
        const BUTTON_SIZE: f32 = 40.0;
        const MARGIN: f32 = 20.0;
        
        let width = state.frame.width as f32;
        let height = state.frame.height as f32;

        let x_center = width - MARGIN - BUTTON_SIZE / 2.0;
        let y_center = height - MARGIN - BUTTON_SIZE / 2.0;

        let half_size = BUTTON_SIZE / 2.0;
        
        mouse_x >= x_center - half_size &&
        mouse_x <= x_center + half_size &&
        mouse_y >= y_center - half_size &&
        mouse_y <= y_center + half_size
    }

    fn setup_output_stream(&mut self) -> Result<(), String> {
        let devices = audio::devices();
        if devices.len() < 3 {
            return Err("Not enough audio devices found (need at least 3)".to_string());
        }

        // Hardcode index 2 for speakers playback as requested
        let output_device = &devices[2];
        if !output_device.is_output {
            return Err("Device at index 2 is not an output device".to_string());
        }

        let device = &output_device.device_cpal;
        let config = device.default_output_config()
            .map_err(|e| format!("Failed to get output config: {}", e))?;

        let playback_buffer = Arc::clone(&self.playback_buffer);
        let channels = config.channels() as usize;

        let stream = device.build_output_stream(
            &config.into(),
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let mut buffer = playback_buffer.lock().unwrap();
                
                // Fill output with samples from buffer, interleaved for all channels
                for chunk in data.chunks_mut(channels) {
                    if let Some(sample) = buffer.pop_front() {
                        // Duplicate the mono sample across all channels
                        for channel_data in chunk.iter_mut() {
                            *channel_data = sample;
                        }
                    } else {
                        // Fill with silence if no data available
                        for channel_data in chunk.iter_mut() {
                            *channel_data = 0.0;
                        }
                    }
                }
            },
            |err| eprintln!("Output stream error: {}", err),
            None,
        ).map_err(|e| format!("Failed to build output stream: {}", e))?;

        stream.play().map_err(|e| format!("Failed to start output stream: {}", e))?;
        self.output_stream = Some(stream);
        Ok(())
    }

    fn stop_output_stream(&mut self) {
        if let Some(stream) = self.output_stream.take() {
            let _ = stream.pause();
        }
        // Clear the buffer
        self.playback_buffer.lock().unwrap().clear();
    }

    fn feed_playback_buffer(&self, samples: &[f32]) {
        if self.playback_enabled {
            let mut buffer = self.playback_buffer.lock().unwrap();
            
            // Add samples to playback buffer, but limit buffer size to prevent latency buildup
            const MAX_BUFFER_SIZE: usize = 2048; // Keep buffer small for low latency
            
            for &sample in samples {
                if buffer.len() < MAX_BUFFER_SIZE {
                    buffer.push_back(sample);
                } else {
                    // If buffer is full, remove oldest sample and add new one
                    buffer.pop_front();
                    buffer.push_back(sample);
                }
            }
        }
    }
}

fn draw_line(
    buffer: &mut [u8],
    width: u32,
    height: u32,
    x0: isize,
    y0: isize,
    x1: isize,
    y1: isize,
    thickness: usize,
) {
    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx - dy;
    let mut x = x0;
    let mut y = y0;

    while x != x1 || y != y1 {
        for tx in 0..thickness {
            for ty in 0..thickness {
                let px = x + tx as isize;
                let py = y + ty as isize;
                if px >= 0 && py >= 0 && (px as u32) < width && (py as u32) < height {
                    let i = (py as usize * width as usize + px as usize) * 4;
                    buffer[i] = 0;
                    buffer[i + 1] = 255;
                    buffer[i + 2] = 0;
                    buffer[i + 3] = 255;
                }
            }
        }

        let e2 = 2 * err;
        if e2 > -dy {
            err -= dy;
            x += sx;
        }
        if e2 < dx {
            err += dx;
            y += sy;
        }
    }
}

impl Application for Waveform {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        let devices = audio::devices();
        if devices.is_empty() {
            return Err("⚠️ No audio input devices found.".to_string());
        }

        println!("🔊 Available devices:");
        for (i, d) in devices.iter().enumerate() {
            println!("  [{}] {}", i, d.name);
        }

        let device_index = 0;
        let device = devices.get(device_index).ok_or("No audio device found")?;

        let buffer_duration = 1.0;
        let listener = audio::AudioListener::new(device, buffer_duration)?;
        listener.record()?;
        self.listener = Some(listener);
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        let Some(listener) = &self.listener else { return };
        let buffer = &mut state.frame.buffer;
        let width = state.frame.width;
        let height = state.frame.height;

        for pixel in buffer.chunks_exact_mut(4) {
            pixel[0] = 16;
            pixel[1] = 16;
            pixel[2] = 24;
            pixel[3] = 255;
        }

        let all_samples = listener.get_samples_by_channel();
        if all_samples.is_empty() {
            return;
        }

        let samples = &all_samples[0];
        
        // Feed samples to playback buffer if playback is enabled
        self.feed_playback_buffer(samples);
        let vertical = height > width;

        let (len, scale, center) = if vertical {
            (height, width as f32 * 0.5 * 0.8, width as f32 * 0.5)
        } else {
            (width, height as f32 * 0.5 * 0.8, height as f32 * 0.5)
        };

        let step = samples.len().max(1) as f32 / len as f32;
        let stride = 2;
        let thickness = 2;

        let mut prev = None;

        for i in (0..len).step_by(stride as usize) {
            let sample_index = (i as f32 * step) as usize;
            if sample_index >= samples.len() {
                break;
            }

            let offset = samples[sample_index] * scale;
            let (x, y) = if vertical {
                ((center + offset) as isize, i as isize)
            } else {
                (i as isize, (center - offset) as isize)
            };

            if let Some((prev_x, prev_y)) = prev {
                draw_line(buffer, width, height, prev_x, prev_y, x, y, thickness);
            }

            prev = Some((x, y));
        }

        // Draw the toggle button
        self.draw_toggle_button(state);
    }

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        if self.is_inside_toggle_button(state.mouse.x, state.mouse.y, state) {
            let was_enabled = self.playback_enabled;
            self.playback_enabled = !self.playback_enabled;
            
            if self.playback_enabled && !was_enabled {
                // Enable playback - setup output stream
                if let Err(e) = self.setup_output_stream() {
                    eprintln!("Failed to setup audio playback: {}", e);
                    self.playback_enabled = false;
                }
            } else if !self.playback_enabled && was_enabled {
                // Disable playback - stop output stream
                self.stop_output_stream();
            }
        }
    }
    
    fn on_mouse_up(&mut self, _state: &mut EngineState) {}
    fn on_mouse_move(&mut self, _state: &mut EngineState) {}
}
