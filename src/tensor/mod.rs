//! Burn tensor integration for xos
//!
//! Re-exports Burn's tensor types and provides a lightweight FrameBuffer for
//! the engine's pixel buffer (requires &mut [u8] for rasterizer compatibility).

pub mod conv;

pub use burn::tensor::backend::Backend;
pub use burn::tensor::{ElementConversion, Int, Shape, Tensor, TensorData};
pub use burn_ndarray::{NdArray, NdArrayDevice};
pub use conv::{conv2d, depthwise_conv2d};

/// Default backend and device for xos tensors
pub type XosBackend = NdArray;
pub type XosDevice = NdArrayDevice;

/// Alias for float tensors using the default backend
pub type FloatTensor<const D: usize> = Tensor<XosBackend, D>;
/// Alias for int tensors using the default backend
pub type IntTensor<const D: usize> = Tensor<XosBackend, D, Int>;

/// Frame buffer for the engine - stores RGBA pixels as [height, width, 4]
/// Uses Vec<u8> for zero-copy rasterizer access (&mut [u8])
#[derive(Debug)]
pub struct FrameBuffer {
    /// Raw RGBA pixel data
    buffer: Vec<u8>,
    /// Shape [height, width, 4]
    shape: Vec<usize>,
}

impl FrameBuffer {
    /// Create a new frame buffer with given dimensions
    pub fn new(width: u32, height: u32) -> Self {
        let shape = vec![height as usize, width as usize, 4];
        let len = (width * height * 4) as usize;
        Self {
            buffer: vec![0u8; len],
            shape,
        }
    }

    /// Get mutable access to the pixel buffer (zero-copy for rasterizer)
    pub fn buffer_mut(&mut self) -> &mut [u8] {
        &mut self.buffer
    }

    /// Get the frame shape [height, width, 4]
    pub fn shape(&self) -> &[usize] {
        &self.shape
    }

    /// Resize the frame to new dimensions
    pub fn resize(&mut self, width: u32, height: u32) {
        self.shape = vec![height as usize, width as usize, 4];
        self.buffer = vec![0u8; (width * height * 4) as usize];
    }

    /// Clear the frame buffer to opaque black (RGBA 0,0,0,255).
    ///
    /// Filling all bytes with zero would clear to transparent black, which composites incorrectly
    /// when the buffer is uploaded as a texture with alpha blending.
    pub fn clear(&mut self) {
        for chunk in self.buffer.chunks_exact_mut(4) {
            chunk[0] = 0;
            chunk[1] = 0;
            chunk[2] = 0;
            chunk[3] = 0xff;
        }
    }
}
