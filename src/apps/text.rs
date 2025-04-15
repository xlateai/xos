use crate::engine::{Application, EngineState};
use std::collections::VecDeque;

const BACKGROUND_COLOR: (u8, u8, u8) = (0, 0, 0);
const TEXT_COLOR: (u8, u8, u8) = (255, 255, 255);
const FONT_WIDTH: u32 = 8;
const FONT_HEIGHT: u32 = 16;

pub struct TextApp {
    scroll_y: f32,
    text: VecDeque<String>,
    cursor_x: usize,
    cursor_y: usize,
}

impl TextApp {
    pub fn new() -> Self {
        Self {
            scroll_y: 0.0,
            text: VecDeque::from([String::new()]),
            cursor_x: 0,
            cursor_y: 0,
        }
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

        // Clear background
        for i in (0..buffer.len()).step_by(4) {
            buffer[i + 0] = BACKGROUND_COLOR.0;
            buffer[i + 1] = BACKGROUND_COLOR.1;
            buffer[i + 2] = BACKGROUND_COLOR.2;
            buffer[i + 3] = 0xff;
        }

        // Draw text (placeholder block rectangles)
        let chars_per_line = (width / FONT_WIDTH) as usize;
        let lines_visible = (height / FONT_HEIGHT) as usize;
        let y_offset = (self.scroll_y / FONT_HEIGHT as f32) as usize;

        for (i, line) in self.text.iter().skip(y_offset).take(lines_visible).enumerate() {
            for (j, _ch) in line.chars().enumerate().take(chars_per_line) {
                let x0 = (j as u32) * FONT_WIDTH;
                let y0 = (i as u32) * FONT_HEIGHT;
                for dy in 0..FONT_HEIGHT {
                    for dx in 0..FONT_WIDTH {
                        let px = x0 + dx;
                        let py = y0 + dy;
                        if px < width && py < height {
                            let idx = ((py * width + px) * 4) as usize;
                            buffer[idx + 0] = TEXT_COLOR.0;
                            buffer[idx + 1] = TEXT_COLOR.1;
                            buffer[idx + 2] = TEXT_COLOR.2;
                            buffer[idx + 3] = 0xff;
                        }
                    }
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
            '\u{8}' => { // Backspace
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
                if ch.is_control() { return; }
                self.text[self.cursor_y].insert(self.cursor_x, ch);
                self.cursor_x += 1;
            }
        }
    }
} 