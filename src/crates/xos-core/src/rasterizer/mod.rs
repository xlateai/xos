//! Framebuffer raster helpers (`fill`, `circles`, …): **pure functions** `(&mut FrameState, …) -> …`
//! with no hidden engine state. Filled circles are CPU-rasterized into staging; solid fill and
//! other shapes may use Burn on [`xos_tensor::XosBackend`] (wgpu); CPU staging is synced for
//! keyboard / FPS overlay and `pixels` upload.

use crate::engine::FrameState;
use crate::burn_raster;

mod cache;
pub mod blur;
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
    burn_raster::fill_solid(frame, color);
}

/// Blit the Burn frame tensor into the pixels backing texture (same wgpu device as Burn).
#[cfg(not(target_arch = "wasm32"))]
pub fn render_pending_gpu_passes(
    cache: &mut RasterCache,
    frame: &mut crate::engine::FrameState,
    encoder: &mut pixels::wgpu::CommandEncoder,
    device: &pixels::wgpu::Device,
    queue: &pixels::wgpu::Queue,
    inner_texture: &pixels::wgpu::Texture,
    extent: pixels::wgpu::Extent3d,
    texture_format: pixels::wgpu::TextureFormat,
) -> bool {
    crate::gpu_present::blit_frame_to_texture(
        cache,
        frame,
        encoder,
        device,
        queue,
        inner_texture,
        texture_format,
        extent,
    )
}
