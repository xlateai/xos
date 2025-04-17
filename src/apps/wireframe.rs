use crate::engine::{Application, EngineState};
use crate::tuneable::write_all_to_source;
use crate::tuneables;

tuneables! {
    square_x: f32 = 0.5;
    square_y: f32 = 0.5;
}

const BACKGROUND_COLOR: (u8, u8, u8) = (32, 32, 32);
const SQUARE_COLOR: (u8, u8, u8) = (255, 100, 100);
const SQUARE_SIZE: f32 = 100.0;

pub struct WireframeDemo {
    dragging: bool,
    drag_offset_x: f32,
    drag_offset_y: f32,
}

impl WireframeDemo {
    pub fn new() -> Self {
        Self {
            dragging: false,
            drag_offset_x: 0.0,
            drag_offset_y: 0.0,
        }
    }

    fn draw_square(&self, state: &mut EngineState, x: f32, y: f32) {
        let buffer = &mut state.frame.buffer;
        let width = state.frame.width as usize;
        let height = state.frame.height as usize;

        let half = SQUARE_SIZE / 2.0;
        let x0 = (x - half).max(0.0) as usize;
        let x1 = (x + half).min(width as f32) as usize;
        let y0 = (y - half).max(0.0) as usize;
        let y1 = (y + half).min(height as f32) as usize;

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

    fn clamp_position(x: f32, y: f32, width: u32, height: u32) -> (f32, f32) {
        let half_w = SQUARE_SIZE / (2.0 * width as f32);
        let half_h = SQUARE_SIZE / (2.0 * height as f32);

        let cx = x.clamp(half_w, 1.0 - half_w);
        let cy = y.clamp(half_h, 1.0 - half_h);
        (cx, cy)
    }

    fn get_absolute_xy(state: &EngineState) -> (f32, f32) {
        let norm_x = square_x().get();
        let norm_y = square_y().get();

        let abs_x = norm_x * state.frame.width as f32;
        let abs_y = norm_y * state.frame.height as f32;
        (abs_x, abs_y)
    }

    fn is_inside_square(&self, x: f32, y: f32, mx: f32, my: f32) -> bool {
        let h = SQUARE_SIZE / 2.0;
        mx >= x - h && mx <= x + h && my >= y - h && my <= y + h
    }
}

impl Application for WireframeDemo {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        for chunk in state.frame.buffer.chunks_exact_mut(4) {
            chunk[0] = BACKGROUND_COLOR.0;
            chunk[1] = BACKGROUND_COLOR.1;
            chunk[2] = BACKGROUND_COLOR.2;
            chunk[3] = 0xff;
        }

        let (abs_x, abs_y) = Self::get_absolute_xy(state);
        self.draw_square(state, abs_x, abs_y);
    }

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        let (abs_x, abs_y) = Self::get_absolute_xy(state);

        if self.is_inside_square(abs_x, abs_y, state.mouse.x, state.mouse.y) {
            self.dragging = true;
            self.drag_offset_x = state.mouse.x - abs_x;
            self.drag_offset_y = state.mouse.y - abs_y;
        }
    }

    fn on_mouse_up(&mut self, state: &mut EngineState) {
        if self.dragging {
            let raw_x = state.mouse.x - self.drag_offset_x;
            let raw_y = state.mouse.y - self.drag_offset_y;

            let norm_x = raw_x / state.frame.width as f32;
            let norm_y = raw_y / state.frame.height as f32;

            let (clamped_x, clamped_y) = Self::clamp_position(norm_x, norm_y, state.frame.width, state.frame.height);

            square_x().set(clamped_x);
            square_y().set(clamped_y);
            write_all_to_source();
        }

        self.dragging = false;
    }

    fn on_mouse_move(&mut self, state: &mut EngineState) {
        if self.dragging {
            let raw_x = state.mouse.x - self.drag_offset_x;
            let raw_y = state.mouse.y - self.drag_offset_y;

            let norm_x = raw_x / state.frame.width as f32;
            let norm_y = raw_y / state.frame.height as f32;

            let (clamped_x, clamped_y) = Self::clamp_position(norm_x, norm_y, state.frame.width, state.frame.height);

            square_x().set(clamped_x);
            square_y().set(clamped_y);
        }
    }
}
