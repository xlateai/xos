use crate::engine::{Application, EngineState};
use crate::tuneable::write_all_to_source;
use crate::tuneables;
use crate::apps::text::geometric::GeometricText;
use fontdue::{Font, FontSettings};

tuneables! {
    square_x: f32 = 0.42148933;
    square_y: f32 = 0.5229107;
}

const BACKGROUND_COLOR: (u8, u8, u8) = (32, 32, 32);
const TEXT_COLOR: (u8, u8, u8) = (255, 255, 255);
const SQUARE_COLOR: (u8, u8, u8) = (64, 64, 64);
const SQUARE_WIDTH: f32 = 400.0;
const SQUARE_HEIGHT: f32 = 200.0;

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

    fn clamp_position(x: f32, y: f32, width: u32, height: u32) -> (f32, f32) {
        let half_w = SQUARE_WIDTH / (2.0 * width as f32);
        let half_h = SQUARE_HEIGHT / (2.0 * height as f32);
        let cx = x.clamp(half_w, 1.0 - half_w);
        let cy = y.clamp(half_h, 1.0 - half_h);
        (cx, cy)
    }

    fn get_absolute_xy(state: &EngineState) -> (f32, f32) {
        let norm_x = square_x().get();
        let norm_y = square_y().get();
        (
            norm_x * state.frame.width as f32,
            norm_y * state.frame.height as f32,
        )
    }

    fn draw_square(&self, buffer: &mut [u8], width: u32, height: u32, x: f32, y: f32) -> (i32, i32, u32, u32) {
        let half_w = SQUARE_WIDTH / 2.0;
        let half_h = SQUARE_HEIGHT / 2.0;
        let x0 = (x - half_w).max(0.0).round() as i32;
        let y0 = (y - half_h).max(0.0).round() as i32;
        let w = SQUARE_WIDTH.round() as u32;
        let h = SQUARE_HEIGHT.round() as u32;

        for dy in 0..h {
            for dx in 0..w {
                let sx = x0 + dx as i32;
                let sy = y0 + dy as i32;
                if sx >= 0 && sx < width as i32 && sy >= 0 && sy < height as i32 {
                    let idx = ((sy as u32 * width + sx as u32) * 4) as usize;
                    buffer[idx + 0] = SQUARE_COLOR.0;
                    buffer[idx + 1] = SQUARE_COLOR.1;
                    buffer[idx + 2] = SQUARE_COLOR.2;
                    buffer[idx + 3] = 0xff;
                }
            }
        }

        (x0, y0, w, h)
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

    fn is_inside(&self, x: f32, y: f32, mx: f32, my: f32) -> bool {
        let half_w = SQUARE_WIDTH / 2.0;
        let half_h = SQUARE_HEIGHT / 2.0;
        mx >= x - half_w && mx <= x + half_w && my >= y - half_h && my <= y + half_h
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

        let (abs_x, abs_y) = Self::get_absolute_xy(state);
        let (x0, y0, w, h) = self.draw_square(&mut state.frame.buffer, state.frame.width, state.frame.height, abs_x, abs_y);
        self.draw_text(state, x0, y0, w, h);
    }

    fn on_key_char(&mut self, _state: &mut EngineState, ch: char) {
        match ch {
            '\t' => self.text_engine.text.push_str("    "),
            '\n' | '\r' => self.text_engine.text.push('\n'),
            '\u{8}' => { self.text_engine.text.pop(); }
            _ if !ch.is_control() => self.text_engine.text.push(ch),
            _ => {}
        }
    }

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        let (x, y) = Self::get_absolute_xy(state);
        if self.is_inside(x, y, state.mouse.x, state.mouse.y) {
            self.dragging = true;
            self.drag_offset_x = state.mouse.x - x;
            self.drag_offset_y = state.mouse.y - y;
        }
    }

    fn on_mouse_up(&mut self, state: &mut EngineState) {
        if self.dragging {
            let raw_x = state.mouse.x - self.drag_offset_x;
            let raw_y = state.mouse.y - self.drag_offset_y;
            let norm_x = raw_x / state.frame.width as f32;
            let norm_y = raw_y / state.frame.height as f32;
            let (cx, cy) = Self::clamp_position(norm_x, norm_y, state.frame.width, state.frame.height);
            square_x().set(cx);
            square_y().set(cy);
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
            let (cx, cy) = Self::clamp_position(norm_x, norm_y, state.frame.width, state.frame.height);
            square_x().set(cx);
            square_y().set(cy);
        }
    }
}