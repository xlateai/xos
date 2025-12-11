use crate::engine::EngineState;
use crate::tensor::conv::{ConvParams, ConvBackend};
#[cfg(any(target_os = "macos", target_os = "ios"))]
use crate::tensor::conv::metal::MetalBackend;
use crate::tensor::conv::cpu::CpuBackend;
use once_cell::sync::Lazy;

const CHANNELS: usize = 3;
const KERNEL_SIZE: usize = 3;

// Image resolution - tune these to adjust the visualization size
const IMAGE_WIDTH: u32 = 2000;
const IMAGE_HEIGHT: u32 = 1000;

// Kernel normalization range - tune these to adjust kernel value distribution
const KERNEL_MIN: f32 = -1.0;
const KERNEL_MAX: f32 = 1.0;

// Steps per kernel update - only recalculate kernel every kth step
const STEPS_PER_KERNEL: u32 = 1;

// Global backend - easily swap between CPU and Metal for performance testing
// Uncomment the one you want to use:
#[cfg(any(target_os = "macos", target_os = "ios"))]
static CONV_BACKEND: Lazy<Box<dyn ConvBackend + Send + Sync>> = Lazy::new(|| {
    // Metal backend (GPU-accelerated, default)
    Box::new(MetalBackend::new())
    
    // CPU backend (for comparison/testing)
    // Box::new(CpuBackend::new())
});

// Fallback to CPU backend on non-Apple platforms
#[cfg(not(any(target_os = "macos", target_os = "ios")))]
static CONV_BACKEND: Lazy<Box<dyn ConvBackend + Send + Sync>> = Lazy::new(|| {
    Box::new(CpuBackend::new())
});

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
    /// Last seek position used for randomization
    last_seek_position: f32,
    /// Step counter for kernel update throttling
    step_counter: u32,
}

impl ConvolutionalWaveform {
    pub fn new() -> Self {
        // Initialize random colored image mask (RGB channels)
        let mut image = Vec::with_capacity(IMAGE_WIDTH as usize * IMAGE_HEIGHT as usize * CHANNELS);
        
        #[cfg(not(target_arch = "wasm32"))]
        {
            use rand::Rng;
            let mut rng = rand::rng();
            for _ in 0..(IMAGE_WIDTH * IMAGE_HEIGHT) {
                image.push(rng.random::<f32>()); // R
                image.push(rng.random::<f32>()); // G
                image.push(rng.random::<f32>()); // B
            }
        }
        
        #[cfg(target_arch = "wasm32")]
        {
            let mut seed = 12345u32;
            for _ in 0..(IMAGE_WIDTH * IMAGE_HEIGHT) {
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
            width: IMAGE_WIDTH,
            height: IMAGE_HEIGHT,
            kernel,
            previous_kernel,
            last_seek_position: -1.0,
            step_counter: 0,
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

    /// Check if the image has "died" (become too uniform or too low)
    /// Returns true if the image should be reinitialized
    fn is_image_dead(&self) -> bool {
        if self.image.is_empty() {
            return true;
        }
        
        // Calculate variance to detect if image is too uniform
        let mean: f32 = self.image.iter().sum::<f32>() / self.image.len() as f32;
        let variance: f32 = self.image.iter()
            .map(|&x| (x - mean) * (x - mean))
            .sum::<f32>() / self.image.len() as f32;
        
        // Image is "dead" if variance is very low (too uniform) or mean is very low (too dark)
        const MIN_VARIANCE: f32 = 0.0001; // Very low variance threshold
        const MIN_MEAN: f32 = 0.05; // Very low mean threshold
        
        variance < MIN_VARIANCE || mean < MIN_MEAN
    }

    /// Reinitialize the image with random values
    fn reinitialize_image(&mut self) {
        self.reinitialize_image_with_seed(12345u32);
    }

    /// Reinitialize the image with random values based on a seed
    fn reinitialize_image_with_seed(&mut self, seed: u32) {
        let width = self.width as usize;
        let height = self.height as usize;
        let total_pixels = width * height;
        
        self.image.clear();
        self.image.reserve(total_pixels * CHANNELS);
        
        let mut rng_state = seed;
        for _ in 0..total_pixels {
            rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
            self.image.push((rng_state % 1000) as f32 / 1000.0); // R
            rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
            self.image.push((rng_state % 1000) as f32 / 1000.0); // G
            rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
            self.image.push((rng_state % 1000) as f32 / 1000.0); // B
        }
    }

    /// Apply convolution using Metal backend
    fn apply_convolution(&mut self) {
        // Check if image has died and reinitialize if needed
        if self.is_image_dead() {
            self.reinitialize_image();
        }
        
        let h = self.height as u32;
        let w = self.width as u32;

        // Convert image from [H, W, C] to [C, H, W] (NCHW format: [batch, channels, height, width])
        let image_chw = self.image_to_chw(&self.image);
        // Input format: [batch=1, channels=3, height, width]
        let input = image_chw;

        // Convert kernel from [H, W, C] to [C, H, W] for depthwise convolution
        // Depthwise kernel format: [channels, kernel_h, kernel_w]
        let kernel_chw = self.kernel_to_chw(&self.kernel);
        let kernel = kernel_chw;

        // Calculate same padding: (kernel_size - 1) / 2
        // For 3x3 kernel with stride 1: (3 - 1) / 2 = 1
        let pad = (KERNEL_SIZE - 1) / 2;
        let pad_h = pad as u32;
        let pad_w = pad as u32;

        // With same padding and stride=1, output size equals input size
        let out_h = h;
        let out_w = w;

        // Set up convolution parameters
        let params = ConvParams {
            batch: 1,
            in_channels: CHANNELS as u32,
            out_channels: CHANNELS as u32, // For depthwise, out_channels = in_channels
            in_h: h,
            in_w: w,
            kernel_h: KERNEL_SIZE as u32,
            kernel_w: KERNEL_SIZE as u32,
            stride_h: 1,
            stride_w: 1,
            pad_h,
            pad_w,
            out_h,
            out_w,
        };

        // Allocate output buffer: [batch, channels, out_h, out_w]
        let output_size = (params.batch * params.out_channels * params.out_h * params.out_w) as usize;
        let mut output = vec![0.0f32; output_size];

        // Perform depthwise convolution using global backend
        CONV_BACKEND.depthwise_conv2d(&input, &kernel, &mut output, params);

        // Output is in [C, H, W] format (batch=1), convert back to [H, W, C]
        let mut image_data = self.image_from_chw(&output, out_h as usize, out_w as usize);
        
        // Clamp pixel values to ensure they never go below minimum threshold
        // This prevents the screen from going completely black
        const MIN_VALUE: f32 = 1.0 / 255.0; // Minimum value (1 in 0-255 range, normalized)
        const MAX_VALUE: f32 = 1.0;
        
        for pixel_value in image_data.iter_mut() {
            *pixel_value = pixel_value.max(MIN_VALUE).min(MAX_VALUE);
        }
        
        self.image = image_data;
        
        // Update dimensions (should be same with padding=1, but update to be safe)
        self.height = out_h;
        self.width = out_w;
    }

    /// Calculate new kernel from audio samples (returns the kernel, doesn't update self.kernel)
    /// Distributes all audio samples across 21 kernel cells in round-robin fashion,
    /// then averages each cell. This makes the kernel more informationally dense.
    fn calculate_kernel_from_audio(&mut self, samples: &[f32]) -> Vec<f32> {
        const KERNEL_CELLS: usize = 21; // Number of cells to fill from audio
        const KERNEL_LEN: usize = KERNEL_SIZE * KERNEL_SIZE * CHANNELS; // 27 total
        
        if samples.is_empty() {
            // If no samples, return the previous kernel (keep existing state)
            return self.previous_kernel.clone();
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
        
        // Blend: new_kernel = previous * (1 - blend) + new * blend
        // Use previous_kernel if it's the right size, otherwise just return new_kernel
        let mut blended_kernel = Vec::with_capacity(KERNEL_LEN);
        if self.previous_kernel.len() == KERNEL_LEN {
            for i in 0..KERNEL_LEN {
                blended_kernel.push(self.previous_kernel[i] * (1.0 - BLEND_FACTOR) + new_kernel[i] * BLEND_FACTOR);
            }
        } else {
            // If previous_kernel is wrong size, just return new_kernel
            blended_kernel = new_kernel;
        }
        
        blended_kernel
    }

    /// Update kernel from audio samples (legacy method for non-blending mode)
    fn update_kernel_from_audio(&mut self, samples: &[f32]) {
        let new_kernel = self.calculate_kernel_from_audio(samples);
        const KERNEL_LEN: usize = KERNEL_SIZE * KERNEL_SIZE * CHANNELS;
        
        // Ensure previous_kernel is the right size
        if self.previous_kernel.len() != KERNEL_LEN {
            self.previous_kernel = new_kernel.clone();
        }
        
        // Update kernel directly (non-blending mode)
        self.kernel = new_kernel;
        self.previous_kernel = self.kernel.clone();
    }

    /// Update with new audio samples - updates kernel and applies convolution
    pub fn update_samples(&mut self, samples: &[f32]) {
        // Increment step counter
        self.step_counter += 1;
        
        // Only update kernel every kth step (where k = STEPS_PER_KERNEL)
        if self.step_counter % STEPS_PER_KERNEL == 0 {
            self.update_kernel_from_audio(samples);
        }
        
        // Always apply convolution with the current kernel
        self.apply_convolution();
    }

    /// Render the convolved image to the frame buffer with perfect square pixels
    /// Applies convolution each frame for dynamic visualization
    pub fn tick(&mut self, state: &mut EngineState) {
        self.tick_with_seed(state, 0.0);
    }

    /// Render with randomization based on seek position
    pub fn tick_with_seed(&mut self, state: &mut EngineState, seek_position: f32) {
        // Reinitialize image if seek position changed (very sensitive for seeking)
        let position_delta = (seek_position - self.last_seek_position).abs();
        if position_delta > 0.0001 {
            // Convert seek position to a seed (0.0 to 1.0 -> 0 to u32::MAX)
            let seed = (seek_position * u32::MAX as f32) as u32;
            self.reinitialize_image_with_seed(seed);
            self.last_seek_position = seek_position;
        }

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
