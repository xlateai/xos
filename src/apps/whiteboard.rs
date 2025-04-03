use crate::engine::Application;

const BACKGROUND_COLOR: (u8, u8, u8) = (0, 0, 0);
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


pub struct Whiteboard {
    strokes: Vec<Vec<(f32, f32)>>,
    current_stroke: Vec<(f32, f32)>,
    drawing: bool,
    cached_pixels: Option<Vec<u8>>,
    last_width: u32,
    last_height: u32,
}

impl Whiteboard {
    pub fn new() -> Self {
        Self {
            strokes: Vec::new(),
            current_stroke: Vec::new(),
            drawing: false,
            cached_pixels: None,
            last_width: 0,
            last_height: 0,
        }
    }

    fn draw_frame(&mut self, width: u32, height: u32) -> Vec<u8> {
        if self.last_width != width || self.last_height != height {
            self.cached_pixels = None;
            self.last_width = width;
            self.last_height = height;
        }
    
        if self.cached_pixels.is_none() {
            let mut pixels = vec![0u8; (width * height * 4) as usize];
            for i in (0..pixels.len()).step_by(4) {
                pixels[i + 0] = BACKGROUND_COLOR.0;
                pixels[i + 1] = BACKGROUND_COLOR.1;
                pixels[i + 2] = BACKGROUND_COLOR.2;
                pixels[i + 3] = 0xff;
            }
    
            for stroke in &self.strokes {
                self.draw_smooth_stroke(&mut pixels, width, height, stroke);
            }
    
            self.cached_pixels = Some(pixels);
        }
    
        let mut pixels = self.cached_pixels.clone().unwrap();
        self.draw_smooth_stroke(&mut pixels, width, height, &self.current_stroke);
    
        pixels
    }
    
    fn draw_smooth_stroke(&self, pixels: &mut [u8], width: u32, height: u32, stroke: &[(f32, f32)]) {
        if stroke.len() < 2 {
            return;
        }
    
        for i in 0..stroke.len().saturating_sub(1) {
            let p0 = if i > 0 { stroke[i - 1] } else { stroke[i] };
            let p1 = stroke[i];
            let p2 = stroke[i + 1];
            let p3 = if i + 2 < stroke.len() { stroke[i + 2] } else { p2 };
    
            // Interpolate between p1 and p2
            let segments = 8;
            for j in 0..segments {
                let t0 = j as f32 / segments as f32;
                let t1 = (j + 1) as f32 / segments as f32;
    
                let (x0, y0) = catmull_rom(p0, p1, p2, p3, t0);
                let (x1, y1) = catmull_rom(p0, p1, p2, p3, t1);
    
                self.draw_line(pixels, width, height, x0, y0, x1, y1);
            }
        }
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
        self.current_stroke.clear();
        self.current_stroke.push((x, y));
    }

    fn on_mouse_up(&mut self, _x: f32, _y: f32) {
        self.drawing = false;
        if !self.current_stroke.is_empty() {
            self.strokes.push(self.current_stroke.clone());
            self.current_stroke.clear();
            self.cached_pixels = None; // Invalidate cache
        }
    }

    fn on_mouse_move(&mut self, x: f32, y: f32) {
        if self.drawing {
            self.current_stroke.push((x, y));
        }
    }
}