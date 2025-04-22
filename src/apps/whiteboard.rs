use crate::engine::{Application, EngineState};

const DRAW_COLOR: (u8, u8, u8) = (255, 255, 255);
const STROKE_WIDTH: f32 = 2.0;

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

fn draw_cursor_dot(pixels: &mut [u8], width: u32, height: u32, x: f32, y: f32, pressed: bool) {
    let color = if pressed { (0, 255, 0) } else { (255, 255, 255) };
    let radius = 3.0;
    let radius_squared = radius * radius;
    let start_x = (x - radius).max(0.0) as u32;
    let end_x = (x + radius).min(width as f32) as u32;
    let start_y = (y - radius).max(0.0) as u32;
    let end_y = (y + radius).min(height as f32) as u32;

    for y_ in start_y..end_y {
        for x_ in start_x..end_x {
            let dx = x_ as f32 - x;
            let dy = y_ as f32 - y;
            if dx * dx + dy * dy <= radius_squared {
                let i = ((y_ * width + x_) * 4) as usize;
                if i + 3 < pixels.len() {
                    pixels[i + 0] = color.0;
                    pixels[i + 1] = color.1;
                    pixels[i + 2] = color.2;
                    pixels[i + 3] = 0xff;
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct Whiteboard {
    strokes: Vec<Vec<(f32, f32)>>, // Normalized
    current_stroke: Vec<(f32, f32)>,
    was_drawing: bool,

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
            was_drawing: false,
            cached_canvas: Vec::new(),
            cached_width: 0,
            cached_height: 0,
            needs_redraw: true,
        }
    }

    fn normalize(x: f32, y: f32, width: u32, height: u32) -> (f32, f32) {
        (x / width as f32, y / height as f32)
    }

    fn denormalize(x: f32, y: f32, width: u32, height: u32) -> (f32, f32) {
        (x * width as f32, y * height as f32)
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

        // Resize
        if width != self.cached_width || height != self.cached_height {
            self.cached_width = width;
            self.cached_height = height;
            self.cached_canvas = vec![0; (width * height * 4) as usize];
            self.needs_redraw = true;
        }

        // Add point if drawing
        if state.mouse.is_left_clicking && self.current_stroke.len() < 10_000 {
            let p = Self::normalize(state.mouse.x, state.mouse.y, width, height);
            self.current_stroke.push(p);
        }

        // On release
        if self.was_drawing && !state.mouse.is_left_clicking {
            if !self.current_stroke.is_empty() {
                self.strokes.push(std::mem::take(&mut self.current_stroke));
                self.needs_redraw = true;
            }
        }

        self.was_drawing = state.mouse.is_left_clicking;

        // Redraw cache
        if self.needs_redraw {
            self.cached_canvas.fill(0);
            for stroke in &self.strokes {
                match stroke.len() {
                    0 => {}
                    1 => {
                        let (x, y) = Self::denormalize(stroke[0].0, stroke[0].1, width, height);
                        draw_circle(&mut self.cached_canvas, width, height, x, y, STROKE_WIDTH);
                    }
                    _ => {
                        let mut last = stroke[0];
                        for &point in &stroke[1..] {
                            if (point.0 - last.0).abs() + (point.1 - last.1).abs() > 0.001 {
                                let (x0, y0) = Self::denormalize(last.0, last.1, width, height);
                                let (x1, y1) = Self::denormalize(point.0, point.1, width, height);
                                draw_line(&mut self.cached_canvas, width, height, x0, y0, x1, y1);
                            }
                            last = point;
                        }
                    }
                }
            }
            self.needs_redraw = false;
        }

        // Copy cache
        state.frame.buffer.copy_from_slice(&self.cached_canvas);

        // Live stroke
        match self.current_stroke.len() {
            0 => {}
            1 => {
                let (x, y) = Self::denormalize(self.current_stroke[0].0, self.current_stroke[0].1, width, height);
                draw_circle(&mut state.frame.buffer, width, height, x, y, STROKE_WIDTH);
            }
            _ => {
                let mut last = self.current_stroke[0];
                for &point in &self.current_stroke[1..] {
                    let (x0, y0) = Self::denormalize(last.0, last.1, width, height);
                    let (x1, y1) = Self::denormalize(point.0, point.1, width, height);
                    draw_line(&mut state.frame.buffer, width, height, x0, y0, x1, y1);
                    last = point;
                }
            }
        }

        // Cursor
        draw_cursor_dot(
            &mut state.frame.buffer,
            width,
            height,
            state.mouse.x,
            state.mouse.y,
            state.mouse.is_left_clicking,
        );
    }
}
