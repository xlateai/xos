//! Burn tensor integration for xos
//!
//! Re-exports Burn's [`Tensor`] and provides [`FrameTensor`]: a CPU-side RGBA pixel grid used by
//! the engine and rasterizer (`&mut [u8]`). It is **not** a Burn tensor; ML ops use Burn separately
//! (see [`conv`]).

pub mod conv;

pub use burn::tensor::backend::Backend;
pub use burn::tensor::{ElementConversion, Int, Shape, Tensor, TensorData};
pub use burn_wgpu::{Wgpu, WgpuDevice};
pub use conv::{conv2d, depthwise_conv2d};

/// Default backend and device for xos tensors (GPU via wgpu; CPU fallback to be wired separately)
pub type XosBackend = Wgpu;
pub type XosDevice = WgpuDevice;

/// Alias for float tensors using the default backend
pub type FloatTensor<const D: usize> = Tensor<XosBackend, D>;
/// Alias for int tensors using the default backend
pub type IntTensor<const D: usize> = Tensor<XosBackend, D, Int>;

/// CPU RGBA frame storage for the engine: shape `[height, width, 4]`, `Vec<u8>` backing.
///
/// Named [`FrameTensor`] to avoid clashing with Burn's [`Tensor`], which is GPU-backed and typed
/// (typically `f32` for conv). The display framebuffer stays on the CPU for zero-copy rasterization;
/// Burn is used only where you explicitly move data to [`XosDevice`] (e.g. [`conv2d`]).
#[derive(Debug)]
pub struct FrameTensor {
    /// Raw RGBA pixel data
    buffer: Vec<u8>,
    /// Shape [height, width, 4]
    shape: Vec<usize>,
}

impl FrameTensor {
    /// Create a new frame buffer with given dimensions
    pub fn new(width: u32, height: u32) -> Self {
        let shape = vec![height as usize, width as usize, 4];
        let len = (width * height * 4) as usize;
        Self {
            buffer: vec![0u8; len],
            shape,
        }
    }

    /// Immutable view of the pixel bytes (e.g. iOS FFI pointer).
    #[inline]
    pub fn data(&self) -> &[u8] {
        &self.buffer
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
        let px = [0u8, 0, 0, 0xff];
        for chunk in self.buffer.chunks_exact_mut(4) {
            chunk.copy_from_slice(&px);
        }
    }
}

/// Backwards-compatible name for [`FrameTensor`].
pub type FrameBuffer = FrameTensor;
