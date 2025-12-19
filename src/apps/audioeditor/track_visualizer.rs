use crate::engine::EngineState;

const WAVEFORM_HEIGHT_PERCENT: f32 = 0.15; // Bottom 15% of screen
const WAVEFORM_BACKGROUND_COLOR: (u8, u8, u8) = (32, 32, 32); // Dark gray
const WAVEFORM_COLOR: (u8, u8, u8) = (57, 255, 20); // Neon green (#39ff14)

pub struct TrackVisualizer {
    #[cfg(not(target_arch = "wasm32"))]
    full_audio_samples: Vec<f32>,
}

impl TrackVisualizer {
    pub fn new() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            full_audio_samples: Vec::new(),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_samples(&mut self, samples: Vec<f32>) {
        self.full_audio_samples = samples;
    }

    /// Render the waveform background and waveform at the bottom of the screen
    #[cfg(not(target_arch = "wasm32"))]
    pub fn render(&self, state: &mut EngineState, playback_position: f32) {
        let shape = state.frame.shape();
        let width = shape[1] as u32;
        let height = shape[0] as u32;
        let buffer = state.frame_buffer_mut();

        // Calculate waveform area (bottom 15% of screen)
        let waveform_height = (height as f32 * WAVEFORM_HEIGHT_PERCENT) as u32;
        let waveform_y_start = height - waveform_height;

        // Fill waveform area with background color
        for y in waveform_y_start..height {
            for x in 0..width {
                let idx = ((y * width + x) * 4) as usize;
                if idx + 3 < buffer.len() {
                    buffer[idx + 0] = WAVEFORM_BACKGROUND_COLOR.0;
                    buffer[idx + 1] = WAVEFORM_BACKGROUND_COLOR.1;
                    buffer[idx + 2] = WAVEFORM_BACKGROUND_COLOR.2;
                    buffer[idx + 3] = 0xff;
                }
            }
        }

        if self.full_audio_samples.is_empty() {
            return;
        }

        let waveform_y_center = waveform_y_start + waveform_height / 2;

        // Draw waveform
        let num_samples = self.full_audio_samples.len();
        let samples_per_pixel = (num_samples as f32 / width as f32).max(1.0);
        let amplitude = (waveform_height as f32 * 0.4) as i32; // Use 40% of waveform height for amplitude

        for x in 0..width {
            let sample_start = (x as f32 * samples_per_pixel) as usize;
            let sample_end = ((x + 1) as f32 * samples_per_pixel) as usize;
            
            // Find min/max in this pixel range
            let range_end = sample_end.min(num_samples);
            if sample_start < range_end {
                let min_val = self.full_audio_samples[sample_start..range_end].iter().fold(f32::INFINITY, |a, &b| a.min(b));
                let max_val = self.full_audio_samples[sample_start..range_end].iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));
                
                // Draw vertical line from min to max
                let y_min = (waveform_y_center as i32 - (max_val * amplitude as f32) as i32).max(waveform_y_start as i32);
                let y_max = (waveform_y_center as i32 - (min_val * amplitude as f32) as i32).min((height - 1) as i32);
                
                for y in y_min..=y_max {
                    let idx = ((y as u32 * width + x) * 4) as usize;
                    if idx + 3 < buffer.len() {
                        buffer[idx + 0] = WAVEFORM_COLOR.0;
                        buffer[idx + 1] = WAVEFORM_COLOR.1;
                        buffer[idx + 2] = WAVEFORM_COLOR.2;
                        buffer[idx + 3] = 0xff;
                    }
                }
            }
        }

        // Draw vertical playback position line
        let position_x = (playback_position * width as f32) as u32;
        let line_color = (255, 100, 100); // Red line

        // Draw vertical line from top of waveform area to bottom
        for y in waveform_y_start..height {
            let idx = ((y * width + position_x) * 4) as usize;
            if idx + 3 < buffer.len() {
                buffer[idx + 0] = line_color.0;
                buffer[idx + 1] = line_color.1;
                buffer[idx + 2] = line_color.2;
                buffer[idx + 3] = 0xff;
            }
        }

        // Draw a thicker line (3 pixels wide) for better visibility
        if position_x > 0 && position_x < width - 1 {
            for offset in [-1, 0, 1] {
                let x = (position_x as i32 + offset).max(0).min((width - 1) as i32) as u32;
                for y in waveform_y_start..height {
                    let idx = ((y * width + x) * 4) as usize;
                    if idx + 3 < buffer.len() {
                        buffer[idx + 0] = line_color.0;
                        buffer[idx + 1] = line_color.1;
                        buffer[idx + 2] = line_color.2;
                        buffer[idx + 3] = 0xff;
                    }
                }
            }
        }
    }

    /// Get the waveform area bounds for interaction
    pub fn get_waveform_bounds(&self, _width: f32, height: f32) -> (f32, f32) {
        let waveform_height = height * WAVEFORM_HEIGHT_PERCENT;
        let waveform_y_start = height - waveform_height;
        (waveform_y_start, height)
    }
}

