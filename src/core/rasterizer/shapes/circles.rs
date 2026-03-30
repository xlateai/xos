//! Filled circles: CPU raster into the frame buffer (same path as Python `xos.rasterizer.circles`).

use crate::engine::FrameState;

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

    let shape = frame.shape();
    let height = shape[0];
    let width = shape[1];
    let buffer = frame.buffer_mut();
    draw_circles_cpu_instances(buffer, width, height, &instances);
    Ok(())
}

/// CPU: filled circles with per-instance RGBA (wasm / legacy `&mut [u8]` paths).
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
