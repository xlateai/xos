use crate::engine::{Application, EngineState};

// Light gray ball color
const BALL_COLOR: (u8, u8, u8) = (200, 200, 200);
const BALL_RADIUS: f32 = 15.0;
const SPEED_MULTIPLIER: f32 = 3.45;

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

pub struct BallGame {
    balls: Vec<BallState>,
}

impl BallGame {
    pub fn new() -> Self {
        Self { balls: Vec::new() }
    }

    fn draw_circle(
        &self,
        state: &mut EngineState,
        cx: f32,
        cy: f32,
        radius: f32,
    ) {
        let buffer = &mut state.frame.buffer;
        
        let width = state.frame.width;
        let height = state.frame.height;
        let radius_squared = radius * radius;

        let start_x = (cx - radius).max(0.0) as u32;
        let end_x = (cx + radius + 1.0).min(width as f32) as u32;
        let start_y = (cy - radius).max(0.0) as u32;
        let end_y = (cy + radius + 1.0).min(height as f32) as u32;

        for y in start_y..end_y {
            for x in start_x..end_x {
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                if dx * dx + dy * dy <= radius_squared {
                    let i = ((y * width + x) * 4) as usize;
                    if i + 3 < buffer.len() {
                        buffer[i + 0] = BALL_COLOR.0;
                        buffer[i + 1] = BALL_COLOR.1;
                        buffer[i + 2] = BALL_COLOR.2;
                        buffer[i + 3] = 0xff;
                    }
                }
            }
        }
    }
}

impl Application for BallGame {
    fn setup(&mut self, state: &mut EngineState) -> Result<(), String> {
        self.balls
            .push(BallState::new(state.frame.width as f32, state.frame.height as f32, BALL_RADIUS));
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        for ball in &mut self.balls {
            ball.update(state.frame.width as f32, state.frame.height as f32);
        }

        for ball in &self.balls {
            self.draw_circle(
                state,
                ball.x,
                ball.y,
                ball.radius,
            );
        }
    }

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        self.balls.push(BallState::new_at_position(state.mouse.x, state.mouse.y, BALL_RADIUS));
    }
}
