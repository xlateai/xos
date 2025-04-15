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
    // Reverting to original approach but giving better names
    metrics_top: f32,    // Distance from baseline to top of line
    metrics_bottom: f32, // Distance from baseline to bottom of line
    debug_text: String,  // For showing debug info on screen
}

impl TextApp {
    pub fn new() -> Self {
        let font_bytes = include_bytes!("../../assets/JetBrainsMono-Regular.ttf") as &[u8];
        let font = Font::from_bytes(font_bytes, FontSettings::default()).expect("Failed to load font");
        
        // Simply use fixed values that are reasonable for most fonts
        // These can be hand-tuned based on observation
        let metrics_top = FONT_SIZE * 0.7;    // How far chars extend above baseline
        let metrics_bottom = FONT_SIZE * 0.3; // How far chars extend below baseline
        let line_height = metrics_top + metrics_bottom + (FONT_SIZE * 0.2); // Add a bit of spacing
        
        // Create debug string with test characters
        let debug_text = "abcdefghijklmnopqrstuvwxyz ABCDEFGHIJKLMNOPQRSTUVWXYZ 0123456789 .,;:!?()[]{}'\"-_".to_string();

        Self {
            scroll_y: 0.0,
            text: VecDeque::from([debug_text.clone()]),
            cursor_x: 0,
            cursor_y: 0,
            font,
            line_height,
            metrics_top,
            metrics_bottom,
            debug_text,
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

    fn draw_rect(buffer: &mut [u8], width: u32, height: u32, x: i32, y: i32, w: u32, h: u32, color: (u8, u8, u8)) {
        if x < 0 || y < 0 {
            return; // Skip offscreen rectangles
        }
        
        let x = x as u32;
        let y = y as u32;
        
        let mut draw_pixel = |x, y| {
            if x < width && y < height {
                let idx = ((y * width + x) * 4) as usize;
                buffer[idx + 0] = color.0;
                buffer[idx + 1] = color.1;
                buffer[idx + 2] = color.2;
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

        let visual_lines = self.wrap_lines(width);
        let mut cursor_drawn = false;

        for (i, (line, logical_y, char_offset)) in visual_lines.iter().enumerate() {
            // Baseline position for this line
            let baseline_y = (i as f32 * self.line_height) + self.metrics_top - self.scroll_y;
            
            // Skip if the line is completely outside the visible area
            if baseline_y + self.metrics_bottom < 0.0 || baseline_y - self.metrics_top > height as f32 {
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
                    buffer, width, height, 0, (baseline_y - self.metrics_top) as i32, 
                    width, ASCENT_COLOR
                );
                
                // Descent line (magenta) - bottom of line
                Self::draw_horizontal_line(
                    buffer, width, height, 0, (baseline_y + self.metrics_bottom) as i32, 
                    width, DESCENT_COLOR
                );
            }

            for (j, ch) in line.chars().enumerate() {
                let (metrics, bitmap) = self.font.rasterize(ch, FONT_SIZE);
                let x_pos = x_cursor as i32;
                
                // EXPERIMENTAL POSITIONING - this is the part we keep getting wrong
                // Instead of trying to interpret fontdue's coordinate system, let's place
                // characters at fixed positions relative to the baseline and observe
                
                // Position character so its baseline aligns with our baseline_y
                // This means the top of the character is at baseline_y - (character height)
                let y_pos = (baseline_y - metrics.height as f32) as i32;
                
                // Draw a rectangle around the area where we're going to place the character
                if SHOW_BOUNDING_RECTANGLES {
                    let w = metrics.width as u32;
                    let h = metrics.height as u32;
                    Self::draw_rect(buffer, width, height, x_pos, y_pos, w, h, BOUND_COLOR);
                    
                    // Additional debug: Show where fontdue thinks ymin and height are
                    // Draw a thinner rect around the area fontdue defines
                    let fontdue_y = (baseline_y + metrics.bounds.ymin as f32) as i32;
                    Self::draw_rect(
                        buffer, width, height, 
                        x_pos, fontdue_y, 
                        metrics.bounds.width as u32, metrics.bounds.height as u32,
                        (127, 127, 255) // Light blue
                    );
                }

                // Draw the glyph bitmap
                for y in 0..metrics.height {
                    for x in 0..metrics.width {
                        let val = bitmap[y * metrics.width + x];
                        let px = x_pos + x as i32;
                        let py = y_pos + y as i32;
                        
                        if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                            let idx = ((py as u32 * width + px as u32) * 4) as usize;
                            // Use white text with alpha from the bitmap
                            buffer[idx + 0] = ((TEXT_COLOR.0 as u16 * val as u16) / 255) as u8;
                            buffer[idx + 1] = ((TEXT_COLOR.1 as u16 * val as u16) / 255) as u8;
                            buffer[idx + 2] = ((TEXT_COLOR.2 as u16 * val as u16) / 255) as u8;
                            buffer[idx + 3] = 0xff;
                        }
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
                let y0 = (baseline_y - self.metrics_top).max(0.0) as u32;
                let h = (self.metrics_top + self.metrics_bottom) as u32;
                
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