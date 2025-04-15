use crate::engine::{Application, EngineState};

const BACKGROUND_COLOR: (u8, u8, u8) = (32, 32, 32);
const DOT_COLOR: (u8, u8, u8) = (200, 200, 200);
const DOT_SPACING: u32 = 40;
const DOT_RADIUS: u32 = 2;

pub struct ScrollApp {
    scroll_x: f32,
    scroll_y: f32,
    dragging: bool,
    last_mouse_x: f32,
    last_mouse_y: f32,
}

impl ScrollApp {
    pub fn new() -> Self {
        Self {
            scroll_x: 0.0,
            scroll_y: 0.0,
            dragging: false,
            last_mouse_x: 0.0,
            last_mouse_y: 0.0,
        }
    }
}

impl Application for ScrollApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        let width = state.frame.width;
        let height = state.frame.height;
        let buffer = &mut state.frame.buffer;

        // Fill background
        for i in (0..buffer.len()).step_by(4) {
            buffer[i + 0] = BACKGROUND_COLOR.0;
            buffer[i + 1] = BACKGROUND_COLOR.1;
            buffer[i + 2] = BACKGROUND_COLOR.2;
            buffer[i + 3] = 0xff;
        }

        // Draw dots
        for y in 0..height {
            for x in 0..width {
                let world_x = x as f32 + self.scroll_x;
                let world_y = y as f32 + self.scroll_y;

                if (world_x.rem_euclid(DOT_SPACING as f32) - (DOT_SPACING / 2) as f32).abs() < DOT_RADIUS as f32 &&
                   (world_y.rem_euclid(DOT_SPACING as f32) - (DOT_SPACING / 2) as f32).abs() < DOT_RADIUS as f32 {
                    let i = ((y * width + x) * 4) as usize;
                    buffer[i + 0] = DOT_COLOR.0;
                    buffer[i + 1] = DOT_COLOR.1;
                    buffer[i + 2] = DOT_COLOR.2;
                    buffer[i + 3] = 0xff;
                }
            }
        }
    }

    fn on_scroll(&mut self, _state: &mut EngineState, dx: f32, dy: f32) {
        self.scroll_x += dx;
        self.scroll_y += dy;
    }

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        self.dragging = true;
        self.last_mouse_x = state.mouse.x;
        self.last_mouse_y = state.mouse.y;
    }

    fn on_mouse_up(&mut self, _state: &mut EngineState) {
        self.dragging = false;
    }

    fn on_mouse_move(&mut self, state: &mut EngineState) {
        if self.dragging {
            let x = state.mouse.x;
            let y = state.mouse.y;
            let dx = x - self.last_mouse_x;
            let dy = y - self.last_mouse_y;
            self.scroll_x -= dx;
            self.scroll_y -= dy;
            self.last_mouse_x = x;
            self.last_mouse_y = y;
        }
    }
}
