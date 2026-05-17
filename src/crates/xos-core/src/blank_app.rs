use crate::engine::{Application, EngineState};
use crate::rasterizer::fill;

const BACKGROUND_COLOR: (u8, u8, u8) = (32, 32, 32);

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
        fill(
            &mut state.frame,
            (BACKGROUND_COLOR.0, BACKGROUND_COLOR.1, BACKGROUND_COLOR.2, 0xff),
        );
    }

    fn on_mouse_down(&mut self, _state: &mut EngineState) {}
    fn on_mouse_up(&mut self, _state: &mut EngineState) {}
    fn on_mouse_move(&mut self, _state: &mut EngineState) {}
}
