use crate::engine::Application;

// Common background color - now black
const BACKGROUND_COLOR: (u8, u8, u8) = (0, 0, 0);
// Light gray ball color
const BALL_COLOR: (u8, u8, u8) = (200, 200, 200);
// Reduced ball radius
const BALL_RADIUS: f32 = 15.0;
// Increased ball speed multiplier (3.0 * 1.15 = 3.45)
const SPEED_MULTIPLIER: f32 = 3.45;

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

        // Fill background first (black)
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
                    // Light gray ball color
                    pixels[i + 0] = BALL_COLOR.0; // R
                    pixels[i + 1] = BALL_COLOR.1; // G
                    pixels[i + 2] = BALL_COLOR.2; // B
                    pixels[i + 3] = 0xff; // A
                }
            }
        }
    }
}

impl Application for BallGame {
    fn setup(&mut self, width: u32, height: u32) -> Result<(), String> {
        // Add initial ball at center with half the radius
        self.balls.push(BallState::new(width as f32, height as f32, BALL_RADIUS));
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
        // Add a new ball where the user clicked with half the radius
        self.balls.push(BallState::new_at_position(x, y, BALL_RADIUS));
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
            vx: 1.5 * SPEED_MULTIPLIER,
            vy: 1.0 * SPEED_MULTIPLIER,
            radius,
        }
    }

    fn new_at_position(x: f32, y: f32, radius: f32) -> Self {
        let vx = rand_float(-2.0, 2.0) * SPEED_MULTIPLIER;
        let vy = rand_float(-2.0, 2.0) * SPEED_MULTIPLIER;
        
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

        // Check if ball is completely off screen (rather than just touching the edge)
        let is_off_screen = 
            self.x + self.radius < 0.0 || 
            self.x - self.radius > width || 
            self.y + self.radius < 0.0 || 
            self.y - self.radius > height;

        if is_off_screen {
            // Respawn at center with the same heading
            self.x = width / 2.0;
            self.y = height / 2.0;
            // Keep the same direction by preserving velocity signs and magnitude
        } else {
            // Normal bounce logic for when ball is just touching the edge
            if self.x - self.radius < 0.0 {
                self.x = self.radius;
                self.vx = self.vx.abs();
            } else if self.x + self.radius > width {
                self.x = width - self.radius;
                self.vx = -self.vx.abs();
            }
            
            if self.y - self.radius < 0.0 {
                self.y = self.radius;
                self.vy = self.vy.abs();
            } else if self.y + self.radius > height {
                self.y = height - self.radius;
                self.vy = -self.vy.abs();
            }
        }
    }
}

fn rand_float(min: f32, max: f32) -> f32 {
    #[cfg(target_arch = "wasm32")]
    {
        let random = js_sys::Math::random() as f32;
        min + random * (max - min)
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        rng.gen_range(min..max)
    }
}