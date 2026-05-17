use xos_core::engine::{Application, EngineState};

pub struct CrashApp;

impl CrashApp {
    pub fn new() -> Self {
        Self
    }
}

impl Application for CrashApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        Ok(())
    }

    fn tick(&mut self, _state: &mut EngineState) {
        // Deliberately crash the app
        panic!("CrashApp: Deliberate crash for testing crash detection!");
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

