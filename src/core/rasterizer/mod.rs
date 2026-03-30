//! Framebuffer raster helpers (`fill`, `circles`, …): **pure functions** `(&mut FrameState, …) -> …`
//! with no hidden engine state. Shape fills use Burn on [`crate::tensor::XosBackend`] (wgpu); CPU
//! staging is synced for keyboard / FPS overlay and `pixels` upload.

use crate::engine::FrameState;
use crate::tensor::burn_raster;

mod cache;
pub mod shapes;
pub mod text;
pub use cache::RasterCache;

pub use shapes::{
    circles, draw_circle_cpu, draw_circles_cpu, draw_circles_cpu_instances, fill_rect,
    fill_rect_buffer, fill_triangle_buffer, triangles, triangles_buffer,
};

/// Fill `frame` with a solid RGBA color. Matches Python: `xos.rasterizer.fill(frame, (r, g, b, a))`.
#[inline]
pub fn fill(frame: &mut FrameState, color: (u8, u8, u8, u8)) {
    burn_raster::fill_solid(&mut frame.tensor, color);
}

/// Hook for future GPU passes after CPU upload; currently a no-op (Burn raster runs on the frame tensor).
#[cfg(not(target_arch = "wasm32"))]
pub fn render_pending_gpu_passes(
    _cache: &mut RasterCache,
    _encoder: &mut pixels::wgpu::CommandEncoder,
    _device: &pixels::wgpu::Device,
    _queue: &pixels::wgpu::Queue,
    _inner_texture: &pixels::wgpu::Texture,
    _extent: pixels::wgpu::Extent3d,
    _texture_format: pixels::wgpu::TextureFormat,
) {
}
