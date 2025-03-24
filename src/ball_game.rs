use wasm_bindgen::prelude::*;
use js_sys;

use crate::engine::Application;

// Common background color
const BACKGROUND_COLOR: (u8, u8, u8) = (64, 0, 64);

// The application that handles the balls
pub struct BallGame {
    balls: Vec<BallState>,
}

impl BallGame {
    pub fn new() -> Self {
        Self {
            balls: Vec::new(),
        }
    }

    fn draw_frame(&self, width: u32, height: u32) -> Vec<u8> {
        let mut pixels = vec![0u8; (width * height * 4) as usize];

        // Fill background first
        for i in (0..pixels.len()).step_by(4) {
            pixels[i + 0] = BACKGROUND_COLOR.0;
            pixels[i + 1] = BACKGROUND_COLOR.1;
            pixels[i + 2] = BACKGROUND_COLOR.2;
            pixels[i + 3] = 0xff;
        }

        // Draw all balls
        for ball in &self.balls {
            self.draw_circle(&mut pixels, width, height, ball.x, ball.y, ball.radius);
        }

        pixels
    }

    fn draw_circle(&self, pixels: &mut [u8], width: u32, height: u32, cx: f32, cy: f32, radius: f32) {
        let radius_squared = radius * radius;

        // Calculate bounding box to avoid checking every pixel
        let start_x = (cx - radius).max(0.0) as u32;
        let end_x = (cx + radius + 1.0).min(width as f32) as u32;
        let start_y = (cy - radius).max(0.0) as u32;
        let end_y = (cy + radius + 1.0).min(height as f32) as u32;

        for y in start_y..end_y {
            for x in start_x..end_x {
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                let distance_squared = dx * dx + dy * dy;

                let i = ((y * width + x) * 4) as usize;

                if distance_squared <= radius_squared {
                    pixels[i + 0] = 0x00; // R
                    pixels[i + 1] = 0xff; // G
                    pixels[i + 2] = 0x00; // B
                    pixels[i + 3] = 0xff; // A
                }
            }
        }
    }
}

impl Application for BallGame {
    fn setup(&mut self, width: u32, height: u32) -> Result<(), JsValue> {
        // Add initial ball at center
        self.balls.push(BallState::new(width as f32, height as f32, 30.0));
        Ok(())
    }

    fn tick(&mut self, width: u32, height: u32) -> Vec<u8> {
        // Update all balls
        for ball in &mut self.balls {
            ball.update(width as f32, height as f32);
        }

        // Draw the frame
        self.draw_frame(width, height)
    }

    fn on_mouse_down(&mut self, x: f32, y: f32) {
        // Add a new ball where the user clicked
        self.balls.push(BallState::new_at_position(x, y, 30.0));
    }
}

// -------------------------------------
// Ball Physics State
// -------------------------------------

struct BallState {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    radius: f32,
}

impl BallState {
    fn new(width: f32, height: f32, radius: f32) -> Self {
        Self {
            x: width / 2.0,
            y: height / 2.0,
            vx: 1.5,
            vy: 1.0,
            radius,
        }
    }

    fn new_at_position(x: f32, y: f32, radius: f32) -> Self {
        let vx = rand_float(-2.0, 2.0);
        let vy = rand_float(-2.0, 2.0);
        
        Self {
            x,
            y,
            vx,
            vy,
            radius,
        }
    }

    fn update(&mut self, width: f32, height: f32) {
        self.x += self.vx;
        self.y += self.vy;

        if self.x - self.radius < 0.0 || self.x + self.radius > width {
            self.vx *= -1.0;
        }
        if self.y - self.radius < 0.0 || self.y + self.radius > height {
            self.vy *= -1.0;
        }
    }
}

// Simple random number generator for velocity
fn rand_float(min: f32, max: f32) -> f32 {
    let random = js_sys::Math::random() as f32;
    min + random * (max - min)
}