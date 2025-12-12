use crate::engine::EngineState;

/// Simple waveform visualizer - draws audio samples as a line graph
pub struct WaveformVisualizer {
    /// Current audio samples to visualize (256 values)
    samples: Vec<f32>,
}

impl WaveformVisualizer {
    pub fn new() -> Self {
        Self {
            samples: vec![0.0; 256],
        }
    }

    /// Update with new audio samples (256 values, typically from audio stream)
    pub fn update_samples(&mut self, samples: &[f32]) {
        // Take up to 256 samples
        let count = samples.len().min(256);
        self.samples[..count].copy_from_slice(&samples[..count]);
        
        // If we have fewer than 256 samples, pad with zeros
        if count < 256 {
            self.samples[count..].fill(0.0);
        }
    }

    /// Render the waveform to the frame buffer
    pub fn tick(&mut self, state: &mut EngineState) {
        self.tick_with_seed(state, 0.0);
    }

    /// Render the waveform to the frame buffer with randomization based on seek position
    pub fn tick_with_seed(&mut self, state: &mut EngineState, seek_position: f32) {
        let width = state.frame.width() as f32;
        let height = state.frame.height() as f32;
        let buffer = state.frame.buffer_mut();

        // Randomize samples based on seek position (more dramatic changes for seeking)
        let seed = (seek_position * 1000000.0) as u32;
        let mut randomized_samples = self.samples.clone();
        
        // Use a simple PRNG based on seed
        let mut rng_state = seed;
        for sample in &mut randomized_samples {
            rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
            let random_value = ((rng_state % 2000) as f32 / 1000.0) - 1.0;
            // More dramatic randomization when seeking (use seek position as primary driver)
            let seek_factor = 0.7; // More influence from seek position
            *sample = (*sample * (1.0 - seek_factor) + random_value * seek_factor).max(-1.0).min(1.0);
        }

        // Draw waveform as a simple line graph
        // Center it vertically, use full width
        let center_y = height / 2.0;
        let amplitude = height * 0.4; // Use 40% of screen height for amplitude
        let line_color = (180, 180, 180); // Light gray

        // Draw line connecting sample points
        let step = width / (randomized_samples.len() as f32 - 1.0);
        
        for i in 0..(randomized_samples.len() - 1) {
            let x0 = (i as f32 * step) as i32;
            let y0 = (center_y - randomized_samples[i] * amplitude) as i32;
            let x1 = ((i + 1) as f32 * step) as i32;
            let y1 = (center_y - randomized_samples[i + 1] * amplitude) as i32;

            // Draw line between points (simple Bresenham-like)
            self.draw_line(buffer, width as u32, height as u32, x0, y0, x1, y1, line_color);
        }
    }

    /// Draw a line between two points
    fn draw_line(
        &self,
        buffer: &mut [u8],
        width: u32,
        height: u32,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        color: (u8, u8, u8),
    ) {
        let dx = (x1 - x0).abs();
        let dy = (y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx - dy;
        let mut x = x0;
        let mut y = y0;

        // Thin line - 1 pixel width
        while x != x1 || y != y1 {
            if x >= 0 && y >= 0 && (x as u32) < width && (y as u32) < height {
                let idx = ((y as u32 * width + x as u32) * 4) as usize;
                if idx + 3 < buffer.len() {
                    buffer[idx + 0] = color.0;
                    buffer[idx + 1] = color.1;
                    buffer[idx + 2] = color.2;
                    buffer[idx + 3] = 0xff;
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

        // Draw the end point
        if x1 >= 0 && y1 >= 0 && (x1 as u32) < width && (y1 as u32) < height {
            let idx = ((y1 as u32 * width + x1 as u32) * 4) as usize;
            if idx + 3 < buffer.len() {
                buffer[idx + 0] = color.0;
                buffer[idx + 1] = color.1;
                buffer[idx + 2] = color.2;
                buffer[idx + 3] = 0xff;
            }
        }
    }
}
