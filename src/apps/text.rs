use crate::engine::{Application, EngineState};
use fontdue::{Font, FontSettings};
use std::collections::VecDeque;

const BACKGROUND_COLOR: (u8, u8, u8) = (0, 0, 0);
const TEXT_COLOR: (u8, u8, u8) = (255, 255, 255);
const CURSOR_COLOR: (u8, u8, u8) = (0, 255, 0);
const BOUND_COLOR: (u8, u8, u8) = (255, 0, 0);
const BASELINE_COLOR: (u8, u8, u8) = (0, 0, 255);  // Blue for baseline
const ASCENT_COLOR: (u8, u8, u8) = (255, 255, 0);  // Yellow for ascent
const DESCENT_COLOR: (u8, u8, u8) = (255, 0, 255); // Magenta for descent
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
    font_metrics: FontMetrics,
}

// Create a struct to hold font metrics
struct FontMetrics {
    ascent: f32,  // How far characters extend above baseline
    descent: f32, // How far characters extend below baseline
    line_gap: f32, // Extra space between lines
}

impl TextApp {
    pub fn new() -> Self {
        let font_bytes = include_bytes!("../../assets/JetBrainsMono-Regular.ttf") as &[u8];
        let font = Font::from_bytes(font_bytes, FontSettings::default()).expect("Failed to load font");
        
        // First, get the font's inherent metrics for ascent and descent
        // For this, we need to look at the metrics of characters that reach the extremes
        
        // Calculate ascent using capital letters and tall lowercase letters
        let ascent_chars = ['A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 
                            'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z',
                            'b', 'd', 'f', 'h', 'k', 'l', 't'];
                            
        // Calculate descent using lowercase letters with descenders
        let descent_chars = ['g', 'j', 'p', 'q', 'y', ',', ';'];
        
        let mut max_ascent = 0.0;
        let mut max_descent = 0.0;
        
        // Measure ascent (characters extending above baseline)
        for &c in ascent_chars.iter() {
            let metrics = font.metrics(c, FONT_SIZE);
            let char_ascent = -metrics.bounds.ymin as f32;
            if char_ascent > max_ascent {
                max_ascent = char_ascent;
            }
        }
        
        // Measure descent (characters extending below baseline)
        for &c in descent_chars.iter() {
            let metrics = font.metrics(c, FONT_SIZE);
            let char_descent = metrics.bounds.ymin as f32 + metrics.bounds.height as f32;
            if char_descent > max_descent {
                max_descent = char_descent;
            }
        }
        
        // Add padding for line gap
        let line_gap = FONT_SIZE * 0.2;
        
        // The total line height includes ascent, descent and line gap
        let line_height = max_ascent + max_descent + line_gap;

        let font_metrics = FontMetrics {
            ascent: max_ascent,
            descent: max_descent,
            line_gap,
        };

        Self {
            scroll_y: 0.0,
            text: VecDeque::from([String::new()]),
            cursor_x: 0,
            cursor_y: 0,
            font,
            line_height,
            font_metrics,
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

    fn draw_rect(buffer: &mut [u8], width: u32, height: u32, x: i32, y: i32, w: u32, h: u32) {
        if x < 0 || y < 0 {
            return; // Skip offscreen rectangles
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
            let baseline_y = self.font_metrics.ascent - self.scroll_y;
            
            if SHOW_BASELINE {
                Self::draw_horizontal_line(
                    buffer, width, height, 0, baseline_y as i32, width, BASELINE_COLOR
                );
            }
            
            if SHOW_ASCENT_DESCENT {
                // Ascent line - at top of line, baseline_y - ascent
                Self::draw_horizontal_line(
                    buffer, width, height, 0, (baseline_y - self.font_metrics.ascent) as i32, 
                    width, ASCENT_COLOR
                );
                
                // Descent line - at bottom of line, baseline_y + descent
                Self::draw_horizontal_line(
                    buffer, width, height, 0, (baseline_y + self.font_metrics.descent) as i32, 
                    width, DESCENT_COLOR
                );
            }
            
            // Draw cursor at start position if we're at the beginning
            if self.cursor_x == 0 && self.cursor_y == 0 {
                let x = 0;
                let y0 = (baseline_y - self.font_metrics.ascent).max(0.0) as u32;
                let h = (self.font_metrics.ascent + self.font_metrics.descent) as u32;
                
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
            // Baseline position for this line
            let baseline_y = (i as f32 * self.line_height) + self.font_metrics.ascent - self.scroll_y;
            
            // Skip if the line is completely outside the visible area
            if baseline_y + self.font_metrics.descent < 0.0 || baseline_y - self.font_metrics.ascent > height as f32 {
                continue;
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
                // Ascent line (yellow) - top of line
                Self::draw_horizontal_line(
                    buffer, width, height, 0, (baseline_y - self.font_metrics.ascent) as i32, 
                    width, ASCENT_COLOR
                );
                
                // Descent line (magenta) - bottom of line
                Self::draw_horizontal_line(
                    buffer, width, height, 0, (baseline_y + self.font_metrics.descent) as i32, 
                    width, DESCENT_COLOR
                );
            }

            for (j, ch) in line.chars().enumerate() {
                let (metrics, bitmap) = self.font.rasterize(ch, FONT_SIZE);
                let x_pos = x_cursor as i32;
                
                // The crucial fix: proper vertical positioning
                // In fontdue, ymin is negative for characters above baseline, positive for below
                let y_pos = (baseline_y + metrics.bounds.ymin as f32) as i32;
                
                // Draw character bitmap using alpha blending
                for y in 0..metrics.height {
                    for x in 0..metrics.width {
                        let val = bitmap[y * metrics.width + x];
                        let px = x_pos + x as i32;
                        let py = y_pos + y as i32;
                        
                        if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                            let idx = ((py as u32 * width + px as u32) * 4) as usize;
                            // Alpha blend with background
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
                    Self::draw_rect(buffer, width, height, x_pos, y_pos, w, h);
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
                let y0 = (baseline_y - self.font_metrics.ascent).max(0.0) as u32;
                let h = (self.font_metrics.ascent + self.font_metrics.descent) as u32;
                
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