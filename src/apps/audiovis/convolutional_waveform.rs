use crate::engine::EngineState;

/// Convolutional waveform visualizer - uses audio to drive a convolutional filter
pub struct ConvolutionalWaveform {
    /// Image buffer (width x height pixels, each pixel is a single f32 value)
    image: Vec<f32>,
    /// Image dimensions
    width: u32,
    height: u32,
    /// 3x3x3 convolutional kernel (27 values total)
    kernel: [f32; 27],
}

impl ConvolutionalWaveform {
    pub fn new(width: u32, height: u32) -> Self {
        // Initialize random image mask
        let mut image = Vec::with_capacity((width * height) as usize);
        
        #[cfg(not(target_arch = "wasm32"))]
        {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            for _ in 0..(width * height) {
                // Random value between 0.0 and 1.0
                image.push(rng.gen::<f32>());
            }
        }
        
        #[cfg(target_arch = "wasm32")]
        {
            // WASM: use simple pseudo-random
            let mut seed = 12345u32;
            for _ in 0..(width * height) {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                image.push((seed % 1000) as f32 / 1000.0);
            }
        }

        Self {
            image,
            width,
            height,
            kernel: [0.0; 27], // Initialize all zeros
        }
    }

    /// Update with new audio samples and perform convolution
    pub fn update_samples(&mut self, samples: &[f32]) {
        // Take first 21 values and assign to kernel
        // Normalize audio samples (which are typically -1.0 to 1.0) to a better range
        for i in 0..21 {
            if i < 27 {
                // Map audio samples from [-1, 1] to [-0.5, 0.5] for kernel weights
                // This prevents the convolution from going too extreme
                self.kernel[i] = samples[i] * 0.5;
            }
        }
        // Fill remaining 6 values with zeros
        for i in 21..27 {
            self.kernel[i] = 0.0;
        }

        // Perform convolution
        self.convolve();
    }

    /// Perform 3D convolution on the image
    fn convolve(&mut self) {
        let mut new_image = vec![0.0; self.image.len()];
        
        // Simple by-hand convolution
        // For each pixel, apply the 3x3 kernel
        for y in 1..(self.height - 1) {
            for x in 1..(self.width - 1) {
                let mut sum = 0.0;
                
                // Apply 3x3 kernel using the first 9 values
                let kernel_2d = [
                    self.kernel[0], self.kernel[1], self.kernel[2],
                    self.kernel[3], self.kernel[4], self.kernel[5],
                    self.kernel[6], self.kernel[7], self.kernel[8],
                ];
                
                let mut kernel_idx = 0;
                for ky in -1..=1 {
                    for kx in -1..=1 {
                        let px = (x as i32 + kx) as u32;
                        let py = (y as i32 + ky) as u32;
                        let idx = (py * self.width + px) as usize;
                        sum += self.image[idx] * kernel_2d[kernel_idx];
                        kernel_idx += 1;
                    }
                }
                
                // Add bias to keep values visible (center kernel value acts as bias)
                // This prevents everything from going black
                sum += 0.5; // Add 0.5 bias to shift range
                
                // Normalize and clamp to [0, 1]
                let idx = (y * self.width + x) as usize;
                new_image[idx] = sum.max(0.0).min(1.0);
            }
        }
        
        // Apply convolution to edges too (use mirror padding)
        for y in 0..self.height {
            for x in 0..self.width {
                if x == 0 || x == self.width - 1 || y == 0 || y == self.height - 1 {
                    // Edge pixel - use mirror padding for convolution
                    let mut sum = 0.0;
                    let kernel_2d = [
                        self.kernel[0], self.kernel[1], self.kernel[2],
                        self.kernel[3], self.kernel[4], self.kernel[5],
                        self.kernel[6], self.kernel[7], self.kernel[8],
                    ];
                    
                    let mut kernel_idx = 0;
                    for ky in -1..=1 {
                        for kx in -1..=1 {
                            // Mirror padding: reflect coordinates at edges
                            let px = if x == 0 && kx < 0 {
                                0
                            } else if x == self.width - 1 && kx > 0 {
                                self.width - 1
                            } else {
                                ((x as i32 + kx).max(0).min(self.width as i32 - 1)) as u32
                            };
                            
                            let py = if y == 0 && ky < 0 {
                                0
                            } else if y == self.height - 1 && ky > 0 {
                                self.height - 1
                            } else {
                                ((y as i32 + ky).max(0).min(self.height as i32 - 1)) as u32
                            };
                            
                            let idx = (py * self.width + px) as usize;
                            sum += self.image[idx] * kernel_2d[kernel_idx];
                            kernel_idx += 1;
                        }
                    }
                    
                    sum += 0.5; // Add bias
                    let idx = (y * self.width + x) as usize;
                    new_image[idx] = sum.max(0.0).min(1.0);
                }
            }
        }
        
        self.image = new_image;
    }

    /// Render the convolved image to the frame buffer
    pub fn tick(&mut self, state: &mut EngineState) {
        let buffer = &mut state.frame.buffer;
        let width = state.frame.width;
        let height = state.frame.height;

        // Scale our internal image to match the frame buffer
        let scale_x = self.width as f32 / width as f32;
        let scale_y = self.height as f32 / height as f32;

        for py in 0..height {
            for px in 0..width {
                // Map frame coordinates to image coordinates
                let ix = (px as f32 * scale_x) as u32;
                let iy = (py as f32 * scale_y) as u32;
                let img_idx = (iy * self.width + ix) as usize;
                
                if img_idx < self.image.len() {
                    let value = self.image[img_idx];
                    // Convert to grayscale color (0-255)
                    let gray = (value * 255.0) as u8;
                    
                    let idx = ((py * width + px) * 4) as usize;
                    if idx + 3 < buffer.len() {
                        buffer[idx + 0] = gray;
                        buffer[idx + 1] = gray;
                        buffer[idx + 2] = gray;
                        buffer[idx + 3] = 0xff;
                    }
                }
            }
        }
    }
}
