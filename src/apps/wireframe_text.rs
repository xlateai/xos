use crate::engine::{Application, EngineState};
use crate::tuneable::write_all_to_source;
use crate::tuneables;
use crate::apps::text::geometric::GeometricText;
use fontdue::{Font, FontSettings};

tuneables! {
    square_x: f32 = 0.5;
    square_y: f32 = 0.5;
    left_edge: f32 = 0.2;
    right_edge: f32 = 0.8;
    top_edge: f32 = 0.3;
    bottom_edge: f32 = 0.7;
}

const BACKGROUND_COLOR: (u8, u8, u8) = (32, 32, 32);
const TEXT_COLOR: (u8, u8, u8) = (255, 255, 255);
const SQUARE_COLOR: (u8, u8, u8) = (64, 64, 64);

pub struct WireframeText {
    pub text_engine: GeometricText,
    dragging: bool,
    drag_offset_x: f32,
    drag_offset_y: f32,
}

impl WireframeText {
    pub fn new() -> Self {
        let font_bytes = include_bytes!("../../assets/JetBrainsMono-Regular.ttf") as &[u8];
        let font = Font::from_bytes(font_bytes, FontSettings::default()).unwrap();
        let mut text_engine = GeometricText::new(font, 24.0);
        text_engine.set_text("start typing...".to_string());

        Self {
            text_engine,
            dragging: false,
            drag_offset_x: 0.0,
            drag_offset_y: 0.0,
        }
    }

    fn draw_square_from_edges(
        &self,
        buffer: &mut [u8],
        width: u32,
        height: u32,
    ) -> (i32, i32, u32, u32) {
        let w = width as f32;
        let h = height as f32;

        let x0 = (left_edge().get() * w).round().clamp(0.0, w) as i32;
        let x1 = (right_edge().get() * w).round().clamp(0.0, w) as i32;
        let y0 = (top_edge().get() * h).round().clamp(0.0, h) as i32;
        let y1 = (bottom_edge().get() * h).round().clamp(0.0, h) as i32;

        let rect_w = (x1 - x0).max(0) as u32;
        let rect_h = (y1 - y0).max(0) as u32;

        for dy in 0..rect_h {
            for dx in 0..rect_w {
                let sx = x0 + dx as i32;
                let sy = y0 + dy as i32;

                if sx >= 0 && sy >= 0 && (sx as u32) < width && (sy as u32) < height {
                    let idx = ((sy as u32 * width + sx as u32) * 4) as usize;
                    buffer[idx + 0] = SQUARE_COLOR.0;
                    buffer[idx + 1] = SQUARE_COLOR.1;
                    buffer[idx + 2] = SQUARE_COLOR.2;
                    buffer[idx + 3] = 0xff;
                }
            }
        }

        (x0, y0, rect_w, rect_h)
    }

    fn draw_text(&mut self, state: &mut EngineState, tx: i32, ty: i32, tw: u32, th: u32) {
        let buffer = &mut state.frame.buffer;
        let width = state.frame.width;
        let height = state.frame.height;
        self.text_engine.tick(tw as f32, th as f32);

        for character in &self.text_engine.characters {
            let px = tx + character.x as i32;
            let py = ty + character.y as i32;

            for y in 0..character.metrics.height {
                for x in 0..character.metrics.width {
                    let val = character.bitmap[y * character.metrics.width + x];

                    if val == 0 {
                        continue;
                    }

                    let sx = px + x as i32;
                    let sy = py + y as i32;

                    if sx >= 0 && sx < width as i32 && sy >= 0 && sy < height as i32 {
                        let idx = ((sy as u32 * width + sx as u32) * 4) as usize;
                        buffer[idx + 0] = ((TEXT_COLOR.0 as u16 * val as u16) / 255) as u8;
                        buffer[idx + 1] = ((TEXT_COLOR.1 as u16 * val as u16) / 255) as u8;
                        buffer[idx + 2] = ((TEXT_COLOR.2 as u16 * val as u16) / 255) as u8;
                        buffer[idx + 3] = val;
                    }
                }
            }
        }
    }
}

impl Application for WireframeText {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        state.frame.buffer.chunks_exact_mut(4).for_each(|p| {
            p[0] = BACKGROUND_COLOR.0;
            p[1] = BACKGROUND_COLOR.1;
            p[2] = BACKGROUND_COLOR.2;
            p[3] = 0xff;
        });

        let (x0, y0, w, h) = self.draw_square_from_edges(&mut state.frame.buffer, state.frame.width, state.frame.height);
        self.draw_text(state, x0, y0, w, h);
    }

    fn on_mouse_move(&mut self, state: &mut EngineState) {
        let mx = state.mouse.x;
        let my = state.mouse.y;
        let w = state.frame.width as f32;
        let h = state.frame.height as f32;

        let left = left_edge().get() * w;
        let right = right_edge().get() * w;
        let top = top_edge().get() * h;
        let bottom = bottom_edge().get() * h;

        let near = 8.0;

        if !self.dragging {
            let near_left = (mx - left).abs() <= near;
            let near_right = (mx - right).abs() <= near;
            let near_top = (my - top).abs() <= near;
            let near_bottom = (my - bottom).abs() <= near;

            if near_left || near_right {
                state.mouse.style.resize_horizontal();
            } else if near_top || near_bottom {
                state.mouse.style.resize_vertical();
            } else {
                state.mouse.style.default();
            }
        }
    }

    fn on_key_char(&mut self, _state: &mut EngineState, ch: char) {
        match ch {
            '\t' => self.text_engine.text.push_str("    "),
            '\n' | '\r' => self.text_engine.text.push('\n'),
            '\u{8}' => {
                self.text_engine.text.pop();
            }
            _ if !ch.is_control() => self.text_engine.text.push(ch),
            _ => {}
        }
    }

    fn on_mouse_down(&mut self, _state: &mut EngineState) {
        self.dragging = true;
    }

    fn on_mouse_up(&mut self, _state: &mut EngineState) {
        self.dragging = false;
    }
}