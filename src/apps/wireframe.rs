use crate::engine::{Application, EngineState};
use crate::tuneable::{Tuneable, tuneable};
use std::sync::{Mutex, OnceLock};

const BACKGROUND_COLOR: (u8, u8, u8) = (32, 32, 32);
const SQUARE_COLOR: (u8, u8, u8) = (255, 100, 100);
const SQUARE_SIZE: f32 = 100.0;

static SQUARE_ORIGIN: OnceLock<Mutex<(Tuneable<f32>, Tuneable<f32>)>> = OnceLock::new();

pub fn init_square_origin() {
    SQUARE_ORIGIN.get_or_init(|| {
        Mutex::new((
            tuneable!("square_x" => 320.0),
            tuneable!("square_y" => 240.0),
        ))
    });
}

pub fn get_square_origin() -> (f32, f32) {
    let origin = SQUARE_ORIGIN.get().expect("not initialized").lock().unwrap();
    (origin.0.get(), origin.1.get())
}

pub fn set_square_origin(new_x: f32, new_y: f32) {
    let mut origin = SQUARE_ORIGIN.get().expect("not initialized").lock().unwrap();
    origin.0.set(new_x);
    origin.1.set(new_y);
    Tuneable::try_write_update_pair(&origin.0, &origin.1, new_x, new_y);
}


pub struct WireframeDemo {
    dragging: bool,
    drag_offset_x: f32,
    drag_offset_y: f32,
}

impl WireframeDemo {
    pub fn new() -> Self {
        init_square_origin();
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
        let buffer = &mut state.frame.buffer;
        for chunk in buffer.chunks_exact_mut(4) {
            chunk[0] = BACKGROUND_COLOR.0;
            chunk[1] = BACKGROUND_COLOR.1;
            chunk[2] = BACKGROUND_COLOR.2;
            chunk[3] = 0xff;
        }

        let (x, y) = get_square_origin();
        self.draw_square(state, x, y);
    }

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        let (x, y) = get_square_origin();

        if self.is_inside_square(x, y, state.mouse.x, state.mouse.y) {
            self.dragging = true;
            self.drag_offset_x = state.mouse.x - x;
            self.drag_offset_y = state.mouse.y - y;
        }
    }

    fn on_mouse_up(&mut self, state: &mut EngineState) {
        if self.dragging {
            let new_x = state.mouse.x - self.drag_offset_x;
            let new_y = state.mouse.y - self.drag_offset_y;
            set_square_origin(new_x, new_y);
        }
        self.dragging = false;
    }

    fn on_mouse_move(&mut self, state: &mut EngineState) {
        if self.dragging {
            let new_x = state.mouse.x - self.drag_offset_x;
            let new_y = state.mouse.y - self.drag_offset_y;
            set_square_origin(new_x, new_y);
        }
    }
}