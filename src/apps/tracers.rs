use crate::engine::{Application, EngineState};
use std::collections::VecDeque;

const PARTICLE_COLOR: (u8, u8, u8) = (200, 200, 200);
const PARTICLE_RADIUS: f32 = 4.0;
const SPEED_MULTIPLIER: f32 = 3.45;
const PARTICLE_COUNT: usize = 256;
const TRAIL_DURATION_SECONDS: f32 = 2.0;
const FRAME_RATE: f32 = 60.0;
const TRAIL_LENGTH: usize = (TRAIL_DURATION_SECONDS * FRAME_RATE) as usize;


fn draw_circle(
    buffer: &mut Vec<u8>,
    width: u32,
    height: u32,
    cx: f32,
    cy: f32,
    radius: f32,
    alpha: u8,
) {
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
                    buffer[i + 0] = PARTICLE_COLOR.0;
                    buffer[i + 1] = PARTICLE_COLOR.1;
                    buffer[i + 2] = PARTICLE_COLOR.2;
                    buffer[i + 3] = alpha;
                }
            }
        }
    }
}


pub struct TracersApp {
    particles: Vec<Particle>,
}

impl TracersApp {
    pub fn new() -> Self {
        Self {
            particles: Vec::new(),
        }
    }
}

impl Application for TracersApp {
    fn setup(&mut self, state: &mut EngineState) -> Result<(), String> {
        for _ in 0..PARTICLE_COUNT {
            self.particles.push(Particle::new(
                state.frame.width as f32,
                state.frame.height as f32,
            ));
        }
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        let width = state.frame.width;
        let height = state.frame.height;

        for particle in &mut self.particles {
            particle.update(width as f32, height as f32);

            for (i, &(x, y)) in particle.trail.iter().enumerate() {
                let alpha = ((i + 1) as f32 / particle.trail.len() as f32 * 255.0) as u8;
                draw_circle(&mut state.frame.buffer, width, height, x, y, PARTICLE_RADIUS, alpha);
            }
        }
    }

}

struct Particle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    trail: VecDeque<(f32, f32)>,
}

impl Particle {
    fn new(width: f32, height: f32) -> Self {
        let x = rand_float(0.0, width);
        let y = rand_float(0.0, height);
        let vx = rand_float(-2.0, 2.0) * SPEED_MULTIPLIER;
        let vy = rand_float(-2.0, 2.0) * SPEED_MULTIPLIER;
        Self {
            x,
            y,
            vx,
            vy,
            trail: VecDeque::with_capacity(TRAIL_LENGTH),
        }
    }

    fn update(&mut self, width: f32, height: f32) {
        self.x += self.vx;
        self.y += self.vy;

        if self.x < 0.0 || self.x > width {
            self.vx *= -1.0;
        }
        if self.y < 0.0 || self.y > height {
            self.vy *= -1.0;
        }

        self.trail.push_back((self.x, self.y));
        if self.trail.len() > TRAIL_LENGTH {
            self.trail.pop_front();
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
