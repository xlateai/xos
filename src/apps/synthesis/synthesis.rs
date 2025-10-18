use crate::engine::{Application, EngineState};
use crate::apps::waveform::Waveform;

const BACKGROUND_COLOR: (u8, u8, u8) = (32, 32, 32); // Dark gray

pub struct SynthesisApp {
    waveform: Waveform,
}

impl SynthesisApp {
    pub fn new() -> Self {
        Self {
            waveform: Waveform::new(),
        }
    }
}

impl Application for SynthesisApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
    self.waveform.setup(_state)?;
    Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
    self.waveform.tick(state);
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