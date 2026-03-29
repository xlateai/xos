//! Framebuffer raster helpers (`fill`, `circles`, …): **pure functions** `(&mut FrameState, …) -> …`
//! with no hidden engine state, so the same APIs can be reused from Rust apps, Python, and future
//! tensor / GPU paths.
//!
//! Filled circles are rasterized on the CPU framebuffer so compositing order matches call order
//! (Python / apps, then keyboard, then FPS overlay). `render_pending_gpu_passes` remains for any
//! future GPU overlays; `circles()` no longer queues wgpu batches.

use crate::engine::FrameState;
use crate::python::rasterizer::fill_buffer_solid_rgba;
use std::sync::Mutex;

mod cache;
pub use cache::RasterCache;

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

/// Fill a clipped axis-aligned rectangle `[x0, x1) × [y0, y1)` in pixel coordinates.
/// Faster than per-pixel loops; uses row-wise `copy_from_slice`.
#[inline]
pub fn fill_rect_buffer(
    buffer: &mut [u8],
    frame_width: usize,
    frame_height: usize,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    color: (u8, u8, u8, u8),
) {
    let fw = frame_width as i32;
    let fh = frame_height as i32;
    let x0 = x0.max(0).min(fw);
    let x1 = x1.max(0).min(fw);
    let y0 = y0.max(0).min(fh);
    let y1 = y1.max(0).min(fh);
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let rgba = [color.0, color.1, color.2, color.3];
    for y in y0..y1 {
        let row_start = (y as usize * frame_width + x0 as usize) * 4;
        let row_end = (y as usize * frame_width + x1 as usize) * 4;
        debug_assert!(row_end <= buffer.len());
        for chunk in buffer[row_start..row_end].chunks_exact_mut(4) {
            chunk.copy_from_slice(&rgba);
        }
    }
}

/// Same as [`fill_rect_buffer`] but takes a [`FrameState`].
#[inline]
pub fn fill_rect(
    frame: &mut FrameState,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    color: (u8, u8, u8, u8),
) {
    let shape = frame.shape();
    let h = shape[0];
    let w = shape[1];
    fill_rect_buffer(frame.buffer_mut(), w, h, x0, y0, x1, y1, color);
}

/// One GPU draw worth of instances (cx, cy, radius px, RGBA8).
#[derive(Clone, Debug)]
pub(crate) struct GpuRasterBatch {
    pub instances: Vec<(f32, f32, f32, [u8; 4])>,
}

static PENDING_GPU_BATCHES: Mutex<Vec<GpuRasterBatch>> = Mutex::new(Vec::new());

fn drain_pending_gpu_batches() -> Vec<GpuRasterBatch> {
    let mut guard = PENDING_GPU_BATCHES.lock().unwrap();
    std::mem::take(&mut *guard)
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
    let batches = drain_pending_gpu_batches();
    if !batches.iter().any(|b| !b.instances.is_empty()) {
        return;
    }
    if cache.inner.is_none() {
        cache.inner = Some(Box::new(wgpu_circles::WgpuRasterRenderer::new(
            device,
            texture_format,
        )));
    }
    let renderer = cache
        .inner
        .as_mut()
        .and_then(|b| b.downcast_mut::<wgpu_circles::WgpuRasterRenderer>())
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

/// Draw filled circles into `frame`. Pixel coordinates; `centers`, `radii`, and `colors` must align:
/// - `radii.len() == n` or `radii.len() == 1` (broadcast),
/// - `colors.len() == n` or `colors.len() == 1` (broadcast),
/// where `n = centers.len()`.
pub fn circles(
    frame: &mut FrameState,
    centers: &[(f32, f32)],
    radii: &[f32],
    colors: &[[u8; 4]],
) -> Result<(), String> {
    let n = centers.len();
    if n == 0 {
        return Ok(());
    }
    if radii.is_empty() {
        return Err("radii is empty".into());
    }
    if colors.is_empty() {
        return Err("colors is empty".into());
    }
    if radii.len() != n && radii.len() != 1 {
        return Err(format!(
            "radii length {} must match centers ({}) or be 1",
            radii.len(),
            n
        ));
    }
    if colors.len() != n && colors.len() != 1 {
        return Err(format!(
            "colors length {} must match centers ({}) or be 1",
            colors.len(),
            n
        ));
    }

    let mut instances = Vec::with_capacity(n);
    for i in 0..n {
        let r = if radii.len() == 1 { radii[0] } else { radii[i] };
        let c = if colors.len() == 1 {
            colors[0]
        } else {
            colors[i]
        };
        instances.push((centers[i].0, centers[i].1, r, c));
    }

    // Always rasterize circles on the CPU framebuffer so compositing order matches draw order:
    // app → keyboard → `tick_fps_overlay` all stay visually above circles. The previous desktop
    // path queued wgpu instancing in `render_pending_gpu_passes`, which ran *after* the buffer upload
    // and drew on top of the FPS overlay.
    {
        let shape = frame.shape();
        let width = shape[1];
        let height = shape[0];
        draw_circles_cpu_instances(frame.buffer_mut(), width, height, &instances);
    }
    Ok(())
}

/// CPU: filled circles with per-instance RGBA (wasm / iOS path and internal helper).
pub fn draw_circles_cpu_instances(
    buffer: &mut [u8],
    width: usize,
    height: usize,
    instances: &[(f32, f32, f32, [u8; 4])],
) {
    for &(cx, cy, r, c) in instances {
        draw_circle_cpu(
            buffer,
            width,
            height,
            cx,
            cy,
            r,
            (c[0], c[1], c[2], c[3]),
        );
    }
}

/// CPU path with a single RGBA for every circle (Python / legacy helpers).
pub fn draw_circles_cpu(
    buffer: &mut [u8],
    width: usize,
    height: usize,
    circles: &[(f32, f32, f32)],
    color: (u8, u8, u8, u8),
) {
    for &(cx, cy, r) in circles {
        draw_circle_cpu(buffer, width, height, cx, cy, r, color);
    }
}

pub fn draw_circle_cpu(
    buffer: &mut [u8],
    width: usize,
    height: usize,
    cx: f32,
    cy: f32,
    radius: f32,
    color: (u8, u8, u8, u8),
) {
    let radius_squared = radius * radius;

    let start_x = (cx - radius).max(0.0) as usize;
    let end_x = ((cx + radius + 1.0) as usize).min(width);
    let start_y = (cy - radius).max(0.0) as usize;
    let end_y = ((cy + radius + 1.0) as usize).min(height);

    for y in start_y..end_y {
        for x in start_x..end_x {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            if dx * dx + dy * dy <= radius_squared {
                let idx = (y * width + x) * 4;
                if idx + 3 < buffer.len() {
                    buffer[idx + 0] = color.0;
                    buffer[idx + 1] = color.1;
                    buffer[idx + 2] = color.2;
                    buffer[idx + 3] = color.3;
                }
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub mod wgpu_circles;
