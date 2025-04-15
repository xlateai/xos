use crate::engine::{Application, EngineState};
use fontdue::{Font, FontSettings};
use std::collections::VecDeque;

const BACKGROUND_COLOR: (u8, u8, u8) = (0, 0, 0);
const TEXT_COLOR: (u8, u8, u8) = (255, 255, 255);
const CURSOR_COLOR: (u8, u8, u8) = (0, 255, 0);
const FONT_SIZE: f32 = 48.0; // 3x larger

pub struct TextApp {
    scroll_y: f32,
    text: VecDeque<String>,
    cursor_x: usize,
    cursor_y: usize,
    font: Font,
}

impl TextApp {
    pub fn new() -> Self {
        let font_bytes = include_bytes!("../../assets/JetBrainsMono-Regular.ttf") as &[u8];
        let font = Font::from_bytes(font_bytes, FontSettings::default()).expect("Failed to load font");

        Self {
            scroll_y: 0.0,
            text: VecDeque::from([String::new()]),
            cursor_x: 0,
            cursor_y: 0,
            font,
        }
    }

    fn wrap_lines(&self, max_width: u32) -> Vec<(String, f32)> {
        let mut visual_lines = Vec::new();

        for line in &self.text {
            let mut current_line = String::new();
            let mut current_width = 0.0;

            for ch in line.chars() {
                let metrics = self.font.metrics(ch, FONT_SIZE);
                if current_width + metrics.advance_width > max_width as f32 {
                    visual_lines.push((current_line.clone(), current_width));
                    current_line.clear();
                    current_width = 0.0;
                }
                current_line.push(ch);
                current_width += metrics.advance_width;
            }

            visual_lines.push((current_line, current_width));
        }

        visual_lines
    }
}

impl Application for TextApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        let width = state.frame.width;
        let height = state.frame.height;
        let buffer = &mut state.frame.buffer;

        for i in (0..buffer.len()).step_by(4) {
            buffer[i + 0] = BACKGROUND_COLOR.0;
            buffer[i + 1] = BACKGROUND_COLOR.1;
            buffer[i + 2] = BACKGROUND_COLOR.2;
            buffer[i + 3] = 0xff;
        }

        let visual_lines = self.wrap_lines(width);
        let lines_visible = (height as f32 / FONT_SIZE) as usize;
        let y_offset = (self.scroll_y / FONT_SIZE) as usize;

        for (i, (line, line_width)) in visual_lines.iter().skip(y_offset).take(lines_visible).enumerate() {
            let mut cursor_x = 0;

            for ch in line.chars() {
                let (metrics, bitmap) = self.font.rasterize(ch, FONT_SIZE);
                let x0 = cursor_x as u32;
                let y0 = (i as f32 * FONT_SIZE) as u32;

                for y in 0..metrics.height {
                    for x in 0..metrics.width {
                        let val = bitmap[y * metrics.width + x];
                        let px = x0 + x as u32;
                        let py = y0 + y as u32;
                        if px < width && py < height {
                            let idx = ((py * width + px) * 4) as usize;
                            buffer[idx + 0] = val;
                            buffer[idx + 1] = val;
                            buffer[idx + 2] = val;
                            buffer[idx + 3] = 0xff;
                        }
                    }
                }

                cursor_x += metrics.advance_width as usize;
            }

            // Draw cursor bar at end of visual line
            let x_cursor = line_width.round() as u32;
            let y0 = (i as f32 * FONT_SIZE) as u32;
            for y in 0..(FONT_SIZE as u32) {
                let px = x_cursor.min(width - 1);
                let py = y0 + y;
                if px < width && py < height {
                    let idx = ((py * width + px) * 4) as usize;
                    buffer[idx + 0] = CURSOR_COLOR.0;
                    buffer[idx + 1] = CURSOR_COLOR.1;
                    buffer[idx + 2] = CURSOR_COLOR.2;
                    buffer[idx + 3] = 0xff;
                }
            }
        }
    }

    fn on_scroll(&mut self, _state: &mut EngineState, _dx: f32, dy: f32) {
        self.scroll_y += dy;
    }

    fn on_key_char(&mut self, _state: &mut EngineState, ch: char) {
        match ch {
            '\n' => {
                self.text.insert(self.cursor_y + 1, String::new());
                self.cursor_y += 1;
                self.cursor_x = 0;
            }
            '\u{8}' => {
                if self.cursor_x > 0 {
                    self.text[self.cursor_y].remove(self.cursor_x - 1);
                    self.cursor_x -= 1;
                } else if self.cursor_y > 0 {
                    let prev_line = self.text.remove(self.cursor_y).unwrap();
                    self.cursor_y -= 1;
                    self.cursor_x = self.text[self.cursor_y].len();
                    self.text[self.cursor_y].push_str(&prev_line);
                }
            }
            _ => {
                if ch.is_control() {
                    return;
                }
                self.text[self.cursor_y].insert(self.cursor_x, ch);
                self.cursor_x += 1;
            }
        }
    }
}