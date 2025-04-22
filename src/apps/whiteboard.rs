use crate::engine::{Application, EngineState};

const DRAW_COLOR: (u8, u8, u8) = (255, 255, 255);
const STROKE_WIDTH: f32 = 2.0;

fn draw_line(pixels: &mut [u8], width: u32, height: u32, x0: f32, y0: f32, x1: f32, y1: f32) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let steps = dx.abs().max(dy.abs()) as usize;

    if steps == 0 {
        draw_circle(pixels, width, height, x0, y0, STROKE_WIDTH);
        return;
    }

    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let x = x0 + t * dx;
        let y = y0 + t * dy;
        draw_circle(pixels, width, height, x, y, STROKE_WIDTH);
    }
}

fn draw_circle(pixels: &mut [u8], width: u32, height: u32, cx: f32, cy: f32, radius: f32) {
    let radius_squared = radius * radius;
    let start_x = (cx - radius).floor() as i32;
    let end_x = (cx + radius).ceil() as i32;
    let start_y = (cy - radius).floor() as i32;
    let end_y = (cy + radius).ceil() as i32;

    for y in start_y..end_y {
        for x in start_x..end_x {
            if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
                continue;
            }
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            if dx * dx + dy * dy <= radius_squared {
                let i = ((y as u32 * width + x as u32) * 4) as usize;
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

fn draw_cursor_dot(pixels: &mut [u8], width: u32, height: u32, x: f32, y: f32, left: bool, right: bool) {
    let color = if right {
        (255, 0, 0)
    } else if left {
        (0, 255, 0)
    } else {
        (255, 255, 255)
    };
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
    strokes: Vec<Vec<(f32, f32)>>,
    current_stroke: Vec<(f32, f32)>,
    was_drawing: bool,

    offset_x: f32,
    offset_y: f32,
    zoom: f32,

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
            offset_x: 0.0,
            offset_y: 0.0,
            zoom: 1.0,
            cached_canvas: Vec::new(),
            cached_width: 0,
            cached_height: 0,
            needs_redraw: true,
        }
    }

    fn screen_to_world(&self, x: f32, y: f32) -> (f32, f32) {
        ((x - self.offset_x) / self.zoom, (y - self.offset_y) / self.zoom)
    }

    fn world_to_screen(&self, x: f32, y: f32) -> (f32, f32) {
        (x * self.zoom + self.offset_x, y * self.zoom + self.offset_y)
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

        if width != self.cached_width || height != self.cached_height {
            self.cached_width = width;
            self.cached_height = height;
            self.cached_canvas = vec![0; (width * height * 4) as usize];
        }

        // Right click: pan
        if state.mouse.is_right_clicking {
            self.offset_x += state.mouse.dx;
            self.offset_y += state.mouse.dy;
        }

        // Left click: draw stroke
        if state.mouse.is_left_clicking && self.current_stroke.len() < 10_000 {
            let p = self.screen_to_world(state.mouse.x, state.mouse.y);
            self.current_stroke.push(p);
        }

        // On release: commit stroke (even if just 1 dot)
        if self.was_drawing && !state.mouse.is_left_clicking {
            if self.current_stroke.is_empty() {
                let p = self.screen_to_world(state.mouse.x, state.mouse.y);
                self.current_stroke.push(p);
            }
            self.strokes.push(std::mem::take(&mut self.current_stroke));
            self.needs_redraw = true;
        }

        self.was_drawing = state.mouse.is_left_clicking;

        // Draw all completed strokes
        self.cached_canvas.fill(0);
        for stroke in &self.strokes {
            if stroke.len() == 1 {
                let (x, y) = self.world_to_screen(stroke[0].0, stroke[0].1);
                draw_circle(&mut self.cached_canvas, width, height, x, y, STROKE_WIDTH);
            } else {
                for stroke_pair in stroke.windows(2) {
                    let (x0, y0) = self.world_to_screen(stroke_pair[0].0, stroke_pair[0].1);
                    let (x1, y1) = self.world_to_screen(stroke_pair[1].0, stroke_pair[1].1);
                    draw_line(&mut self.cached_canvas, width, height, x0, y0, x1, y1);
                }
            }
        }

        // Push final canvas
        state.frame.buffer.copy_from_slice(&self.cached_canvas);

        // Live preview stroke
        if let Some((first, rest)) = self.current_stroke.split_first() {
            let (x0, y0) = self.world_to_screen(first.0, first.1);
            if rest.is_empty() {
                draw_circle(&mut state.frame.buffer, width, height, x0, y0, STROKE_WIDTH);
            } else {
                let mut last = *first;
                for &point in rest {
                    let (x1, y1) = self.world_to_screen(point.0, point.1);
                    draw_line(&mut state.frame.buffer, width, height, last.0 * self.zoom + self.offset_x, last.1 * self.zoom + self.offset_y, x1, y1);
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
            state.mouse.is_right_clicking,
        );
    }

    fn on_scroll(&mut self, state: &mut EngineState, _dx: f32, dy: f32) {
        let factor = if dy > 0.0 { 1.1 } else { 1.0 / 1.1 };

        let mouse_screen_x = state.mouse.x;
        let mouse_screen_y = state.mouse.y;
        let world_before = self.screen_to_world(mouse_screen_x, mouse_screen_y);
        self.zoom *= factor;
        let world_after = self.screen_to_world(mouse_screen_x, mouse_screen_y);
        self.offset_x += (world_after.0 - world_before.0) * self.zoom;
        self.offset_y += (world_after.1 - world_before.1) * self.zoom;
    }
}