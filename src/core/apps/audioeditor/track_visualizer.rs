#[cfg(not(target_arch = "wasm32"))]
use crate::engine::EngineState;

const WAVEFORM_HEIGHT_PERCENT: f32 = 0.15; // Bottom 15% of screen
#[cfg(not(target_arch = "wasm32"))]
const WAVEFORM_BACKGROUND_COLOR: (u8, u8, u8) = (32, 32, 32); // Dark gray
#[cfg(not(target_arch = "wasm32"))]
const WAVEFORM_COLOR: (u8, u8, u8) = (57, 255, 20); // Neon green (#39ff14)

pub struct TrackVisualizer {
    #[cfg(not(target_arch = "wasm32"))]
    full_audio_samples: Vec<f32>,
    #[cfg(not(target_arch = "wasm32"))]
    original_sample_count: usize, // Original sample count before downsampling
}

impl TrackVisualizer {
    pub fn new() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            full_audio_samples: Vec::new(),
            #[cfg(not(target_arch = "wasm32"))]
            original_sample_count: 0,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_samples(&mut self, samples: Vec<f32>) {
        self.full_audio_samples = samples;
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_original_sample_count(&mut self, count: usize) {
        self.original_sample_count = count;
    }

    /// Render the waveform background and waveform at the bottom of the screen
    #[cfg(not(target_arch = "wasm32"))]
    pub fn render(&self, state: &mut EngineState, playback_position: f32, zoom_level: f32, zoom_center: f32) {
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

        // Calculate visible range - need to get audio duration from somewhere
        // For now, calculate based on zoom_level and zoom_center
        // This should match calculate_visible_range() in audioedit.rs
        // We'll pass audio_duration as a parameter or calculate it here
        // For rendering purposes, calculate visible range directly
        let visible_range = 1.0 / zoom_level.max(1.0);
        let visible_start = (zoom_center - visible_range / 2.0).max(0.0).min(1.0);
        let visible_end = (zoom_center + visible_range / 2.0).max(0.0).min(1.0);
        let visible_width = visible_end - visible_start;

        // Draw waveform
        let num_samples = self.full_audio_samples.len();
        
        // Find the maximum absolute value in the entire waveform for normalization
        // This ensures we use the full available height
        let max_abs_value = self.full_audio_samples.iter()
            .map(|s| s.abs())
            .fold(0.0f32, |a, b| a.max(b));
        
        // Use full waveform height (minus a small margin) for amplitude
        // Normalize based on the actual peak in the data
        let amplitude_scale = if max_abs_value > 0.0 {
            // Use 90% of waveform height, normalized by the peak value
            (waveform_height as f32 * 0.45) / max_abs_value
        } else {
            waveform_height as f32 * 0.45
        };

        // Use original sample count for accurate position mapping
        // This ensures alignment even if waveform was downsampled
        let effective_sample_count = if self.original_sample_count > 0 {
            self.original_sample_count
        } else {
            num_samples
        };

        // Calculate samples per pixel based on visible range
        let samples_per_pixel = (effective_sample_count as f32 * visible_width / width as f32).max(1.0);

        for x in 0..width {
            // Map screen x coordinate to position in visible range
            let position_in_visible = (x as f32 / width as f32) * visible_width + visible_start;
            let position_in_visible = position_in_visible.max(0.0).min(1.0);
            
            // Map position to sample index in original (non-downsampled) space
            let original_sample_index_f = position_in_visible * effective_sample_count as f32;
            
            // Map back to downsampled index
            let downsample_ratio = if effective_sample_count > 0 && num_samples > 0 {
                num_samples as f32 / effective_sample_count as f32
            } else {
                1.0
            };
            let sample_start_f = original_sample_index_f * downsample_ratio;
            let sample_start = sample_start_f as usize;
            let sample_end = (sample_start_f + samples_per_pixel * downsample_ratio) as usize;
            
            // Find min/max in this pixel range
            let range_end = sample_end.min(num_samples);
            if sample_start < range_end {
                let min_val = self.full_audio_samples[sample_start..range_end].iter().fold(f32::INFINITY, |a, &b| a.min(b));
                let max_val = self.full_audio_samples[sample_start..range_end].iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));
                
                // Draw from min to max to show the full waveform range
                // Scale both values using the normalized amplitude
                let y_min_val = (min_val * amplitude_scale) as i32;
                let y_max_val = (max_val * amplitude_scale) as i32;
                
                // Convert to screen coordinates (center is at waveform_y_center)
                let y_min = (waveform_y_center as i32 + y_min_val).max(waveform_y_start as i32).min((height - 1) as i32);
                let y_max = (waveform_y_center as i32 + y_max_val).max(waveform_y_start as i32).min((height - 1) as i32);
                
                // Draw vertical line from min to max
                let y_start = y_min.min(y_max);
                let y_end = y_min.max(y_max);
                for y in y_start..=y_end {
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
        // Map playback position to screen x coordinate based on visible range
        let position_in_visible = if visible_width > 0.0 {
            ((playback_position - visible_start) / visible_width).max(0.0).min(1.0)
        } else {
            0.5
        };
        let position_x = (position_in_visible * width as f32) as u32;
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

