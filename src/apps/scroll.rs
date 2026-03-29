use crate::engine::{Application, EngineState};
use crate::rasterizer::fill;

const BACKGROUND_COLOR: (u8, u8, u8) = (32, 32, 32);
const DOT_COLOR: (u8, u8, u8) = (200, 200, 200);
const DOT_SPACING: f32 = 40.0;
/// 2×2 px per grid point — same visual weight as the old r≈2 circles, ~50× fewer pixel writes.
const DOT_SIDE: i32 = 2;

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
        let shape = state.frame.array.shape();
        let width = shape[1] as f32;
        let height = shape[0] as f32;
        let fw = shape[1] as usize;
        let fh = shape[0] as usize;

        fill(
            &mut state.frame,
            (BACKGROUND_COLOR.0, BACKGROUND_COLOR.1, BACKGROUND_COLOR.2, 0xff),
        );

        let half = DOT_SPACING * 0.5;
        let kx0 = ((self.scroll_x - half) / DOT_SPACING).floor() as i32;
        let kx1 = ((self.scroll_x + width - half) / DOT_SPACING).ceil() as i32;
        let ky0 = ((self.scroll_y - half) / DOT_SPACING).floor() as i32;
        let ky1 = ((self.scroll_y + height - half) / DOT_SPACING).ceil() as i32;

        let buf = state.frame.buffer_mut();
        let px = [DOT_COLOR.0, DOT_COLOR.1, DOT_COLOR.2, 0xff];

        for kx in kx0..=kx1 {
            for ky in ky0..=ky1 {
                let cx = half + kx as f32 * DOT_SPACING;
                let cy = half + ky as f32 * DOT_SPACING;
                let sx = cx - self.scroll_x;
                let sy = cy - self.scroll_y;
                if sx < 0.0 || sx >= width || sy < 0.0 || sy >= height {
                    continue;
                }
                let x0 = sx.floor() as i32;
                let y0 = sy.floor() as i32;
                for dy in 0..DOT_SIDE {
                    for dx in 0..DOT_SIDE {
                        let px_i = x0 + dx;
                        let py_i = y0 + dy;
                        if px_i < 0 || py_i < 0 {
                            continue;
                        }
                        let px_u = px_i as usize;
                        let py_u = py_i as usize;
                        if px_u >= fw || py_u >= fh {
                            continue;
                        }
                        let idx = (py_u * fw + px_u) * 4;
                        buf[idx..idx + 4].copy_from_slice(&px);
                    }
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
