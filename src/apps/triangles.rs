use crate::engine::{Application, EngineState};
use delaunator::{triangulate, Point};
use rand::Rng;
use std::collections::HashMap;

const BAND_HEIGHT: f64 = 200.0;
const POINT_RADIUS: f64 = 6.0;
const POINTS_PER_BAND: usize = 40;

pub struct InfiniteTriApp {
    scroll_offset: f64,
    cached_bands: HashMap<i32, Band>,
}

struct Band {
    y_offset: f64,
    points: Vec<Point>,
    triangles: Vec<[usize; 3]>,
    color_map: Vec<[u8; 3]>,
}

impl InfiniteTriApp {
    pub fn new() -> Self {
        Self {
            scroll_offset: 0.0,
            cached_bands: HashMap::new(),
        }
    }

    fn get_band(&mut self, index: i32, width: f64) -> &Band {
        self.cached_bands.entry(index).or_insert_with(|| {
            let mut rng = rand::thread_rng();
            let y_offset = index as f64 * BAND_HEIGHT;
            let mut points = Vec::with_capacity(POINTS_PER_BAND);

            for _ in 0..POINTS_PER_BAND {
                points.push(Point {
                    x: rng.gen_range(0.0..width),
                    y: rng.gen_range(0.0..BAND_HEIGHT) + y_offset,
                });
            }

            let delaunay = triangulate(&points);
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

impl Application for InfiniteTriApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        let width = state.frame.width as f64;
        let height = state.frame.height as f64;
        let buffer = &mut state.frame.buffer;

        for chunk in buffer.chunks_exact_mut(4) {
            chunk.copy_from_slice(&[255, 255, 255, 255]);
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

        // Temporary scroll control
        self.scroll_offset += 1.5;
    }
}

fn draw_line(a: &Point, b: &Point, scroll: f64, buffer: &mut [u8], width: f64) {
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

fn put_pixel(x: i32, y: i32, color: [u8; 3], buffer: &mut [u8], width: f64) {
    if x < 0 || y < 0 {
        return;
    }
    let width = width as usize;
    let idx = ((y as usize) * width + (x as usize)) * 4;
    if idx + 3 >= buffer.len() {
        return;
    }
    buffer[idx..idx + 4].copy_from_slice(&[color[0], color[1], color[2], 255]);
}

fn draw_filled_triangle(
    _a: &Point,
    _b: &Point,
    _c: &Point,
    _color: [u8; 3],
    _scroll: f64,
    _width: f64,
    _height: f64,
    _buffer: &mut [u8],
) {
    // no-op placeholder for now
}
