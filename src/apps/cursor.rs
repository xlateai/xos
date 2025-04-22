use crate::engine::{Application, EngineState};

const BACKGROUND_COLOR: (u8, u8, u8) = (32, 32, 32); // Dark gray
const CURSOR_COLOR: (u8, u8, u8) = (255, 255, 255); // White

pub struct CursorApp;

impl CursorApp {
    pub fn new() -> Self {
        Self
    }
}

impl Application for CursorApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        // state.mouse.style.hidden();

        let buffer = &mut state.frame.buffer;
        let width = state.frame.width;
        let height = state.frame.height;

        // Fill background
        for i in (0..buffer.len()).step_by(4) {
            buffer[i + 0] = BACKGROUND_COLOR.0;
            buffer[i + 1] = BACKGROUND_COLOR.1;
            buffer[i + 2] = BACKGROUND_COLOR.2;
            buffer[i + 3] = 0xff;
        }

        // Draw white dot at mouse location
        let mx = state.mouse.x.round() as i32;
        let my = state.mouse.y.round() as i32;
        let radius = 2;

        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy <= radius * radius {
                    let px = mx + dx;
                    let py = my + dy;

                    if px >= 0 && py >= 0 && px < width as i32 && py < height as i32 {
                        let idx = ((py as u32 * width + px as u32) * 4) as usize;
                        buffer[idx + 0] = CURSOR_COLOR.0;
                        buffer[idx + 1] = CURSOR_COLOR.1;
                        buffer[idx + 2] = CURSOR_COLOR.2;
                        buffer[idx + 3] = 0xff;
                    }
                }
            }
        }
    }

    fn on_mouse_down(&mut self, _state: &mut EngineState) {}
    fn on_mouse_up(&mut self, _state: &mut EngineState) {}
    fn on_mouse_move(&mut self, _state: &mut EngineState) {}
}
