use crate::engine::{Application, EngineState};
use delaunator::{triangulate, Point};
use rand::Rng;
use std::collections::HashMap;

const BAND_HEIGHT: f32 = 200.0;
const POINT_RADIUS: f32 = 6.0;
const POINTS_PER_BAND: usize = 40;

pub struct TrianglesApp {
    scroll_offset: f32,
    cached_bands: HashMap<i32, Band>,
}

struct Band {
    y_offset: f32,
    points: Vec<Point>,
    triangles: Vec<[usize; 3]>,
    color_map: Vec<[u8; 3]>,
}

impl TrianglesApp {
    pub fn new() -> Self {
        Self {
            scroll_offset: 0.0,
            cached_bands: HashMap::new(),
        }
    }

    fn get_band(&mut self, index: i32, width: f32) -> &Band {
        self.cached_bands.entry(index).or_insert_with(|| {
            let mut rng = rand::thread_rng();
            let y_offset = index as f32 * BAND_HEIGHT;
            let mut points = Vec::with_capacity(POINTS_PER_BAND);

            for _ in 0..POINTS_PER_BAND {
                points.push(Point {
                    x: rng.gen_range(0.0..width),
                    y: rng.gen_range(0.0..BAND_HEIGHT) + y_offset,
                });
            }

            let delaunay = triangulate(&points).expect("triangulation failed");
            let triangles = delaunay.triangles.chunks(3)
                .map(|t| [t[0], t[1], t[2]])
                .collect();

            let color_map = (0..(delaunay.triangles.len() / 3))
                .map(|_| [rng.gen_range(50..230), rng.gen_range(50..230), rng.gen_range(50..230)])
                .collect();

            Band { y_offset, points, triangles, color_map }
        })
    }
}

impl Application for TrianglesApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        let width = state.frame.width as f32;
        let height = state.frame.height as f32;
        let buffer = &mut state.frame.buffer;

        // Clear screen
        for chunk in buffer.chunks_exact_mut(4) {
            chunk.copy_from_slice(&[255, 255, 255, 255]); // white background
        }

        let top_band = ((self.scroll_offset - BAND_HEIGHT).floor() / BAND_HEIGHT).floor() as i32;
        let bottom_band = ((self.scroll_offset + height).ceil() / BAND_HEIGHT).ceil() as i32;

        for band_index in top_band..=bottom_band {
            let band = self.get_band(band_index, width);

            for (i, tri) in band.triangles.iter().enumerate() {
                let a = &band.points[tri[0]];
                let b = &band.points[tri[1]];
                let c = &band.points[tri[2]];
                let color = band.color_map[i];
                draw_filled_triangle(a, b, c, color, self.scroll_offset, width, height, buffer);
                draw_line(a, b, self.scroll_offset, buffer, width);
                draw_line(b, c, self.scroll_offset, buffer, width);
                draw_line(c, a, self.scroll_offset, buffer, width);
            }
        }

        // Optional: scroll using up/down keys
        if state.keyboard.is_pressed("ArrowUp") {
            self.scroll_offset -= 4.0;
        }
        if state.keyboard.is_pressed("ArrowDown") {
            self.scroll_offset += 4.0;
        }
    }
}

// Simple line drawing using Bresenham
fn draw_line(a: &Point, b: &Point, scroll: f32, buffer: &mut [u8], width: f32) {
    let (x0, y0) = (a.x as i32, (a.y - scroll) as i32);
    let (x1, y1) = (b.x as i32, (b.y - scroll) as i32);
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
    let mut err = dx + dy;
    let (mut x, mut y) = (x0, y0);

    while x != x1 || y != y1 {
        put_pixel(x, y, [0, 0, 0], buffer, width);
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

fn draw_filled_triangle(a: &Point, b: &Point, c: &Point, color: [u8; 3], scroll: f32, width: f32, height: f32, buffer: &mut [u8]) {
    // TODO: barycentric rasterization or bounding box fill
    // For now: do nothing. We’ll add real triangle fills next.
}

fn put_pixel(x: i32, y: i32, color: [u8; 3], buffer: &mut [u8], width: f32) {
    if x < 0 || y < 0 || (x as usize) >= width as usize || (y as usize) >= (buffer.len() / (4 * width as usize)) {
        return;
    }
    let idx = ((y as usize) * width as usize + (x as usize)) * 4;
    if idx + 3 < buffer.len() {
        buffer[idx..idx + 4].copy_from_slice(&[color[0], color[1], color[2], 255]);
    }
}
