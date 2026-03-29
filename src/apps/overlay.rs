//! Transparent overlay text editor: same behavior as [`crate::apps::text::TextApp`], hosted in a
//! top-left HUD window (see [`crate::engine::start_overlay_native`]).

use crate::apps::text::TextApp;
use crate::engine::{Application, EngineState};
use crate::keyboard::shortcuts::ShortcutAction;
use crate::rasterizer::fill_rect_buffer;

const NEON: (u8, u8, u8) = (57, 255, 20);
/// Stroke width in pixels for the window outline (drawn after text).
const FRAME_STROKE: i32 = 3;

pub struct OverlayApp {
    text: TextApp,
}

impl OverlayApp {
    pub fn new() -> Self {
        Self {
            text: TextApp::new_for_overlay(),
        }
    }

    fn draw_neon_frame(buffer: &mut [u8], fw: usize, fh: usize) {
        let w = fw as i32;
        let h = fh as i32;
        let t = FRAME_STROKE.min(w).min(h).max(1);
        let c = (NEON.0, NEON.1, NEON.2, 255);
        fill_rect_buffer(buffer, fw, fh, 0, 0, w, t, c);
        fill_rect_buffer(buffer, fw, fh, 0, h - t, w, h, c);
        fill_rect_buffer(buffer, fw, fh, 0, t, t, h - t, c);
        fill_rect_buffer(buffer, fw, fh, w - t, t, w, h - t, c);
    }
}

impl Application for OverlayApp {
    fn setup(&mut self, state: &mut EngineState) -> Result<(), String> {
        self.text.setup(state)
    }

    fn tick(&mut self, state: &mut EngineState) {
        self.text.tick(state);
        let shape = state.frame.array.shape();
        let fw = shape[1];
        let fh = shape[0];
        let buffer = state.frame_buffer_mut();
        Self::draw_neon_frame(buffer, fw, fh);
    }

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        self.text.on_mouse_down(state);
    }

    fn on_mouse_up(&mut self, state: &mut EngineState) {
        self.text.on_mouse_up(state);
    }

    fn on_mouse_move(&mut self, state: &mut EngineState) {
        self.text.on_mouse_move(state);
    }

    fn on_scroll(&mut self, state: &mut EngineState, dx: f32, dy: f32) {
        self.text.on_scroll(state, dx, dy);
    }

    fn on_key_char(&mut self, state: &mut EngineState, ch: char) {
        self.text.on_key_char(state, ch);
    }

    fn on_key_shortcut(&mut self, state: &mut EngineState, shortcut: ShortcutAction) {
        self.text.on_key_shortcut(state, shortcut);
    }

    fn on_screen_size_change(&mut self, state: &mut EngineState, width: u32, height: u32) {
        self.text.on_screen_size_change(state, width, height);
    }
}
