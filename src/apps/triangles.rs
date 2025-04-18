use crate::engine::{Application, EngineState};
use delaunator::{triangulate, Point};
use rand::Rng;

const NUM_POINTS: usize = 64;
const BACKGROUND_COLOR: (u8, u8, u8) = (0, 0, 0);
const LINE_COLOR: (u8, u8, u8) = (255, 255, 255);

pub struct TrianglesApp {
    points: Vec<Point>,
    triangles: Vec<[usize; 3]>,
    generated: bool,
}

impl TrianglesApp {
    pub fn new() -> Self {
        Self {
            points: Vec::new(),
            triangles: Vec::new(),
            generated: false,
        }
    }

    fn generate(&mut self, width: f64, height: f64) {
        let mut rng = rand::thread_rng();
        self.points = (0..NUM_POINTS)
            .map(|_| Point {
                x: rng.gen_range(0.0..width),
                y: rng.gen_range(0.0..height),
            })
            .collect();

        let result = triangulate(&self.points);
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

        self.generated = true;
    }
}

impl Application for TrianglesApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        let width = state.frame.width as f64;
        let height = state.frame.height as f64;
        let buffer = &mut state.frame.buffer;

        for i in (0..buffer.len()).step_by(4) {
            buffer[i + 0] = BACKGROUND_COLOR.0;
            buffer[i + 1] = BACKGROUND_COLOR.1;
            buffer[i + 2] = BACKGROUND_COLOR.2;
            buffer[i + 3] = 255;
        }

        if !self.generated {
            self.generate(width, height);
        }

        for tri in &self.triangles {
            let a = &self.points[tri[0]];
            let b = &self.points[tri[1]];
            let c = &self.points[tri[2]];

            draw_line(a.x, a.y, b.x, b.y, buffer, width, height, LINE_COLOR);
            draw_line(b.x, b.y, c.x, c.y, buffer, width, height, LINE_COLOR);
            draw_line(c.x, c.y, a.x, a.y, buffer, width, height, LINE_COLOR);
        }
    }
}

fn draw_line(x0: f64, y0: f64, x1: f64, y1: f64, buffer: &mut [u8], width: f64, height: f64, color: (u8, u8, u8)) {
    let (mut x0, mut y0, mut x1, mut y1) = (x0 as i32, y0 as i32, x1 as i32, y1 as i32);

    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
    let mut err = dx + dy;

    while x0 != x1 || y0 != y1 {
        if x0 >= 0 && y0 >= 0 && (x0 as usize) < width as usize && (y0 as usize) < height as usize {
            let idx = ((y0 as usize) * width as usize + (x0 as usize)) * 4;
            if idx + 3 < buffer.len() {
                buffer[idx..idx + 4].copy_from_slice(&[color.0, color.1, color.2, 255]);
            }
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
