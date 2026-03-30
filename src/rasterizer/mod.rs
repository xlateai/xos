//! Framebuffer raster helpers (`fill`, `circles`, …): **pure functions** `(&mut FrameState, …) -> …`
//! with no hidden engine state, so the same APIs can be reused from Rust apps, Python, and future
//! tensor / GPU paths.
//!
//! Filled circles are rasterized on the CPU framebuffer so compositing order matches call order
//! (Python / apps, then keyboard, then FPS overlay). `render_pending_gpu_passes` remains for any
//! future GPU overlays; `circles()` no longer queues wgpu batches.

use crate::engine::FrameState;
use crate::python::rasterizer::fill_buffer_solid_rgba;

mod cache;
pub mod shapes;
pub mod text;
pub use cache::RasterCache;

pub use shapes::{
    circles, draw_circle_cpu, draw_circles_cpu, draw_circles_cpu_instances, fill_rect,
    fill_rect_buffer, fill_triangle_buffer, triangles, triangles_buffer,
};

pub use shapes::circles::GpuRasterBatch;

/// Fill `frame` with a solid RGBA color. Matches Python: `xos.rasterizer.fill(frame, (r, g, b, a))`.
#[inline]
pub fn fill(frame: &mut FrameState, color: (u8, u8, u8, u8)) {
    fill_buffer_solid_rgba(
        frame.buffer_mut(),
        color.0,
        color.1,
        color.2,
        color.3,
    );
}

/// After the CPU buffer is uploaded to the inner pixel texture, run any queued wgpu passes
/// (instanced overlays). `cache` retains backend state across frames (`Box<dyn Any + Send>`).
#[cfg(not(target_arch = "wasm32"))]
pub fn render_pending_gpu_passes(
    cache: &mut RasterCache,
    encoder: &mut pixels::wgpu::CommandEncoder,
    device: &pixels::wgpu::Device,
    queue: &pixels::wgpu::Queue,
    inner_texture: &pixels::wgpu::Texture,
    extent: pixels::wgpu::Extent3d,
    texture_format: pixels::wgpu::TextureFormat,
) {
    let batches = shapes::circles::drain_pending_gpu_batches();
    if !batches.iter().any(|b| !b.instances.is_empty()) {
        return;
    }
    if cache.inner.is_none() {
        cache.inner = Some(Box::new(shapes::circles::WgpuRasterRenderer::new(
            device,
            texture_format,
        )));
    }
    let renderer = cache
        .inner
        .as_mut()
        .and_then(|b| b.downcast_mut::<shapes::circles::WgpuRasterRenderer>())
        .expect("raster cache: expected WgpuRasterRenderer payload for GPU passes");
    renderer.ensure_format(device, texture_format);
    renderer.render(
        device,
        queue,
        encoder,
        inner_texture,
        extent,
        texture_format,
        &batches,
    );
}
