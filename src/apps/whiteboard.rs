use crate::engine::{Application, EngineState};

const DRAW_COLOR: (u8, u8, u8) = (255, 255, 255);
const STROKE_WIDTH: f32 = 2.0;

fn catmull_rom(p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), p3: (f32, f32), t: f32) -> (f32, f32) {
    let t2 = t * t;
    let t3 = t2 * t;

    let x = 0.5 * (
        2.0 * p1.0 +
        (-p0.0 + p2.0) * t +
        (2.0*p0.0 - 5.0*p1.0 + 4.0*p2.0 - p3.0) * t2 +
        (-p0.0 + 3.0*p1.0 - 3.0*p2.0 + p3.0) * t3
    );

    let y = 0.5 * (
        2.0 * p1.1 +
        (-p0.1 + p2.1) * t +
        (2.0*p0.1 - 5.0*p1.1 + 4.0*p2.1 - p3.1) * t2 +
        (-p0.1 + 3.0*p1.1 - 3.0*p2.1 + p3.1) * t3
    );

    (x, y)
}

fn draw_smooth_stroke(pixels: &mut [u8], width: u32, height: u32, stroke: &[(f32, f32)]) {
    if stroke.len() < 2 {
        return;
    }

    for i in 0..stroke.len().saturating_sub(1) {
        let p0 = if i > 0 { stroke[i - 1] } else { stroke[i] };
        let p1 = stroke[i];
        let p2 = stroke[i + 1];
        let p3 = if i + 2 < stroke.len() { stroke[i + 2] } else { p2 };

        let segments = 8;
        for j in 0..segments {
            let t0 = j as f32 / segments as f32;
            let t1 = (j + 1) as f32 / segments as f32;

            let (x0, y0) = catmull_rom(p0, p1, p2, p3, t0);
            let (x1, y1) = catmull_rom(p0, p1, p2, p3, t1);

            draw_line(pixels, width, height, x0 * width as f32, y0 * height as f32, x1 * width as f32, y1 * height as f32);
        }
    }
}

fn draw_line(pixels: &mut [u8], width: u32, height: u32, x0: f32, y0: f32, x1: f32, y1: f32) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let steps = dx.abs().max(dy.abs()) as usize;

    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let x = x0 + t * dx;
        let y = y0 + t * dy;
        draw_circle(pixels, width, height, x, y, STROKE_WIDTH);
    }
}

fn draw_circle(pixels: &mut [u8], width: u32, height: u32, cx: f32, cy: f32, radius: f32) {
    let radius_squared = radius * radius;
    let start_x = (cx - radius).max(0.0) as u32;
    let end_x = (cx + radius).min(width as f32) as u32;
    let start_y = (cy - radius).max(0.0) as u32;
    let end_y = (cy + radius).min(height as f32) as u32;

    for y in start_y..end_y {
        for x in start_x..end_x {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            if dx * dx + dy * dy <= radius_squared {
                let i = ((y * width + x) * 4) as usize;
                if i + 3 < pixels.len() {
                    pixels[i + 0] = DRAW_COLOR.0;
                    pixels[i + 1] = DRAW_COLOR.1;
                    pixels[i + 2] = DRAW_COLOR.2;
                    pixels[i + 3] = 0xff;
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct Whiteboard {
    strokes: Vec<Vec<(f32, f32)>>, // Normalized strokes [0.0 - 1.0]
    current_stroke: Vec<(f32, f32)>,
    drawing: bool,

    // Cache
    cached_canvas: Vec<u8>,
    cached_width: u32,
    cached_height: u32,
    needs_redraw: bool,
}

impl Whiteboard {
    pub fn new() -> Self {
        Self {
            strokes: Vec::new(),
            current_stroke: Vec::new(),
            drawing: false,
            cached_canvas: Vec::new(),
            cached_width: 0,
            cached_height: 0,
            needs_redraw: true,
        }
    }

    fn normalize(x: f32, y: f32, width: u32, height: u32) -> (f32, f32) {
        (x / width as f32, y / height as f32)
    }
}

impl Application for Whiteboard {
    fn setup(&mut self, state: &mut EngineState) -> Result<(), String> {
        self.cached_width = state.frame.width;
        self.cached_height = state.frame.height;
        self.cached_canvas = vec![0; (self.cached_width * self.cached_height * 4) as usize];
        self.needs_redraw = true;
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        let (width, height) = (state.frame.width, state.frame.height);

        // Resize + rebuild cache if window changed
        if width != self.cached_width || height != self.cached_height {
            self.cached_width = width;
            self.cached_height = height;
            self.cached_canvas = vec![0; (width * height * 4) as usize];
            self.needs_redraw = true;
        }

        // Redraw cached strokes only when needed
        if self.needs_redraw {
            self.cached_canvas.fill(0);
            for stroke in &self.strokes {
                draw_smooth_stroke(&mut self.cached_canvas, width, height, stroke);
            }
            self.needs_redraw = false;
        }

        // Copy cached canvas into live buffer
        state.frame.buffer.copy_from_slice(&self.cached_canvas);

        // Draw current stroke on top
        draw_smooth_stroke(&mut state.frame.buffer, width, height, &self.current_stroke);
    }

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        self.drawing = true;
        self.current_stroke.clear();

        let point = Self::normalize(state.mouse.x, state.mouse.y, state.frame.width, state.frame.height);
        self.current_stroke.push(point);
    }

    fn on_mouse_move(&mut self, state: &mut EngineState) {
        if self.drawing {
            let point = Self::normalize(state.mouse.x, state.mouse.y, state.frame.width, state.frame.height);
            self.current_stroke.push(point);
        }
    }

    fn on_mouse_up(&mut self, _state: &mut EngineState) {
        self.drawing = false;
        if !self.current_stroke.is_empty() {
            self.strokes.push(self.current_stroke.clone());
            self.current_stroke.clear();
            self.needs_redraw = true;
        }
    }
}
