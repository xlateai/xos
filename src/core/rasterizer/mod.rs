//! Framebuffer raster helpers (`fill`, `circles`, …): **pure functions** `(&mut FrameState, …) -> …`
//! with no hidden engine state. Shape fills use Burn on [`crate::tensor::XosBackend`] (wgpu); CPU
//! staging is synced for keyboard / FPS overlay and `pixels` upload.

use crate::engine::FrameState;
use crate::tensor::burn_raster;

mod cache;

#[cfg(not(target_arch = "wasm32"))]
pub use shapes::circles::GpuCircle;
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

/// After `pixels` uploads the CPU frame buffer, runs WGSL circle compositing when
/// [`GpuCircle`]s were queued for this frame (native desktop).
#[cfg(not(target_arch = "wasm32"))]
pub fn render_pending_gpu_passes(
    frame: &mut FrameState,
    cache: &mut RasterCache,
    encoder: &mut pixels::wgpu::CommandEncoder,
    device: &pixels::wgpu::Device,
    queue: &pixels::wgpu::Queue,
    inner_texture: &pixels::wgpu::Texture,
    extent: pixels::wgpu::Extent3d,
    texture_format: pixels::wgpu::TextureFormat,
) {
    let pending = frame.take_wgpu_circles();
    if pending.is_empty() {
        return;
    }
    if texture_format != pixels::wgpu::TextureFormat::Rgba8Unorm {
        eprintln!(
            "xos: WGSL circles expect Rgba8Unorm framebuffer; got {:?}",
            texture_format
        );
        return;
    }

    if cache.circles_gpu.is_none() {
        cache.circles_gpu = Some(shapes::circles::CirclesGpu::new(device, extent));
    }
    let gpu = cache.circles_gpu.as_mut().expect("circles_gpu");
    let input_view = inner_texture.create_view(&pixels::wgpu::TextureViewDescriptor::default());
    gpu.encode(
        device,
        queue,
        encoder,
        &input_view,
        inner_texture,
        extent,
        &pending,
    );
}
