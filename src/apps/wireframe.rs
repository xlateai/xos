use crate::engine::{Application, EngineState};

const BACKGROUND_COLOR: (u8, u8, u8) = (32, 32, 32); // Dark gray
const SQUARE_COLOR: (u8, u8, u8) = (255, 100, 100); // Light red
const SQUARE_SIZE: f32 = 100.0;

pub struct WireframeDemo {
    square_x: f32,
    square_y: f32,
    dragging: bool,
    drag_offset_x: f32,
    drag_offset_y: f32,
}

impl WireframeDemo {
    pub fn new() -> Self {
        Self {
            square_x: 0.0,
            square_y: 0.0,
            dragging: false,
            drag_offset_x: 0.0,
            drag_offset_y: 0.0,
        }
    }

    fn draw_square(&self, state: &mut EngineState) {
        let buffer = &mut state.frame.buffer;
        let width = state.frame.width as usize;
        let height = state.frame.height as usize;

        let half_size = SQUARE_SIZE / 2.0;
        let x0 = (self.square_x - half_size).max(0.0) as usize;
        let x1 = (self.square_x + half_size).min(width as f32) as usize;
        let y0 = (self.square_y - half_size).max(0.0) as usize;
        let y1 = (self.square_y + half_size).min(height as f32) as usize;

        for y in y0..y1 {
            for x in x0..x1 {
                let i = (y * width + x) * 4;
                if i + 3 >= buffer.len() { continue; }

                buffer[i + 0] = SQUARE_COLOR.0;
                buffer[i + 1] = SQUARE_COLOR.1;
                buffer[i + 2] = SQUARE_COLOR.2;
                buffer[i + 3] = 0xff;
            }
        }
    }

    fn is_inside_square(&self, mouse_x: f32, mouse_y: f32) -> bool {
        let half_size = SQUARE_SIZE / 2.0;
        mouse_x >= self.square_x - half_size &&
        mouse_x <= self.square_x + half_size &&
        mouse_y >= self.square_y - half_size &&
        mouse_y <= self.square_y + half_size
    }
}

impl Application for WireframeDemo {
    fn setup(&mut self, state: &mut EngineState) -> Result<(), String> {
        // Center the square in the middle of the screen
        self.square_x = state.frame.width as f32 / 2.0;
        self.square_y = state.frame.height as f32 / 2.0;
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        let buffer = &mut state.frame.buffer;
        let len = buffer.len();

        // Fill background
        for i in (0..len).step_by(4) {
            buffer[i + 0] = BACKGROUND_COLOR.0;
            buffer[i + 1] = BACKGROUND_COLOR.1;
            buffer[i + 2] = BACKGROUND_COLOR.2;
            buffer[i + 3] = 0xff;
        }

        // Draw square
        self.draw_square(state);
    }

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        let mx = state.mouse.x;
        let my = state.mouse.y;

        if self.is_inside_square(mx, my) {
            self.dragging = true;
            self.drag_offset_x = mx - self.square_x;
            self.drag_offset_y = my - self.square_y;
        }
    }

    fn on_mouse_up(&mut self, _state: &mut EngineState) {
        self.dragging = false;
    }

    fn on_mouse_move(&mut self, state: &mut EngineState) {
        if self.dragging {
            let mx = state.mouse.x;
            let my = state.mouse.y;
            self.square_x = mx - self.drag_offset_x;
            self.square_y = my - self.drag_offset_y;
        }
    }
}
