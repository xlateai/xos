use crate::engine::{Application, EngineState};
use crate::rasterizer::{circles, fill};

const BALL_COLOR: (u8, u8, u8) = (200, 50, 200);
const BALL_RADIUS: f32 = 15.0;
const SPEED_MULTIPLIER: f32 = 3.45;
/// Original movement was per-frame at ~60 Hz; scale random [-2,2]*multiplier to px/s.
const REF_FPS: f32 = 60.0;

struct BallState {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    radius: f32,
}

impl BallState {
    fn new_at_position(x: f32, y: f32, radius: f32) -> Self {
        let vx = rand_float(-2.0, 2.0) * SPEED_MULTIPLIER * REF_FPS;
        let vy = rand_float(-2.0, 2.0) * SPEED_MULTIPLIER * REF_FPS;
        
        Self {
            x,
            y,
            vx,
            vy,
            radius,
        }
    }

    fn update(&mut self, width: f32, height: f32, dt: f32) {
        self.x += self.vx * dt;
        self.y += self.vy * dt;

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
        let mut rng = rand::rng();
        rng.random_range(min..max)
    }
}

pub struct BallGame {
    balls: Vec<BallState>,
}

impl BallGame {
    pub fn new() -> Self {
        Self { balls: Vec::new() }
    }
}

impl Application for BallGame {
    fn setup(&mut self, state: &mut EngineState) -> Result<(), String> {
        let width = state.frame.shape()[1] as f32;
        let height = state.frame.shape()[0] as f32;
        
        for _ in 0..512 {
            let x = rand_float(BALL_RADIUS, width - BALL_RADIUS);
            let y = rand_float(BALL_RADIUS, height - BALL_RADIUS);
            self.balls.push(BallState::new_at_position(x, y, BALL_RADIUS));
        }
        crate::print("+512 balls (initial spawn)");
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        // Clear the frame (no longer auto-cleared)
        fill(&mut state.frame, (0, 0, 0, 255));
        
        for ball in &mut self.balls {
            ball.update(
                state.frame.shape()[1] as f32,
                state.frame.shape()[0] as f32,
                state.delta_time_seconds,
            );
        }

        let centers: Vec<(f32, f32)> = self.balls.iter().map(|b| (b.x, b.y)).collect();
        let radii: Vec<f32> = self.balls.iter().map(|b| b.radius).collect();
        let rgba = [BALL_COLOR.0, BALL_COLOR.1, BALL_COLOR.2, 255u8];
        let colors: Vec<[u8; 4]> = vec![rgba; self.balls.len()];
        let _ = circles(&mut state.frame, &centers, &radii, &colors);
    }

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        self.balls.push(BallState::new_at_position(state.mouse.x, state.mouse.y, BALL_RADIUS));
        crate::print("+1 ball (click spawn)");
    }
}
