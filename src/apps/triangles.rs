use crate::engine::{Application, EngineState};
use delaunator::{triangulate, Point};
use rand::Rng;

const NUM_POINTS: usize = 40;
const BACKGROUND_COLOR: (u8, u8, u8) = (0, 0, 0);
const LINE_COLOR: (u8, u8, u8) = (255, 255, 255);
const EXTRA_MARGIN: f64 = 100.0;

pub struct TrianglesApp {
    scroll_x: f64,
    scroll_y: f64,
    dragging: bool,
    last_mouse_x: f32,
    last_mouse_y: f32,
}

impl TrianglesApp {
    pub fn new() -> Self {
        Self {
            scroll_x: 0.0,
            scroll_y: 0.0,
            dragging: false,
            last_mouse_x: 0.0,
            last_mouse_y: 0.0,
        }
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
            buffer[i + 3] = 0xff;
        }

        let mut rng = rand::thread_rng();
        let mut points = Vec::with_capacity(NUM_POINTS);

        for _ in 0..NUM_POINTS {
            points.push(Point {
                x: rng.gen_range(-EXTRA_MARGIN..(width + EXTRA_MARGIN)) + self.scroll_x,
                y: rng.gen_range(-EXTRA_MARGIN..(height + EXTRA_MARGIN)) + self.scroll_y,
            });
        }

        let triangulation = triangulate(&points);
        for tri in triangulation.triangles.chunks(3) {
            if tri.len() == 3 {
                let a = &points[tri[0]];
                let b = &points[tri[1]];
                let c = &points[tri[2]];

                draw_line(
                    a.x - self.scroll_x,
                    a.y - self.scroll_y,
                    b.x - self.scroll_x,
                    b.y - self.scroll_y,
                    buffer,
                    width,
                    height,
                    LINE_COLOR,
                );
                draw_line(
                    b.x - self.scroll_x,
                    b.y - self.scroll_y,
                    c.x - self.scroll_x,
                    c.y - self.scroll_y,
                    buffer,
                    width,
                    height,
                    LINE_COLOR,
                );
                draw_line(
                    c.x - self.scroll_x,
                    c.y - self.scroll_y,
                    a.x - self.scroll_x,
                    a.y - self.scroll_y,
                    buffer,
                    width,
                    height,
                    LINE_COLOR,
                );
            }
        }
    }

    fn on_scroll(&mut self, _state: &mut EngineState, dx: f32, dy: f32) {
        self.scroll_x += dx as f64;
        self.scroll_y += dy as f64;
    }

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        self.dragging = true;
        self.last_mouse_x = state.mouse.x;
        self.last_mouse_y = state.mouse.y;
    }

    fn on_mouse_up(&mut self, _state: &mut EngineState) {
        self.dragging = false;
    }

    fn on_mouse_move(&mut self, state: &mut EngineState) {
        if self.dragging {
            let x = state.mouse.x;
            let y = state.mouse.y;
            let dx = x - self.last_mouse_x;
            let dy = y - self.last_mouse_y;
            self.scroll_x -= dx as f64;
            self.scroll_y -= dy as f64;
            self.last_mouse_x = x;
            self.last_mouse_y = y;
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
