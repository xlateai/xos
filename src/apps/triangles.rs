use crate::engine::{Application, EngineState};
use delaunator::{triangulate, Point};
use rand::Rng;

const NUM_POINTS: usize = 256;
const UNIT_MIN: f64 = -0.25;
const UNIT_MAX: f64 = 1.25;

const LINE_COLOR: (u8, u8, u8) = (255, 255, 255);
const LINE_THICKNESS: i32 = 1;
const POINT_COLOR: (u8, u8, u8) = (255, 255, 255);
const POINT_RADIUS: i32 = 3;
const BACKGROUND_COLOR: (u8, u8, u8) = (0, 0, 0);

pub struct TrianglesApp {
    unit_points: Vec<Point>,         // normalized points [0.0, 1.0]
    triangles: Vec<[usize; 3]>,      // index triplets
    last_width: u32,
    last_height: u32,
}

impl TrianglesApp {
    pub fn new() -> Self {
        let mut rng = rand::thread_rng();
        let unit_points = (0..NUM_POINTS)
            .map(|_| Point {
                x: rng.gen_range(UNIT_MIN..UNIT_MAX),
                y: rng.gen_range(UNIT_MIN..UNIT_MAX),
            })
            .collect();
    
        Self {
            unit_points,
            triangles: Vec::new(),
            last_width: 0,
            last_height: 0,
        }
    }

    fn recompute_triangulation(&mut self, width: f64, height: f64) {
        let points: Vec<Point> = self.unit_points
            .iter()
            .map(|p| Point {
                x: p.x * width,
                y: p.y * height,
            })
            .collect();

        let result = triangulate(&points);
        self.triangles = result
            .triangles
            .chunks(3)
            .filter_map(|t| {
                if t.len() == 3 {
                    Some([t[0], t[1], t[2]])
                } else {
                    None
                }
            })
            .collect();
    }
}

impl Application for TrianglesApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        let width = state.frame.width;
        let height = state.frame.height;
        let buffer = &mut state.frame.buffer;

        for i in (0..buffer.len()).step_by(4) {
            buffer[i + 0] = BACKGROUND_COLOR.0;
            buffer[i + 1] = BACKGROUND_COLOR.1;
            buffer[i + 2] = BACKGROUND_COLOR.2;
            buffer[i + 3] = 255;
        }

        let width_f = width as f64;
        let height_f = height as f64;

        if width != self.last_width || height != self.last_height {
            self.recompute_triangulation(width_f, height_f);
            self.last_width = width;
            self.last_height = height;
        }

        // Map normalized points to pixel space
        let points: Vec<Point> = self.unit_points
            .iter()
            .map(|p| Point {
                x: p.x * width_f,
                y: p.y * height_f,
            })
            .collect();

        for tri in &self.triangles {
            let a = &points[tri[0]];
            let b = &points[tri[1]];
            let c = &points[tri[2]];
            draw_line(a.x, a.y, b.x, b.y, buffer, width_f, height_f, LINE_COLOR);
            draw_line(b.x, b.y, c.x, c.y, buffer, width_f, height_f, LINE_COLOR);
            draw_line(c.x, c.y, a.x, a.y, buffer, width_f, height_f, LINE_COLOR);
        }

        for p in &points {
            draw_circle(p.x, p.y, POINT_RADIUS, buffer, width_f, height_f, POINT_COLOR);
        }
    }
}

fn draw_line(x0: f64, y0: f64, x1: f64, y1: f64, buffer: &mut [u8], width: f64, height: f64, color: (u8, u8, u8)) {
    for dx in -LINE_THICKNESS..=LINE_THICKNESS {
        for dy in -LINE_THICKNESS..=LINE_THICKNESS {
            draw_thin_line(x0 + dx as f64, y0 + dy as f64, x1 + dx as f64, y1 + dy as f64, buffer, width, height, color);
        }
    }
}

fn draw_thin_line(x0: f64, y0: f64, x1: f64, y1: f64, buffer: &mut [u8], width: f64, height: f64, color: (u8, u8, u8)) {
    let (mut x0, mut y0, mut x1, mut y1) = (x0 as i32, y0 as i32, x1 as i32, y1 as i32);
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
    let mut err = dx + dy;

    while x0 != x1 || y0 != y1 {
        put_pixel(x0, y0, buffer, width, height, color);
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

fn draw_circle(cx: f64, cy: f64, radius: i32, buffer: &mut [u8], width: f64, height: f64, color: (u8, u8, u8)) {
    let (cx, cy) = (cx as i32, cy as i32);
    let r2 = radius * radius;
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if dx * dx + dy * dy <= r2 {
                put_pixel(cx + dx, cy + dy, buffer, width, height, color);
            }
        }
    }
}

fn put_pixel(x: i32, y: i32, buffer: &mut [u8], width: f64, height: f64, color: (u8, u8, u8)) {
    if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
        return;
    }
    let idx = (y as usize * width as usize + x as usize) * 4;
    if idx + 3 < buffer.len() {
        buffer[idx..idx + 4].copy_from_slice(&[color.0, color.1, color.2, 255]);
    }
}
