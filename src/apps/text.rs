use crate::engine::{Application, EngineState};
use fontdue::{Font, FontSettings};
use std::collections::VecDeque;

const BACKGROUND_COLOR: (u8, u8, u8) = (0, 0, 0);
const TEXT_COLOR: (u8, u8, u8) = (255, 255, 255);
const CURSOR_COLOR: (u8, u8, u8) = (0, 255, 0);
const BOUND_COLOR: (u8, u8, u8) = (255, 0, 0);
const BASELINE_COLOR: (u8, u8, u8) = (0, 0, 255);
const ASCENT_COLOR: (u8, u8, u8) = (255, 255, 0);
const DESCENT_COLOR: (u8, u8, u8) = (255, 0, 255);
const FONT_SIZE: f32 = 48.0;

const SHOW_BOUNDING_RECTANGLES: bool = true;
const SHOW_BASELINE: bool = true;
const SHOW_ASCENT_DESCENT: bool = true;

pub struct TextApp {
    scroll_y: f32,
    text: VecDeque<String>,
    cursor_x: usize,
    cursor_y: usize,
    font: Font,
    line_height: f32,
    // We'll compute these directly from the font
    max_ascent: f32,
    max_descent: f32,
}

impl TextApp {
    pub fn new() -> Self {
        let font_bytes = include_bytes!("../../assets/JetBrainsMono-Regular.ttf") as &[u8];
        let font = Font::from_bytes(font_bytes, FontSettings::default()).expect("Failed to load font");
        
        // Get font metrics for common ASCII range to determine max ascent and descent
        let mut max_ascent: f32 = 0.0;
        let mut max_descent: f32 = 0.0;
        
        // Sample a range of characters to determine proper metrics
        for c in " abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789.,;:!?()[]{}'\"-_".chars() {
            let metrics = font.metrics(c, FONT_SIZE);
            // ymin is negative for characters extending above the baseline
            if -metrics.bounds.ymin as f32 > max_ascent {
                max_ascent = -metrics.bounds.ymin as f32;
            }
            
            // For descent, we need to calculate how much the character extends below the baseline
            // This is (ymin + height) if positive
            let descent = metrics.bounds.ymin as f32 + metrics.bounds.height as f32;
            if descent > 0.0 && descent > max_descent {
                max_descent = descent;
            }
        }
        
        // Add a bit of padding for line height
        let line_height = max_ascent + max_descent + (FONT_SIZE * 0.2);

        Self {
            scroll_y: 0.0,
            text: VecDeque::from([String::new()]),
            cursor_x: 0,
            cursor_y: 0,
            font,
            line_height,
            max_ascent,
            max_descent,
        }
    }

    fn wrap_lines(&self, max_width: u32) -> Vec<(String, usize, usize)> {
        let mut visual_lines = Vec::new();

        for (line_idx, line) in self.text.iter().enumerate() {
            let mut current_line = String::new();
            let mut current_width = 0.0;
            let mut char_start = 0;

            for (i, ch) in line.chars().enumerate() {
                let metrics = self.font.metrics(ch, FONT_SIZE);
                if current_width + metrics.advance_width > max_width as f32 {
                    visual_lines.push((current_line.clone(), line_idx, char_start));
                    current_line.clear();
                    current_width = 0.0;
                    char_start = i;
                }
                current_line.push(ch);
                current_width += metrics.advance_width;
            }

            visual_lines.push((current_line, line_idx, char_start));
        }

        visual_lines
    }

    fn draw_rect(buffer: &mut [u8], width: u32, height: u32, x: u32, y: u32, w: u32, h: u32) {
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

    fn draw_horizontal_line(buffer: &mut [u8], width: u32, height: u32, x: u32, y: i32, w: u32, color: (u8, u8, u8)) {
        if y < 0 || y >= height as i32 {
            return;
        }
        
        for dx in 0..w {
            let px = x + dx;
            if px < width {
                let idx = ((y as u32 * width + px) * 4) as usize;
                buffer[idx + 0] = color.0;
                buffer[idx + 1] = color.1;
                buffer[idx + 2] = color.2;
                buffer[idx + 3] = 0xff;
            }
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

        // Clear the screen
        for i in (0..buffer.len()).step_by(4) {
            buffer[i + 0] = BACKGROUND_COLOR.0;
            buffer[i + 1] = BACKGROUND_COLOR.1;
            buffer[i + 2] = BACKGROUND_COLOR.2;
            buffer[i + 3] = 0xff;
        }

        // Always draw baseline, ascent, and descent lines for the first line even if empty
        if self.text.is_empty() || (self.text.len() == 1 && self.text[0].is_empty()) {
            let baseline_y = self.max_ascent - self.scroll_y;
            
            if SHOW_BASELINE {
                Self::draw_horizontal_line(
                    buffer, width, height, 0, baseline_y as i32, width, BASELINE_COLOR
                );
            }
            
            if SHOW_ASCENT_DESCENT {
                Self::draw_horizontal_line(
                    buffer, width, height, 0, (baseline_y - self.max_ascent) as i32, 
                    width, ASCENT_COLOR
                );
                
                Self::draw_horizontal_line(
                    buffer, width, height, 0, (baseline_y + self.max_descent) as i32, 
                    width, DESCENT_COLOR
                );
            }
            
            // Draw cursor at start position if we're at the beginning
            if self.cursor_x == 0 && self.cursor_y == 0 {
                let x = 0;
                let y0 = (baseline_y - self.max_ascent).max(0.0) as u32;
                let h = (self.max_ascent + self.max_descent) as u32;
                
                for y in 0..h {
                    let py = y0 + y;
                    if py < height {
                        let idx = ((py * width + x) * 4) as usize;
                        buffer[idx + 0] = CURSOR_COLOR.0;
                        buffer[idx + 1] = CURSOR_COLOR.1;
                        buffer[idx + 2] = CURSOR_COLOR.2;
                        buffer[idx + 3] = 0xff;
                    }
                }
            }
        }

        let visual_lines = self.wrap_lines(width);
        let mut cursor_drawn = false;

        for (i, (line, logical_y, char_offset)) in visual_lines.iter().enumerate() {
            // Position based on line height
            let baseline_y = (i as f32 * self.line_height) + self.max_ascent - self.scroll_y;
            
            if baseline_y + self.max_descent < 0.0 || baseline_y - self.max_ascent > height as f32 {
                continue; // Skip if line is not visible
            }

            let mut x_cursor = 0.0;
            let mut cursor_here = false;

            // Draw baseline for this line
            if SHOW_BASELINE {
                Self::draw_horizontal_line(
                    buffer, width, height, 0, baseline_y as i32, width, BASELINE_COLOR
                );
            }

            // Draw ascent and descent lines for this line
            if SHOW_ASCENT_DESCENT {
                Self::draw_horizontal_line(
                    buffer, width, height, 0, (baseline_y - self.max_ascent) as i32, 
                    width, ASCENT_COLOR
                );
                
                Self::draw_horizontal_line(
                    buffer, width, height, 0, (baseline_y + self.max_descent) as i32, 
                    width, DESCENT_COLOR
                );
            }

            for (j, ch) in line.chars().enumerate() {
                let (metrics, bitmap) = self.font.rasterize(ch, FONT_SIZE);
                let x_pos = x_cursor as u32;
                
                // Calculate vertical position:
                // The baseline is at baseline_y, and we need to position the glyph
                // so that its baseline aligns with baseline_y.
                // The ymin value tells us how far above the baseline the glyph extends
                let y_pos = (baseline_y - metrics.bounds.ymin as f32) as i32;

                // Draw glyph
                for y in 0..metrics.height {
                    for x in 0..metrics.width {
                        let val = bitmap[y * metrics.width + x];
                        let px = x_pos + x as u32;
                        let py = y_pos + y as i32;
                        if px < width && py >= 0 && py < height as i32 {
                            let idx = ((py as u32 * width + px) * 4) as usize;
                            // Use proper text color and alpha blending
                            buffer[idx + 0] = ((TEXT_COLOR.0 as u16 * val as u16) / 255) as u8;
                            buffer[idx + 1] = ((TEXT_COLOR.1 as u16 * val as u16) / 255) as u8;
                            buffer[idx + 2] = ((TEXT_COLOR.2 as u16 * val as u16) / 255) as u8;
                            buffer[idx + 3] = 0xff;
                        }
                    }
                }

                if SHOW_BOUNDING_RECTANGLES {
                    let w = metrics.width as u32;
                    let h = metrics.height as u32;
                    if y_pos >= 0 {
                        Self::draw_rect(buffer, width, height, x_pos, y_pos as u32, w, h);
                    }
                }

                // Check cursor position
                let is_cursor = *logical_y == self.cursor_y && self.cursor_x == char_offset + j;
                if is_cursor {
                    cursor_here = true;
                }

                x_cursor += metrics.advance_width;
            }

            if *logical_y == self.cursor_y && self.cursor_x == char_offset + line.len() {
                cursor_here = true;
            }

            if cursor_here && !cursor_drawn {
                let x = x_cursor as u32;
                let y0 = (baseline_y - self.max_ascent).max(0.0) as u32;
                let h = (self.max_ascent + self.max_descent) as u32;
                
                for y in 0..h {
                    let py = y0 + y;
                    if x < width && py < height {
                        let idx = ((py * width + x) * 4) as usize;
                        buffer[idx + 0] = CURSOR_COLOR.0;
                        buffer[idx + 1] = CURSOR_COLOR.1;
                        buffer[idx + 2] = CURSOR_COLOR.2;
                        buffer[idx + 3] = 0xff;
                    }
                }
                cursor_drawn = true;
            }
        }
    }

    fn on_scroll(&mut self, _state: &mut EngineState, _dx: f32, dy: f32) {
        self.scroll_y -= dy;
        self.scroll_y = self.scroll_y.max(0.0);
    }

    fn on_key_char(&mut self, _state: &mut EngineState, ch: char) {
        match ch {
            '\r' | '\n' => {
                let current_line = self.text[self.cursor_y].split_off(self.cursor_x);
                self.text.insert(self.cursor_y + 1, current_line);
                self.cursor_y += 1;
                self.cursor_x = 0;
            }
            '\u{8}' => {
                if self.cursor_x > 0 {
                    self.cursor_x -= 1;
                    self.text[self.cursor_y].remove(self.cursor_x);
                } else if self.cursor_y > 0 {
                    let current = self.text.remove(self.cursor_y).unwrap();
                    self.cursor_y -= 1;
                    self.cursor_x = self.text[self.cursor_y].len();
                    self.text[self.cursor_y].push_str(&current);
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