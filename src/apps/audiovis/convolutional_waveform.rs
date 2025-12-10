use crate::engine::EngineState;
use crate::tensor::{Array, depthwise_conv2d};

const CHANNELS: usize = 3;
const KERNEL_SIZE: usize = 3;

// Kernel normalization range - tune these to adjust kernel value distribution
const KERNEL_MIN: f32 = -0.75;
const KERNEL_MAX: f32 = 1.0;

/// Convolutional waveform visualizer - uses audio to drive a convolutional filter
pub struct ConvolutionalWaveform {
    /// Image buffer (width x height pixels, each pixel has RGB channels)
    /// Stored as [R, G, B, R, G, B, ...] for each pixel
    image: Vec<f32>,
    /// Image dimensions
    width: u32,
    height: u32,
    /// Current convolution kernel (3x3x3 for RGB)
    kernel: Vec<f32>,
    /// Previous kernel for temporal smoothing
    previous_kernel: Vec<f32>,
}

impl ConvolutionalWaveform {
    pub fn new(width: u32, height: u32) -> Self {
        // Initialize random colored image mask (RGB channels)
        let mut image = Vec::with_capacity(width as usize * height as usize * CHANNELS);
        
        #[cfg(not(target_arch = "wasm32"))]
        {
            use rand::Rng;
            let mut rng = rand::rng();
            for _ in 0..(width * height) {
                image.push(rng.random::<f32>()); // R
                image.push(rng.random::<f32>()); // G
                image.push(rng.random::<f32>()); // B
            }
        }
        
        #[cfg(target_arch = "wasm32")]
        {
            let mut seed = 12345u32;
            for _ in 0..(width * height) {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                image.push((seed % 1000) as f32 / 1000.0); // R
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                image.push((seed % 1000) as f32 / 1000.0); // G
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                image.push((seed % 1000) as f32 / 1000.0); // B
            }
        }

        // Generate random 3x3x3 convolution kernel
        let kernel = Self::generate_random_kernel();
        let previous_kernel = kernel.clone(); // Initialize previous kernel to same as current

        Self {
            image,
            width,
            height,
            kernel,
            previous_kernel,
        }
    }

    /// Generate a random 3x3x3 convolution kernel
    fn generate_random_kernel() -> Vec<f32> {
        let mut kernel = Vec::with_capacity(KERNEL_SIZE * KERNEL_SIZE * CHANNELS);
        
        #[cfg(not(target_arch = "wasm32"))]
        {
            use rand::Rng;
            let mut rng = rand::rng();
            for _ in 0..(KERNEL_SIZE * KERNEL_SIZE * CHANNELS) {
                // Random values between -1.0 and 1.0
                kernel.push(rng.random_range(-1.0..=1.0));
            }
        }
        
        #[cfg(target_arch = "wasm32")]
        {
            let mut seed = 67890u32;
            for _ in 0..(KERNEL_SIZE * KERNEL_SIZE * CHANNELS) {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                kernel.push(((seed % 2000) as f32 / 1000.0) - 1.0);
            }
        }
        
        kernel
    }

    /// Convert image from [H, W, C] to [C, H, W] format
    fn image_to_chw(&self, hwc: &[f32]) -> Vec<f32> {
        let h = self.height as usize;
        let w = self.width as usize;
        let mut chw = Vec::with_capacity(hwc.len());
        
        for c in 0..CHANNELS {
            for y in 0..h {
                for x in 0..w {
                    let src_idx = (y * w + x) * CHANNELS + c;
                    chw.push(hwc[src_idx]);
                }
            }
        }
        chw
    }

    /// Convert image from [C, H, W] back to [H, W, C] format
    fn image_from_chw(&self, chw: &[f32], h: usize, w: usize) -> Vec<f32> {
        let mut hwc = Vec::with_capacity(chw.len());
        
        for y in 0..h {
            for x in 0..w {
                for c in 0..CHANNELS {
                    let src_idx = c * h * w + y * w + x;
                    hwc.push(chw[src_idx]);
                }
            }
        }
        hwc
    }

    /// Convert kernel from [H, W, C] to [C, H, W] format for depthwise convolution
    fn kernel_to_chw(&self, hwc: &[f32]) -> Vec<f32> {
        let mut chw = Vec::with_capacity(CHANNELS * KERNEL_SIZE * KERNEL_SIZE);
        
        for c in 0..CHANNELS {
            for ky in 0..KERNEL_SIZE {
                for kx in 0..KERNEL_SIZE {
                    let src_idx = (ky * KERNEL_SIZE + kx) * CHANNELS + c;
                    chw.push(hwc[src_idx]);
                }
            }
        }
        chw
    }

    /// Add barely-visible noise to the image to keep it alive
    fn add_noise(&mut self) {
        const NOISE_AMOUNT: f32 = 0.01; // Small amount of noise (±1% in normalized range)
        
        #[cfg(not(target_arch = "wasm32"))]
        {
            use rand::Rng;
            let mut rng = rand::rng();
            for pixel_value in self.image.iter_mut() {
                // Add small random noise: ±NOISE_AMOUNT
                let noise = rng.random_range(-NOISE_AMOUNT..=NOISE_AMOUNT);
                *pixel_value = (*pixel_value + noise).clamp(0.0, 1.0);
            }
        }
        
        #[cfg(target_arch = "wasm32")]
        {
            // WASM: use simple pseudo-random
            let mut seed = 12345u32;
            for pixel_value in self.image.iter_mut() {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                // Generate noise in range [-NOISE_AMOUNT, NOISE_AMOUNT]
                let noise = ((seed % 2000) as f32 / 1000.0 - 1.0) * NOISE_AMOUNT;
                *pixel_value = (*pixel_value + noise).clamp(0.0, 1.0);
            }
        }
    }

    /// Apply convolution using custom tensor library
    fn apply_convolution(&mut self) {
        // Add barely-visible noise before convolution to keep image alive
        self.add_noise();
        
        let h = self.height as usize;
        let w = self.width as usize;

        // Convert image from [H, W, C] to [C, H, W]
        let image_chw = self.image_to_chw(&self.image);
        let image_array = Array::new(image_chw, vec![CHANNELS, h, w]);

        // Convert kernel from [H, W, C] to [C, H, W] for depthwise convolution
        let kernel_chw = self.kernel_to_chw(&self.kernel);
        let kernel_array = Array::new(kernel_chw, vec![CHANNELS, KERNEL_SIZE, KERNEL_SIZE]);

        // Apply depthwise convolution with same padding (padding=1 for 3x3 kernel, stride=1)
        let output_array = depthwise_conv2d(&image_array, &kernel_array, (1, 1), (1, 1));

        // Output is [1, C, H, W], but since batch=1, the data is effectively [C, H, W]
        // The data layout is already [C, H, W] (batch dimension is just 1)
        let output_shape = output_array.shape();
        let output_data = output_array.data();
        
        // Extract output dimensions (should match input with same padding)
        let out_h = output_shape[2];
        let out_w = output_shape[3];
        
        // Convert from [C, H, W] back to [H, W, C]
        let mut image_data = self.image_from_chw(output_data, out_h, out_w);
        
        // Clamp pixel values to ensure they never go below minimum threshold
        // This prevents the screen from going completely black
        const MIN_VALUE: f32 = 1.0 / 255.0; // Minimum value (1 in 0-255 range, normalized)
        const MAX_VALUE: f32 = 1.0;
        
        for pixel_value in image_data.iter_mut() {
            *pixel_value = pixel_value.max(MIN_VALUE).min(MAX_VALUE);
        }
        
        self.image = image_data;
        
        // Update dimensions (should be same with padding=1, but update to be safe)
        self.height = out_h as u32;
        self.width = out_w as u32;
    }

    /// Update kernel from audio samples using de-interleaving approach
    /// Distributes all audio samples across 21 kernel cells in round-robin fashion,
    /// then averages each cell. This makes the kernel more informationally dense.
    fn update_kernel_from_audio(&mut self, samples: &[f32]) {
        const KERNEL_CELLS: usize = 21; // Number of cells to fill from audio
        const KERNEL_LEN: usize = KERNEL_SIZE * KERNEL_SIZE * CHANNELS; // 27 total
        
        if samples.is_empty() {
            // If no samples, keep existing kernel
            return;
        }
        
        // Initialize bins: sums and counts for each of the 21 cells
        let mut cell_sums = vec![0.0f32; KERNEL_CELLS];
        let mut cell_counts = vec![0usize; KERNEL_CELLS];
        
        // Distribute all samples across the 21 cells using modulus (round-robin)
        for (idx, &sample) in samples.iter().enumerate() {
            let cell_idx = idx % KERNEL_CELLS;
            cell_sums[cell_idx] += sample;
            cell_counts[cell_idx] += 1;
        }
        
        // Calculate averages for each cell
        let mut cell_averages = Vec::with_capacity(KERNEL_CELLS);
        for i in 0..KERNEL_CELLS {
            if cell_counts[i] > 0 {
                cell_averages.push(cell_sums[i] / cell_counts[i] as f32);
            } else {
                cell_averages.push(0.0);
            }
        }
        
        // Normalize the averages to [KERNEL_MIN, KERNEL_MAX] range
        let min_val = cell_averages.iter().copied().fold(f32::INFINITY, f32::min);
        let max_val = cell_averages.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        
        let normalized_cells: Vec<f32> = if (max_val - min_val).abs() > f32::EPSILON {
            // Normal case: scale from [min, max] to [KERNEL_MIN, KERNEL_MAX]
            let range_scale = KERNEL_MAX - KERNEL_MIN;
            cell_averages.iter().map(|&val| {
                KERNEL_MIN + (val - min_val) / (max_val - min_val) * range_scale
            }).collect()
        } else {
            // Edge case: all values are the same, set to middle of [KERNEL_MIN, KERNEL_MAX] range
            let mid_value = (KERNEL_MIN + KERNEL_MAX) / 2.0;
            vec![mid_value; KERNEL_CELLS]
        };
        
        // Build new kernel from normalized cell averages
        let mut new_kernel = Vec::with_capacity(KERNEL_LEN);
        
        // Fill kernel with the 21 normalized cell averages
        for &value in normalized_cells.iter() {
            new_kernel.push(value);
        }
        
        // Pad remaining positions with zeros (27 - 21 = 6)
        while new_kernel.len() < KERNEL_LEN {
            new_kernel.push(0.0);
        }
        
        // Blend new kernel with previous kernel for smooth temporal transition
        // Average between previous and new (50/50 blend for smooth transition)
        const BLEND_FACTOR: f32 = 0.5; // 0.0 = no change, 1.0 = instant change
        
        // Ensure previous_kernel is the right size
        if self.previous_kernel.len() != KERNEL_LEN {
            self.previous_kernel = new_kernel.clone();
        }
        
        // Blend: new_kernel = previous * (1 - blend) + new * blend
        for i in 0..KERNEL_LEN {
            self.kernel[i] = self.previous_kernel[i] * (1.0 - BLEND_FACTOR) + new_kernel[i] * BLEND_FACTOR;
        }
        
        // Update previous kernel for next frame
        self.previous_kernel = self.kernel.clone();
    }

    /// Update with new audio samples - updates kernel and applies convolution
    pub fn update_samples(&mut self, samples: &[f32]) {
        // Update kernel from audio stream
        self.update_kernel_from_audio(samples);
        // Apply convolution with the new kernel
        self.apply_convolution();
    }

    /// Render the convolved image to the frame buffer with perfect square pixels
    /// Applies convolution each frame for dynamic visualization
    pub fn tick(&mut self, state: &mut EngineState) {
        // Apply convolution each frame
        self.apply_convolution();
        let buffer = &mut state.frame.buffer;
        let width = state.frame.width;
        let height = state.frame.height;

        // Calculate pixel size to maintain square pixels
        // Use the smaller dimension to determine pixel size
        let pixel_size = (width.min(height) / self.width.min(self.height)) as u32;
        let pixel_size = pixel_size.max(1); // At least 1 pixel
        
        // Calculate the actual rendered size (may be smaller than screen)
        let rendered_width = self.width * pixel_size;
        let rendered_height = self.height * pixel_size;
        
        // Center the image on screen
        let offset_x = (width.saturating_sub(rendered_width)) / 2;
        let offset_y = (height.saturating_sub(rendered_height)) / 2;

        // Render each pixel as a perfect square
        for iy in 0..self.height {
            for ix in 0..self.width {
                let pixel_idx = (iy * self.width + ix) as usize;
                
                if pixel_idx < (self.width * self.height) as usize {
                    // Get RGB values from image buffer
                    let r_idx = pixel_idx * 3;
                    let g_idx = pixel_idx * 3 + 1;
                    let b_idx = pixel_idx * 3 + 2;
                    
                    if r_idx < self.image.len() && g_idx < self.image.len() && b_idx < self.image.len() {
                        let r = (self.image[r_idx] * 255.0) as u8;
                        let g = (self.image[g_idx] * 255.0) as u8;
                        let b = (self.image[b_idx] * 255.0) as u8;
                        
                        // Draw this pixel as a square
                        let screen_x_start = offset_x + (ix * pixel_size);
                        let screen_y_start = offset_y + (iy * pixel_size);
                        
                        for dy in 0..pixel_size {
                            for dx in 0..pixel_size {
                                let px = screen_x_start + dx;
                                let py = screen_y_start + dy;
                                
                                if px < width && py < height {
                                    let idx = ((py * width + px) * 4) as usize;
                                    if idx + 3 < buffer.len() {
                                        buffer[idx + 0] = r;
                                        buffer[idx + 1] = g;
                                        buffer[idx + 2] = b;
                                        buffer[idx + 3] = 0xff;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
