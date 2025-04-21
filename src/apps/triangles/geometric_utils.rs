// geometric_utils.rs

use delaunator::Point;
use rand::Rng;

pub fn edge_function(a: &Point, b: &Point, x: f64, y: f64) -> f64 {
    (b.x - a.x) * (y - a.y) - (b.y - a.y) * (x - a.x)
}

pub fn draw_filled_triangle(
    a: &Point,
    b: &Point,
    c: &Point,
    buffer: &mut [u8],
    width: f64,
    height: f64,
    color: (u8, u8, u8),
) {
    let min_x = a.x.min(b.x).min(c.x).floor() as i32;
    let max_x = a.x.max(b.x).max(c.x).ceil() as i32;
    let min_y = a.y.min(b.y).min(c.y).floor() as i32;
    let max_y = a.y.max(b.y).max(c.y).ceil() as i32;

    let area = edge_function(a, b, c.x, c.y);
    if area == 0.0 {
        return;
    }

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let px = x as f64 + 0.5;
            let py = y as f64 + 0.5;
            let w0 = edge_function(b, c, px, py);
            let w1 = edge_function(c, a, px, py);
            let w2 = edge_function(a, b, px, py);

            if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                put_pixel(x, y, buffer, width, height, color);
            }
        }
    }
}

pub fn put_pixel(
    x: i32,
    y: i32,
    buffer: &mut [u8],
    width: f64,
    height: f64,
    color: (u8, u8, u8),
) {
    if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
        return;
    }
    let idx = (y as usize * width as usize + x as usize) * 4;
    if idx + 3 < buffer.len() {
        buffer[idx..idx + 4].copy_from_slice(&[color.0, color.1, color.2, 255]);
    }
}
