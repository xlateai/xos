use crate::audio;
use crate::engine::{Application, EngineState};
#[cfg(not(target_os = "ios"))]
use dialoguer::Select;

const NUM_LINES: usize = 512;
const BASELINE_LENGTH: f32 = 0.012;  // 20% of original (0.06)
const MAX_EXTRA_LENGTH: f32 = 0.678; // 1.5x total of original (0.46) minus baseline
const LINE_THICKNESS: f32 = 0.003;
const PROPAGATION_TIME_SECS: f32 = 1.0; // Time for a line to travel from top to bottom
const AMPLIFICATION_FACTOR: f32 = 50.0; // Multiply raw audio by this amount before compression
const SAMPLE_RATE: f32 = 44100.0; // Expected sample rate
const TARGET_FPS: f32 = 60.0; // Target frame rate for smooth animation

pub struct Waveform {
    listener: Option<audio::AudioListener>,
    sample_buffer: Vec<f32>,
    color_buffer: Vec<(u8, u8, u8)>,
    buffer_index: usize,
    lines_to_add: f32, // Fractional lines to add this frame (for smooth 60fps)
}

impl Waveform {
    pub fn new() -> Self {
        Self {
            listener: None,
            sample_buffer: vec![0.0; NUM_LINES],
            color_buffer: vec![(128, 128, 128); NUM_LINES],
            buffer_index: 0,
            lines_to_add: 0.0,
        }
    }
    
    // Non-linear amplification: boosts quiet sounds more than loud ones
    // Multiplies by AMPLIFICATION_FACTOR but compresses the high end with logarithmic decay
    fn amplify_nonlinear(&self, value: f32) -> f32 {
        let abs_val = value.abs();
        
        // Multiply by AMPLIFICATION_FACTOR first
        let boosted = abs_val * AMPLIFICATION_FACTOR;
        
        // Apply logarithmic compression to prevent clipping and show lower volumes better
        // This heavily compresses the top end while preserving low-end dynamics
        let amplified = if boosted < 0.1 {
            // Very quiet - linear boost
            boosted * 2.0
        } else if boosted < 1.0 {
            // Quiet - gentle compression
            0.2 + (boosted - 0.1) * 1.5
        } else {
            // Loud - logarithmic decay to compress the top end
            // This makes the scale: 0-1 maps to 0-1.55, but 10 maps to ~2.8
            // So we can see quiet sounds while loud sounds don't dominate
            0.2 + 1.35 + (boosted - 1.0).ln().max(0.0) * 0.4
        };
        
        if value < 0.0 {
            -amplified
        } else {
            amplified
        }
    }
    
    fn draw_horizontal_line(&self, buffer: &mut [u8], width: u32, height: u32, 
                           y: f32, half_length: f32, color: (u8, u8, u8), thickness: f32) {
        let center_x = width as f32 * 0.5;
        let x0 = (center_x - half_length).max(0.0);
        let x1 = (center_x + half_length).min(width as f32 - 1.0);
        
        let y_start = (y - thickness * 0.5).max(0.0) as u32;
        let y_end = (y + thickness * 0.5).min(height as f32 - 1.0) as u32;
        
        for y_pos in y_start..=y_end {
            for x_pos in x0 as u32..=x1 as u32 {
                let i = (y_pos * width + x_pos) as usize * 4;
                if i + 3 < buffer.len() {
                    buffer[i] = color.0;
                    buffer[i + 1] = color.1;
                    buffer[i + 2] = color.2;
                    buffer[i + 3] = 255;
                }
            }
        }
    }
}

impl Application for Waveform {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        let all_devices = audio::devices();
        
        // Filter to only input devices (microphones)
        let input_devices: Vec<_> = all_devices
            .into_iter()
            .filter(|d| d.is_input)
            .collect();
        
        if input_devices.is_empty() {
            return Err("⚠️ No audio input devices (microphones) found.".to_string());
        }

        // On iOS, skip dialoguer selection and use the first available microphone
        // dialoguer doesn't work on iOS since there's no terminal
        #[cfg(target_os = "ios")]
        {
            let device = input_devices.first().ok_or("No input devices available")?;
            crate::print(&format!("🔊 Attempting to use device: {}", device.name));
            
            let buffer_duration = 1.0;
            match audio::AudioListener::new(device, buffer_duration) {
                Ok(listener) => {
                    match listener.record() {
                        Ok(_) => {
                            crate::print("✅ Audio listener started successfully");
                            self.listener = Some(listener);
                            Ok(())
                        }
                        Err(e) => {
                            Err(format!("Failed to start recording: {}. Make sure microphone permission is granted in Settings.", e))
                        }
                    }
                }
                Err(e) => {
                    Err(format!("Failed to initialize audio listener: {}. On iOS, this usually means microphone permission was denied. Please grant microphone access in Settings > Privacy & Security > Microphone.", e))
                }
            }
        }
        
        // On non-iOS platforms, use dialoguer for device selection
        #[cfg(not(target_os = "ios"))]
        {
            // Create a list of device names for the selector
            let device_names: Vec<String> = input_devices.iter().map(|d| d.name.clone()).collect();

            // Use dialoguer to let user select a microphone
            let selection = Select::new()
                .with_prompt("Select microphone")
                .items(&device_names)
                .default(0)
                .interact()
                .map_err(|e| format!("Failed to get user selection: {}", e))?;

            let device = input_devices.get(selection).ok_or("Selected device not found")?;
            
            crate::print(&format!("🔊 Selected device: {}", device.name));

            let buffer_duration = 1.0;
            let listener = audio::AudioListener::new(device, buffer_duration)?;
            listener.record()?;
            self.listener = Some(listener);
            Ok(())
        }
    }

    fn tick(&mut self, state: &mut EngineState) {
        let Some(listener) = &self.listener else { return };
        let shape = state.frame.tensor.shape();
        let width = shape[1] as u32;
        let height = shape[0] as u32;
        let buffer = state.frame_buffer_mut();

        // Clear background
        for pixel in buffer.chunks_exact_mut(4) {
            pixel[0] = 8;
            pixel[1] = 10;
            pixel[2] = 15;
            pixel[3] = 255;
        }

        let all_samples = listener.get_samples_by_channel();
        if all_samples.is_empty() {
            return;
        }

        let samples = &all_samples[0];
        if samples.is_empty() {
            return;
        }

        // Calculate how many lines to add per frame for smooth 60fps animation
        // At 60fps, we need NUM_LINES / (PROPAGATION_TIME_SECS * 60) lines per frame
        let lines_per_frame = NUM_LINES as f32 / (PROPAGATION_TIME_SECS * TARGET_FPS);
        self.lines_to_add += lines_per_frame;
        
        // Only process whole lines (round down)
        let lines_to_process = self.lines_to_add.floor() as usize;
        self.lines_to_add -= lines_to_process as f32;
        
        // Clamp to max 20 lines per frame to prevent lag spikes
        let lines_to_process = lines_to_process.min(20);
        
        if lines_to_process == 0 {
            return;
        }
        
        // Calculate samples per line
        let samples_per_line = ((SAMPLE_RATE * PROPAGATION_TIME_SECS) / NUM_LINES as f32) as usize;
        let total_samples = samples.len();
        
        // Process only the requested number of lines this frame
        for _ in 0..lines_to_process {
            // Use the most recent samples for RMS calculation
            let window_size = samples_per_line.min(total_samples);
            let start_idx = total_samples.saturating_sub(window_size);
            
            if start_idx >= total_samples {
                break;
            }
            
            // Calculate RMS over this window
            let mut rms_sum = 0.0f32;
            let chunk_samples = &samples[start_idx..total_samples];
            for &sample in chunk_samples {
                rms_sum += sample * sample;
            }
            let rms = (rms_sum / chunk_samples.len() as f32).sqrt();
            
            // Amplify non-linearly
            let amplified = self.amplify_nonlinear(rms);
            let normalized = amplified.clamp(0.0, 1.0);
            
            // Store in circular buffer
            self.sample_buffer[self.buffer_index] = normalized;
            
            // Compute color based on amplitude - white to colors
            let amp = normalized;
            let color = if amp < 0.15 {
                // Very quiet - white/gray
                let brightness = (180.0 + amp / 0.15 * 75.0) as u8;
                (brightness, brightness, brightness)
            } else if amp < 0.4 {
                // Quiet to medium - white to cyan
                let t = (amp - 0.15) / 0.25;
                let r = (255.0 - t * 155.0) as u8;
                let g = 255;
                let b = 255;
                (r, g, b)
            } else if amp < 0.65 {
                // Medium to loud - cyan to green
                let t = (amp - 0.4) / 0.25;
                let r = (100.0 - t * 100.0) as u8;
                let g = 255;
                let b = (255.0 - t * 155.0) as u8;
                (r, g, b)
            } else if amp < 0.85 {
                // Loud - green to yellow
                let t = (amp - 0.65) / 0.2;
                let r = (t * 255.0) as u8;
                let g = 255;
                let b = 0;
                (r, g, b)
            } else {
                // Very loud - yellow to red
                let t = (amp - 0.85) / 0.15;
                let r = 255;
                let g = (255.0 - t * 100.0) as u8;
                let b = 0;
                (r, g, b)
            };
            
            self.color_buffer[self.buffer_index] = color;
            self.buffer_index = (self.buffer_index + 1) % NUM_LINES;
        }

        // Draw flowing horizontal lines (like magnetometer)
        let spacing = height as f32 / NUM_LINES as f32;
        let thickness_px = LINE_THICKNESS * height as f32;
        
        for line_idx in 0..NUM_LINES {
            let buf_idx = (self.buffer_index + line_idx) % NUM_LINES;
            let amp = self.sample_buffer[buf_idx];
            
            // Calculate line length (baseline + extra based on amplitude)
            let half_len = (BASELINE_LENGTH + amp * MAX_EXTRA_LENGTH) * width as f32 * 0.5;
            
            // Y position flows from top to bottom
            let y = height as f32 - (line_idx as f32 * spacing);
            
            let color = self.color_buffer[buf_idx];
            self.draw_horizontal_line(buffer, width, height, y, half_len, color, thickness_px);
        }
    }

    fn on_mouse_down(&mut self, _state: &mut EngineState) {}
    fn on_mouse_up(&mut self, _state: &mut EngineState) {}
    fn on_mouse_move(&mut self, _state: &mut EngineState) {}
}
