use crate::engine::Application;

const BACKGROUND_COLOR: (u8, u8, u8) = (0, 0, 0);
const DRAW_COLOR: (u8, u8, u8) = (255, 255, 255);
const STROKE_WIDTH: f32 = 2.0;

pub struct Whiteboard {
    strokes: Vec<(f32, f32)>,
    drawing: bool,
}

impl Whiteboard {
    pub fn new() -> Self {
        Self {
            strokes: Vec::new(),
            drawing: false,
        }
    }

    fn draw_frame(&self, width: u32, height: u32) -> Vec<u8> {
        let mut pixels = vec![0u8; (width * height * 4) as usize];

        for i in (0..pixels.len()).step_by(4) {
            pixels[i + 0] = BACKGROUND_COLOR.0;
            pixels[i + 1] = BACKGROUND_COLOR.1;
            pixels[i + 2] = BACKGROUND_COLOR.2;
            pixels[i + 3] = 0xff;
        }

        for window in self.strokes.windows(2) {
            if let [(x0, y0), (x1, y1)] = window {
                self.draw_line(&mut pixels, width, height, *x0, *y0, *x1, *y1);
            }
        }

        pixels
    }

    fn draw_line(&self, pixels: &mut [u8], width: u32, height: u32, x0: f32, y0: f32, x1: f32, y1: f32) {
        let dx = x1 - x0;
        let dy = y1 - y0;
        let steps = dx.abs().max(dy.abs()) as usize;

        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let x = x0 + t * dx;
            let y = y0 + t * dy;
            self.draw_circle(pixels, width, height, x, y, STROKE_WIDTH);
        }
    }

    fn draw_circle(&self, pixels: &mut [u8], width: u32, height: u32, cx: f32, cy: f32, radius: f32) {
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
                    pixels[i + 0] = DRAW_COLOR.0;
                    pixels[i + 1] = DRAW_COLOR.1;
                    pixels[i + 2] = DRAW_COLOR.2;
                    pixels[i + 3] = 0xff;
                }
            }
        }
    }
}

impl Application for Whiteboard {
    fn setup(&mut self, _width: u32, _height: u32) -> Result<(), String> {
        Ok(())
    }

    fn tick(&mut self, width: u32, height: u32) -> Vec<u8> {
        self.draw_frame(width, height)
    }

    fn on_mouse_down(&mut self, x: f32, y: f32) {
        self.drawing = true;
        self.strokes.push((x, y));
    }

    fn on_mouse_up(&mut self, _x: f32, _y: f32) {
        self.drawing = false;
    }

    fn on_mouse_move(&mut self, x: f32, y: f32) {
        if self.drawing {
            self.strokes.push((x, y));
        }
    }
}