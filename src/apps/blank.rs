use crate::engine::{Application, EngineState};

const BACKGROUND_COLOR: (u8, u8, u8) = (32, 32, 32); // Dark gray

pub struct BlankApp;

impl BlankApp {
    pub fn new() -> Self {
        Self
    }
}

impl Application for BlankApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        // Change this line:
        // From: let mut buffer = &state.frame.buffer;
        // To: let buffer = &mut state.frame.buffer;
        let buffer = &mut state.frame.buffer;
        let len = buffer.len();

        for i in (0..len).step_by(4) {
            buffer[i + 0] = BACKGROUND_COLOR.0;
            buffer[i + 1] = BACKGROUND_COLOR.1;
            buffer[i + 2] = BACKGROUND_COLOR.2;
            buffer[i + 3] = 0xff;
        }
    }

    fn on_mouse_down(&mut self, _state: &mut EngineState) {
        // No interaction
    }
    
    fn on_mouse_up(&mut self, _state: &mut EngineState) {
        // No interaction
    }
    
    fn on_mouse_move(&mut self, _state: &mut EngineState) {
        // No interaction
    }
}