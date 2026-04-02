use crate::engine::{Application, EngineState};
use crate::rasterizer::text::text_rasterization::TextRasterizer;
use fontdue::{Font, FontSettings};
use std::time::{Instant, Duration};

const BACKGROUND_COLOR: (u8, u8, u8) = (0, 0, 0);
const TEXT_COLOR: (u8, u8, u8) = (255, 255, 255);
const INPUT_BG: (u8, u8, u8) = (24, 24, 24);
const INPUT_TEXT_COLOR: (u8, u8, u8) = (230, 230, 230);

pub struct ChatApp {
    messages_rasterizer: TextRasterizer,
    input_rasterizer: TextRasterizer,
    messages: Vec<String>, // newest first
    input: String,
    last_blink: Instant,
    show_cursor: bool,
}

impl ChatApp {
    pub fn new() -> Self {
        let font_bytes = include_bytes!("../assets/JetBrainsMono-Regular.ttf") as &[u8];
        // create two Font instances since Font doesn't implement Clone
        let messages_font = Font::from_bytes(font_bytes, FontSettings::default()).expect("Failed to load font");
        let input_font = Font::from_bytes(font_bytes, FontSettings::default()).expect("Failed to load font");

        // separate rasterizers for messages and input (different sizes)
        let mut messages_rasterizer = TextRasterizer::new(messages_font, 20.0);
        let mut input_rasterizer = TextRasterizer::new(input_font, 22.0);

        Self {
            messages_rasterizer,
            input_rasterizer,
            messages: Vec::new(),
            input: String::new(),
            last_blink: Instant::now(),
            show_cursor: true,
        }
    }

    fn draw_text_to_buffer(&self, buffer: &mut [u8], width: u32, height: u32, rasterizer: &TextRasterizer, offset_x: f32, offset_y: f32, color: (u8,u8,u8)) {
        for character in &rasterizer.characters {
            let px = (character.x + offset_x) as i32;
            let py = (character.y + offset_y) as i32;

            for y in 0..character.metrics.height {
                for x in 0..character.metrics.width {
                    let val = character.bitmap[y * character.metrics.width + x];
                    if val == 0 { continue; }

                    let sx = px + x as i32;
                    let sy = py + y as i32;

                    if sx >= 0 && sx < width as i32 && sy >= 0 && sy < height as i32 {
                        let idx = ((sy as u32 * width + sx as u32) * 4) as usize;
                        if idx + 3 < buffer.len() {
                            buffer[idx + 0] = color.0;
                            buffer[idx + 1] = color.1;
                            buffer[idx + 2] = color.2;
                            buffer[idx + 3] = val;
                        }
                    }
                }
            }
        }
    }
}

impl Application for ChatApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        // Handle pending on-screen keyboard characters
        while let Some(ch) = state.keyboard.onscreen.pop_pending_char() {
            match ch {
                '\u{8}' => { self.input.pop(); },
                '\n' => {
                    if !self.input.is_empty() {
                        // add to top
                        self.messages.insert(0, self.input.clone());
                        self.input.clear();
                    }
                }
                _ => { self.input.push(ch); }
            }
        }

        // Blink cursor
        let now = Instant::now();
        if now.duration_since(self.last_blink) > Duration::from_millis(500) {
            self.last_blink = now;
            self.show_cursor = !self.show_cursor;
        }

        let shape = state.frame.tensor.shape();
        let width = shape[1] as u32;
        let height = shape[0] as u32;
        let buffer = state.frame_buffer_mut();

        // Clear background
        for pixel in buffer.chunks_exact_mut(4) {
            pixel[0] = BACKGROUND_COLOR.0;
            pixel[1] = BACKGROUND_COLOR.1;
            pixel[2] = BACKGROUND_COLOR.2;
            pixel[3] = 255;
        }

        // Input area at bottom
        let input_height = (height as f32 * 0.12).max(48.0) as u32; // 12% of screen or min 48px
        let messages_height = height.saturating_sub(input_height);

        // Prepare messages text: join messages with blank line between
        let display_messages = if self.messages.is_empty() {
            "".to_string()
        } else {
            self.messages.join("\n\n")
        };

        self.messages_rasterizer.set_text(display_messages);
        self.messages_rasterizer.tick(width as f32, messages_height as f32);

        // Draw messages at top with small padding
        let padding = 8.0;
        self.draw_text_to_buffer(buffer, width, height, &self.messages_rasterizer, padding, padding, TEXT_COLOR);

        // Draw input background rectangle
        let input_top = messages_height as i32;
        for y in input_top..(height as i32) {
            for x in 0..(width as i32) {
                let idx = ((y as u32 * width + x as u32) * 4) as usize;
                if idx + 3 < buffer.len() {
                    buffer[idx + 0] = INPUT_BG.0;
                    buffer[idx + 1] = INPUT_BG.1;
                    buffer[idx + 2] = INPUT_BG.2;
                    buffer[idx + 3] = 255;
                }
            }
        }

        // Prepare input text (show cursor if blinking)
        let mut input_display = self.input.clone();
        if self.show_cursor {
            input_display.push('|');
        }

        self.input_rasterizer.set_text(input_display);
        self.input_rasterizer.tick(width as f32, input_height as f32);

        // draw input text with some padding inside input area
        let input_padding = 10.0;
        let input_offset_y = messages_height as f32 + input_padding;
        self.draw_text_to_buffer(buffer, width, height, &self.input_rasterizer, input_padding, input_offset_y, INPUT_TEXT_COLOR);
    }

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        // Show on-screen keyboard when user clicks/taps in the input area
        let shape = state.frame.tensor.shape();
        let width = shape[1] as u32;
        let height = shape[0] as u32;
        let input_height = (height as f32 * 0.12).max(48.0) as u32;
        let messages_height = height.saturating_sub(input_height);

        let mx = state.mouse.x as f32;
        let my = state.mouse.y as f32;

        // Coordinates are in pixels; input area starts at messages_height
        if my >= messages_height as f32 {
            state.keyboard.onscreen.show();
        }
    }

    fn on_mouse_up(&mut self, _state: &mut EngineState) {}
    fn on_mouse_move(&mut self, _state: &mut EngineState) {}

    fn on_key_char(&mut self, _state: &mut EngineState, ch: char) {
        match ch {
            '\u{8}' => { self.input.pop(); },
            '\n' => {
                if !self.input.is_empty() {
                    self.messages.insert(0, self.input.clone());
                    self.input.clear();
                }
            }
            _ => { self.input.push(ch); }
        }
    }
}
