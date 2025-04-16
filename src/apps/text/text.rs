use crate::engine::{Application, EngineState};
use crate::apps::text::geometric::GeometricText;
use fontdue::{Font, FontSettings};

const BACKGROUND_COLOR: (u8, u8, u8) = (0, 0, 0);
const TEXT_COLOR: (u8, u8, u8) = (255, 255, 255);
const CURSOR_COLOR: (u8, u8, u8) = (0, 255, 0);
const BOUND_COLOR: (u8, u8, u8) = (255, 0, 0);

const SHOW_BOUNDING_RECTANGLES: bool = true;

pub struct TextApp {
    pub text_engine: GeometricText,
    pub cursor_index: usize,
    pub scroll_y: f32,
}

impl TextApp {
    pub fn new() -> Self {
        let font_bytes = include_bytes!("../../../assets/JetBrainsMono-Regular.ttf") as &[u8];
        let font = Font::from_bytes(font_bytes, FontSettings::default()).expect("Failed to load font");

        let mut text_engine = GeometricText::new(font, 48.0);
        // text_engine.set_text("hello world!".to_string());

        Self {
            text_engine,
            cursor_index: 0,
            scroll_y: 0.0,
        }
    }

    fn draw_rect(buffer: &mut [u8], width: u32, height: u32, x: i32, y: i32, w: u32, h: u32) {
        if x < 0 || y < 0 || w == 0 || h == 0 {
            return;
        }
        let x = x as u32;
        let y = y as u32;

        let mut draw_pixel = |x, y| {
            if x < width && y < height {
                let idx = ((y * width + x) * 4) as usize;
                buffer[idx + 0] = BOUND_COLOR.0;
                buffer[idx + 1] = BOUND_COLOR.1;
                buffer[idx + 2] = BOUND_COLOR.2;
                buffer[idx + 3] = 0xff;
            }
        };

        for dx in 0..w {
            draw_pixel(x + dx, y);
            draw_pixel(x + dx, y + h.saturating_sub(1));
        }
        for dy in 0..h {
            draw_pixel(x, y + dy);
            draw_pixel(x + w.saturating_sub(1), y + dy);
        }
    }
}

impl Application for TextApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        let width = state.frame.width as f32;
        let height = state.frame.height as f32;
        let buffer = &mut state.frame.buffer;

        self.text_engine.tick(width, height);

        for i in (0..buffer.len()).step_by(4) {
            buffer[i + 0] = BACKGROUND_COLOR.0;
            buffer[i + 1] = BACKGROUND_COLOR.1;
            buffer[i + 2] = BACKGROUND_COLOR.2;
            buffer[i + 3] = 0xff;
        }

        for character in &self.text_engine.characters {
            let px = character.x as i32;
            let py = (character.y - self.scroll_y) as i32;
            let pw = character.width as u32;
            let ph = character.height as u32;

            for y in 0..character.metrics.height {
                for x in 0..character.metrics.width {
                    let val = character.bitmap[y * character.metrics.width + x];
                    let sx = px + x as i32;
                    let sy = py + y as i32;

                    if sx >= 0 && sx < width as i32 && sy >= 0 && sy < height as i32 {
                        let idx = ((sy as u32 * width as u32 + sx as u32) * 4) as usize;
                        buffer[idx + 0] = ((TEXT_COLOR.0 as u16 * val as u16) / 255) as u8;
                        buffer[idx + 1] = ((TEXT_COLOR.1 as u16 * val as u16) / 255) as u8;
                        buffer[idx + 2] = ((TEXT_COLOR.2 as u16 * val as u16) / 255) as u8;
                        buffer[idx + 3] = 0xff;
                    }
                }
            }

            if SHOW_BOUNDING_RECTANGLES {
                Self::draw_rect(buffer, width as u32, height as u32, px, py, pw, ph);
            }
        }

        let (cx, cy, ch) = if let Some(c) = self.text_engine.characters.get(self.cursor_index) {
            (c.x, c.y, c.height)
        } else if let Some(last) = self.text_engine.characters.last() {
            (last.x + last.metrics.advance_width, last.y, last.height)
        } else {
            (0.0, self.text_engine.ascent, self.text_engine.ascent + self.text_engine.descent)
        };
        

        let cx = cx as u32;
        let cy = (cy - self.scroll_y) as u32;
        let h = ch as u32;

        for y in 0..h {
            let idx = ((cy + y) * width as u32 + cx) as usize * 4;
            if idx + 3 < buffer.len() {
                buffer[idx + 0] = CURSOR_COLOR.0;
                buffer[idx + 1] = CURSOR_COLOR.1;
                buffer[idx + 2] = CURSOR_COLOR.2;
                buffer[idx + 3] = 0xff;
            }
        }
    }

    fn on_scroll(&mut self, _state: &mut EngineState, _dx: f32, dy: f32) {
        self.scroll_y -= dy * 20.0;
        self.scroll_y = self.scroll_y.max(0.0);
    }

    fn on_key_char(&mut self, _state: &mut EngineState, ch: char) {
        if ch.is_control() {
            return;
        }
    
        let mut text = self.text_engine.text.clone();
        text.insert(self.cursor_index, ch);
        self.cursor_index += 1;
    
        self.text_engine.set_text(text);
    
        // fix: cursor_index can be == len, but that's not a valid index for .get()
        if self.cursor_index > self.text_engine.characters.len() {
            self.cursor_index = self.text_engine.characters.len(); // allow one-past-end
        }
    }
    
}
