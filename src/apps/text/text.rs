use crate::engine::{Application, EngineState};
use crate::apps::text::geometric::GeometricText;
use fontdue::{Font, FontSettings};

const BACKGROUND_COLOR: (u8, u8, u8) = (0, 0, 0);
const TEXT_COLOR: (u8, u8, u8) = (255, 255, 255);
const CURSOR_COLOR: (u8, u8, u8) = (0, 255, 0);
const BOUND_COLOR: (u8, u8, u8) = (255, 0, 0);
const BASELINE_COLOR: (u8, u8, u8) = (100, 100, 100);

const SHOW_BOUNDING_RECTANGLES: bool = true;

pub struct TextApp {
    pub text_engine: GeometricText,
    pub scroll_y: f32,
    pub smooth_cursor_x: f32, // ← NEW
}


impl TextApp {
    pub fn new() -> Self {
        let font_bytes = include_bytes!("../../../assets/JetBrainsMono-Regular.ttf") as &[u8];
        // let font_bytes = include_bytes!("../../../assets/NotoSansJP-Regular.ttf") as &[u8];
        let font = Font::from_bytes(font_bytes, FontSettings::default()).expect("Failed to load font");

        let text_engine = GeometricText::new(font, 48.0);

        // huge TODO: multi-language support lmao
        // text_engine.set_text("こんにちは、今日は大丈夫です".to_string());

        Self {
            text_engine,
            scroll_y: 0.0,
            smooth_cursor_x: 0.0,
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

    fn tick_cursor(&mut self, state: &mut EngineState) {
        let (target_x, baseline_y) = if let Some(last) = self.text_engine.characters.last() {
            if self.text_engine.text.chars().last() == Some('\n') {
                (0.0, self.text_engine.lines.last().map_or(self.text_engine.ascent, |line| line.baseline_y))
            } else {
                (last.x + last.metrics.advance_width, self.text_engine.lines.last().map_or(self.text_engine.ascent, |line| line.baseline_y))
            }
        } else {
            (0.0, self.text_engine.ascent)
        };
        
        // Smooth the x-position (linear interpolation)
        self.smooth_cursor_x += (target_x - self.smooth_cursor_x) * 0.2;
        
        let cursor_top = (baseline_y - self.text_engine.ascent - self.scroll_y).round() as i32;
        let cursor_bottom = (baseline_y + self.text_engine.descent - self.scroll_y).round() as i32;
        let cx = self.smooth_cursor_x.round() as i32;
        
        for y in cursor_top..cursor_bottom {
            if y >= 0 && y < state.frame.height as i32 && cx >= 0 && cx < state.frame.width as i32 {
                let idx = ((y as u32 * state.frame.width as u32 + cx as u32) * 4) as usize;
                state.frame.buffer[idx + 0] = CURSOR_COLOR.0;
                state.frame.buffer[idx + 1] = CURSOR_COLOR.1;
                state.frame.buffer[idx + 2] = CURSOR_COLOR.2;
                state.frame.buffer[idx + 3] = 0xff;
            }
        }
    }

    fn tick_keyboard(&mut self, state: &mut EngineState) {
        for key in state.keyboard.get_pressed_keys() {
            match key.char {
                '\u{8}' => {
                    self.text_engine.text.pop(); // backspace
                }
                '\t' => {
                    self.text_engine.text.push_str("    "); // tab → 4 spaces
                }
                '\n' => {
                    self.text_engine.text.push('\n'); // enter
                }
                ' ' => {
                    self.text_engine.text.push(' '); // space
                }
                c if !c.is_control() && c != '\0' => {
                    self.text_engine.text.push(c); // normal character
                }
                _ => {}
            }
        }
    }
    
}

impl Application for TextApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        self.tick_keyboard(state);

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

        // Draw baselines
        for line in &self.text_engine.lines {
            let y = (line.baseline_y - self.scroll_y) as i32;
            if y >= 0 && y < height as i32 {
                for x in 0..width as i32 {
                    let idx = ((y as u32 * width as u32 + x as u32) * 4) as usize;
                    buffer[idx + 0] = BASELINE_COLOR.0;
                    buffer[idx + 1] = BASELINE_COLOR.1;
                    buffer[idx + 2] = BASELINE_COLOR.2;
                    buffer[idx + 3] = 0xff;
                }
            }
        }

        // Draw characters
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

        self.tick_cursor(state);
    }

    fn on_scroll(&mut self, _state: &mut EngineState, _dx: f32, dy: f32) {
        self.scroll_y -= dy * 20.0;
        self.scroll_y = self.scroll_y.max(0.0);
    }
    
}
