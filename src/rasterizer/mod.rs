//! Vectorized circle rasterization (`circles(frame, centers, radii, colors)`) — no engine state.
//!
//! Desktop (non-wasm, non-iOS): batches are queued for the wgpu pass after CPU buffer upload.
//! wasm / iOS: draws on the CPU frame buffer immediately.

use crate::engine::FrameState;
use std::sync::Mutex;

mod cache;
pub use cache::RasterCache;

/// One GPU draw worth of instances (cx, cy, radius px, RGBA8).
#[derive(Clone, Debug)]
pub(crate) struct GpuRasterBatch {
    pub instances: Vec<(f32, f32, f32, [u8; 4])>,
}

static PENDING_GPU_BATCHES: Mutex<Vec<GpuRasterBatch>> = Mutex::new(Vec::new());

fn push_gpu_batch(batch: GpuRasterBatch) {
    if batch.instances.is_empty() {
        return;
    }
    PENDING_GPU_BATCHES.lock().unwrap().push(batch);
}

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

    #[cfg(any(target_arch = "wasm32", target_os = "ios"))]
    {
        let shape = frame.shape();
        let width = shape[1];
        let height = shape[0];
        draw_circles_cpu_instances(frame.buffer_mut(), width, height, &instances);
    }
    #[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
    {
        push_gpu_batch(GpuRasterBatch { instances });
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
