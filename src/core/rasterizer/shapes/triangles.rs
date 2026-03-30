//! Filled triangle rasterization.

use crate::engine::FrameState;
use crate::tensor::burn_raster;

/// Half-space edge test (same sign convention as `apps::triangles::geometric_utils::edge_function`).
#[inline]
pub fn edge_ori(ax: f64, ay: f64, bx: f64, by: f64, px: f64, py: f64) -> f64 {
    (bx - ax) * (py - ay) - (by - ay) * (px - ax)
}

/// Filled triangle into an RGBA8 buffer. Pixel-space coordinates; clipped to the framebuffer.
///
/// Uses a bounding box plus incremental half-space weights along each scanline (O(bbox area),
/// fast inner loop vs naive double `edge_function` per pixel).
pub fn fill_triangle_buffer(
    buffer: &mut [u8],
    width: usize,
    height: usize,
    v0: (f32, f32),
    v1: (f32, f32),
    v2: (f32, f32),
    color: [u8; 4],
) {
    if width == 0 || height == 0 {
        return;
    }
    let ax = v0.0 as f64;
    let ay = v0.1 as f64;
    let mut bx = v1.0 as f64;
    let mut by = v1.1 as f64;
    let mut cx = v2.0 as f64;
    let mut cy = v2.1 as f64;

    let area = edge_ori(ax, ay, bx, by, cx, cy);
    if area < 0.0 {
        std::mem::swap(&mut bx, &mut cx);
        std::mem::swap(&mut by, &mut cy);
    }
    if edge_ori(ax, ay, bx, by, cx, cy).abs() < 1e-20 {
        return;
    }

    let wi = width as i32;
    let hi = height as i32;
    let min_x = ax.min(bx).min(cx).floor() as i32;
    let max_x = ax.max(bx).max(cx).ceil() as i32;
    let min_y = ay.min(by).min(cy).floor() as i32;
    let max_y = ay.max(by).max(cy).ceil() as i32;

    let min_x = min_x.max(0).min(wi - 1);
    let max_x = max_x.max(0).min(wi - 1);
    let min_y = min_y.max(0).min(hi - 1);
    let max_y = max_y.max(0).min(hi - 1);
    if min_x > max_x || min_y > max_y {
        return;
    }

    let w0_dx = by - cy;
    let w1_dx = cy - ay;
    let w2_dx = ay - by;

    for y in min_y..=max_y {
        let py = y as f64 + 0.5;
        let px0 = min_x as f64 + 0.5;
        let mut w0 = edge_ori(bx, by, cx, cy, px0, py);
        let mut w1 = edge_ori(cx, cy, ax, ay, px0, py);
        let mut w2 = edge_ori(ax, ay, bx, by, px0, py);
        for x in min_x..=max_x {
            if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                let idx = (y as usize * width + x as usize) * 4;
                if idx + 3 < buffer.len() {
                    buffer[idx..idx + 4].copy_from_slice(&color);
                }
            }
            w0 += w0_dx;
            w1 += w1_dx;
            w2 += w2_dx;
        }
    }
}

/// Filled triangles: `points` is `[a0, b0, c0, a1, b1, c1, …]` in pixel coordinates.
/// `colors.len()` must be `n` or `1` (broadcast), where `n = points.len() / 3`.
pub fn triangles_buffer(
    buffer: &mut [u8],
    width: usize,
    height: usize,
    points: &[(f32, f32)],
    colors: &[[u8; 4]],
) -> Result<(), String> {
    if points.len() % 3 != 0 {
        return Err(format!(
            "points length {} is not divisible by 3",
            points.len()
        ));
    }
    let n = points.len() / 3;
    if n == 0 {
        return Ok(());
    }
    if colors.is_empty() {
        return Err("colors is empty".into());
    }
    if colors.len() != n && colors.len() != 1 {
        return Err(format!(
            "colors length {} must match triangle count ({}) or be 1",
            colors.len(),
            n
        ));
    }

    for i in 0..n {
        let c = if colors.len() == 1 {
            colors[0]
        } else {
            colors[i]
        };
        let j = i * 3;
        fill_triangle_buffer(
            buffer,
            width,
            height,
            points[j],
            points[j + 1],
            points[j + 2],
            c,
        );
    }
    Ok(())
}

/// Same as [`triangles_buffer`] but takes a [`FrameState`].
pub fn triangles(
    frame: &mut FrameState,
    points: &[(f32, f32)],
    colors: &[[u8; 4]],
) -> Result<(), String> {
    burn_raster::triangles(&mut frame.tensor, points, colors)
}
