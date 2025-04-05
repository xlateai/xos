use crate::engine::{Application, EngineState};

const BACKGROUND_COLOR: (u8, u8, u8) = (32, 32, 32); // Dark gray

pub struct BlankApp;

impl BlankApp {
    pub fn new() -> Self {
        Self
    }

    fn draw_blank_frame(&self, width: u32, height: u32) -> Vec<u8> {
        let mut pixels = vec![0u8; (width * height * 4) as usize];

        for i in (0..pixels.len()).step_by(4) {
            pixels[i + 0] = BACKGROUND_COLOR.0;
            pixels[i + 1] = BACKGROUND_COLOR.1;
            pixels[i + 2] = BACKGROUND_COLOR.2;
            pixels[i + 3] = 0xff;
        }

        pixels
    }
}

impl Application for BlankApp {
    fn setup(&mut self, _state: &EngineState) -> Result<(), String> {
        Ok(())
    }

    fn tick(&mut self, state: &EngineState) -> Vec<u8> {
        self.draw_blank_frame(state.frame.width, state.frame.height)
    }

    fn on_mouse_down(&mut self, _x: f32, _y: f32) {
        // No interaction
    }
}
