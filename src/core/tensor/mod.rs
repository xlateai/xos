//! Burn tensor integration for xos
//!
//! Re-exports Burn's [`Tensor`] and provides [`FrameTensor`]: RGBA frame storage as a
//! [`Tensor`] on [`XosDevice`] (wgpu), with a CPU staging buffer for `pixels` upload and
//! legacy `&mut [u8]` raster paths.
//!
//! Not every draw path uses this tensor: on **native**, `rasterizer::circles` queues a **WGSL
//! compute** pass after the CPU→GPU upload (see `rasterizer::render_pending_gpu_passes`); it does
//! not rasterize circles through Burn. **Python** `xos.rasterizer.circles` uses **CPU** helpers
//! (`draw_circles_cpu_instances`). WASM still uses Burn for circles (`rasterizer::shapes::circles`).

mod fill;

pub mod burn_raster;
pub mod conv;

pub use burn::tensor::backend::Backend;
pub use burn::tensor::{ElementConversion, Int, Shape, Tensor, TensorData};
pub use burn_wgpu::{Wgpu, WgpuDevice};
pub use conv::{conv2d, depthwise_conv2d};

/// Default backend and device for xos tensors (GPU via wgpu).
pub type XosBackend = Wgpu;
pub type XosDevice = WgpuDevice;

/// Alias for float tensors using the default backend
pub type FloatTensor<const D: usize> = Tensor<XosBackend, D>;
/// Alias for int tensors using the default backend
pub type IntTensor<const D: usize> = Tensor<XosBackend, D, Int>;

use burn::tensor::Float;
use std::ptr::NonNull;

/// RGBA frame: primary storage is a Burn [`Tensor`] `f32` in **0..=255** per channel; shape
/// `[height, width, 4]`. A CPU mirror is kept for presentation (`pixels`) and code that still
/// writes `&mut [u8]`.
///
/// On **native desktop**, the winit host can bind `pixels::Pixels::frame_mut` so the engine draws
/// into the same `Vec` that `pixels.render` uploads — avoiding a full-frame `copy_from_slice` every tick.
pub struct FrameTensor {
    tensor: Tensor<XosBackend, 3, Float>,
    device: WgpuDevice,
    width: u32,
    height: u32,
    cpu_staging: Vec<u8>,
    /// When set, `buffer_mut` / fills use this memory instead of `cpu_staging` (native + `pixels`).
    pixels_mirror: Option<(NonNull<u8>, usize)>,
    /// GPU tensor has newer pixels than `cpu_staging`.
    gpu_dirty: bool,
    /// CPU staging was written since last GPU sync.
    cpu_dirty: bool,
}

impl FrameTensor {
    /// Create a new opaque-black frame.
    pub fn new(width: u32, height: u32) -> Self {
        let device = WgpuDevice::default();
        let h = height as usize;
        let w = width as usize;
        let len = (width * height * 4) as usize;
        let mut cpu_staging = vec![0u8; len];
        for chunk in cpu_staging.chunks_exact_mut(4) {
            chunk.copy_from_slice(&[0, 0, 0, 0xff]);
        }
        let tensor = burn_raster::tensor_from_rgba_u8(&device, w, h, &cpu_staging);
        Self {
            tensor,
            device,
            width,
            height,
            cpu_staging,
            pixels_mirror: None,
            gpu_dirty: false,
            cpu_dirty: false,
        }
    }

    /// # Safety
    /// `ptr` must be valid for writes of `len` bytes until [`Self::clear_pixels_mirror_buffer`].
    /// `len` must be `width * height * 4`. The caller must not resize the backing buffer or
    /// call `pixels.frame_mut()` again until this is cleared.
    pub(crate) unsafe fn set_pixels_mirror_buffer(&mut self, ptr: *mut u8, len: usize) {
        debug_assert_eq!(len, (self.width * self.height * 4) as usize);
        self.pixels_mirror = Some((NonNull::new(ptr).expect("pixels mirror ptr"), len));
    }

    pub(crate) fn clear_pixels_mirror_buffer(&mut self) {
        self.pixels_mirror = None;
    }

    fn staging_slice_mut(&mut self) -> &mut [u8] {
        if let Some((ptr, len)) = &self.pixels_mirror {
            unsafe { std::slice::from_raw_parts_mut(ptr.as_ptr(), *len) }
        } else {
            &mut self.cpu_staging
        }
    }

    fn staging_slice(&self) -> &[u8] {
        if let Some((ptr, len)) = &self.pixels_mirror {
            unsafe { std::slice::from_raw_parts(ptr.as_ptr(), *len) }
        } else {
            &self.cpu_staging
        }
    }

    #[inline]
    pub(crate) fn device(&self) -> &WgpuDevice {
        &self.device
    }

    #[inline]
    pub(crate) fn tensor_dims(&self) -> [usize; 3] {
        self.tensor.dims()
    }

    #[inline]
    pub(crate) fn tensor(&self) -> &Tensor<XosBackend, 3, Float> {
        &self.tensor
    }

    /// Replace GPU frame (marks GPU authoritative; CPU stale until next `buffer_mut` / `data`).
    pub(crate) fn set_tensor(&mut self, t: Tensor<XosBackend, 3, Float>) {
        self.tensor = t;
        self.gpu_dirty = true;
        self.cpu_dirty = false;
    }

    /// If the CPU staging was mutated, upload it to the GPU tensor before Burn raster ops.
    ///
    /// This is a **CPU → GPU** copy (u8 → f32 tensor). Full-frame solid fill ([`Self::fill_solid_fast`])
    /// only updates CPU staging and sets `cpu_dirty` so this runs before the next Burn raster op,
    /// avoiding a per-frame tensor upload that can stress the wgpu queue (and risk device loss).
    pub(crate) fn ensure_gpu_from_cpu(&mut self) {
        if self.cpu_dirty {
            let w = self.width as usize;
            let h = self.height as usize;
            let (ptr, len) = match &self.pixels_mirror {
                Some((p, l)) => (p.as_ptr() as *const u8, *l),
                None => (self.cpu_staging.as_ptr(), self.cpu_staging.len()),
            };
            let staging = unsafe { std::slice::from_raw_parts(ptr, len) };
            self.tensor = burn_raster::tensor_from_rgba_u8(&self.device, w, h, staging);
            self.cpu_dirty = false;
        }
    }

    /// Full-frame solid color: fill CPU staging only (no Burn tensor upload this frame).
    ///
    /// Per-frame `Tensor::from_data` / GPU uploads were doubling FPS vs the old GPU path but could
    /// trigger `Queue::submit` / device loss under load. Presentation reads the staging buffer
    /// (or the `pixels` mirror on native) via [`Self::buffer_mut`]; the GPU tensor is stale until
    /// [`Self::ensure_gpu_from_cpu`] runs before a Burn raster op.
    pub(crate) fn fill_solid_fast(&mut self, color: (u8, u8, u8, u8)) {
        fill::fill_solid_fast(self, color);
    }

    fn sync_tensor_to_cpu(&mut self) {
        let h = self.height as usize;
        let w = self.width as usize;
        let data = self.tensor.clone().into_data();
        let s = data.as_slice::<f32>().expect("frame f32");
        let buf = self.staging_slice_mut();
        for i in 0..(h * w) {
            let o = i * 4;
            buf[o] = s[o].clamp(0., 255.) as u8;
            buf[o + 1] = s[o + 1].clamp(0., 255.) as u8;
            buf[o + 2] = s[o + 2].clamp(0., 255.) as u8;
            buf[o + 3] = s[o + 3].clamp(0., 255.) as u8;
        }
    }

    /// Immutable RGBA bytes; syncs from GPU if the tensor was written more recently than CPU.
    pub fn data(&mut self) -> &[u8] {
        if self.gpu_dirty {
            self.sync_tensor_to_cpu();
            self.gpu_dirty = false;
        }
        self.staging_slice()
    }

    /// Legacy zero-copy-style CPU buffer (keyboard, text, FFI). Syncs GPU→CPU when needed.
    pub fn buffer_mut(&mut self) -> &mut [u8] {
        if self.gpu_dirty {
            self.sync_tensor_to_cpu();
            self.gpu_dirty = false;
        }
        self.cpu_dirty = true;
        self.staging_slice_mut()
    }

    /// Get the frame shape `[height, width, 4]`
    pub fn shape(&self) -> Vec<usize> {
        vec![
            self.height as usize,
            self.width as usize,
            4,
        ]
    }

    /// Resize the frame (opaque black).
    pub fn resize(&mut self, width: u32, height: u32) {
        *self = FrameTensor::new(width, height);
    }

    /// Clear to opaque black (GPU + CPU).
    pub fn clear(&mut self) {
        let len = (self.width * self.height * 4) as usize;
        self.pixels_mirror = None;
        self.cpu_staging.clear();
        self.cpu_staging.resize(len, 0);
        for chunk in self.cpu_staging.chunks_exact_mut(4) {
            chunk.copy_from_slice(&[0, 0, 0, 0xff]);
        }
        self.tensor = burn_raster::tensor_from_rgba_u8(
            &self.device,
            self.width as usize,
            self.height as usize,
            &self.cpu_staging,
        );
        self.gpu_dirty = false;
        self.cpu_dirty = false;
    }
}

impl std::fmt::Debug for FrameTensor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameTensor")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("pixels_mirror", &self.pixels_mirror.is_some())
            .field("gpu_dirty", &self.gpu_dirty)
            .field("cpu_dirty", &self.cpu_dirty)
            .finish()
    }
}

/// Backwards-compatible name for [`FrameTensor`].
pub type FrameBuffer = FrameTensor;
