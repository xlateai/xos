//! GPU presentation (Burn frame tensor → pixels texture without CPU readback).
//!
//! Blocked until `pixels` and Burn/cubecl share the same `wgpu` major version
//! (`pixels` 0.15 → wgpu 0.19; Burn 0.21 → wgpu 29). Until then, display uses
//! [`FrameState::publish_gpu_to_staging`] + `pixels` CPU upload.

use crate::engine::FrameState;
use crate::rasterizer::RasterCache;
use pixels::wgpu::{CommandEncoder, Device, Extent3d, Queue, Texture, TextureFormat};

/// Placeholder cache for a future same-`wgpu` blit pipeline.
pub struct GpuPresentCache;

impl GpuPresentCache {
    pub fn new() -> Self {
        Self
    }
}

/// Not implemented yet (wgpu version mismatch between `pixels` and Burn).
pub fn blit_frame_to_texture(
    _cache: &mut RasterCache,
    _frame: &FrameState,
    _encoder: &mut CommandEncoder,
    _device: &Device,
    _queue: &Queue,
    _dst_texture: &Texture,
    _dst_format: TextureFormat,
    _extent: Extent3d,
) -> bool {
    false
}
