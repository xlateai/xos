//! Line segments: Bresenham for thin strokes, circle stamps for thick strokes (`ball_pairs` path).

use super::circles::draw_circle_cpu;

/// Draw a line segment with thickness in pixels. Thin lines (`thickness < 2`) use Bresenham;
/// thick lines stamp `draw_circle_cpu` along the segment (same as Python `xos.rasterizer.lines`).
pub fn draw_line_direct(
    buffer: &mut [u8],
    width: usize,
    height: usize,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    thickness: f32,
    color: (u8, u8, u8, u8),
) {
    // For very thin lines (< 2 pixels), use super-fast Bresenham algorithm
    if thickness < 2.0 {
        draw_line_bresenham(buffer, width, height, x1, y1, x2, y2, color);
        return;
    }

    let radius = thickness / 2.0;

    // Calculate line vector and length
    let dx = x2 - x1;
    let dy = y2 - y1;
    let length = (dx * dx + dy * dy).sqrt();

    if length < 0.001 {
        // Degenerate line, just draw a circle
        draw_circle_cpu(buffer, width, height, x1, y1, radius, color);
        return;
    }

    // For thick lines: Draw circles along the line at regular intervals
    let step_size = (radius * 0.5).max(1.0);
    let num_steps = (length / step_size).ceil() as i32 + 1;

    for i in 0..=num_steps {
        let t = (i as f32) / (num_steps as f32);
        let x = x1 + dx * t;
        let y = y1 + dy * t;
        draw_circle_cpu(buffer, width, height, x, y, radius, color);
    }
}

/// Bresenham line algorithm for thin lines (1 pixel).
pub fn draw_line_bresenham(
    buffer: &mut [u8],
    width: usize,
    height: usize,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    color: (u8, u8, u8, u8),
) {
    let mut x0 = x1 as i32;
    let mut y0 = y1 as i32;
    let x1 = x2 as i32;
    let y1 = y2 as i32;

    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        // Draw pixel if in bounds
        if x0 >= 0 && x0 < width as i32 && y0 >= 0 && y0 < height as i32 {
            let idx = (y0 as usize * width + x0 as usize) * 4;
            if idx + 3 < buffer.len() {
                buffer[idx + 0] = color.0;
                buffer[idx + 1] = color.1;
                buffer[idx + 2] = color.2;
                buffer[idx + 3] = color.3;
            }
        }

        if x0 == x1 && y0 == y1 {
            break;
        }

        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}
