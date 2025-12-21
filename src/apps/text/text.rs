use crate::engine::{Application, EngineState};
use crate::apps::text::geometric::GeometricText;
use crate::apps::text::onscreen_keyboard::OnScreenKeyboard;
use crate::apps::partitions::partition::Partition;
use fontdue::{Font, FontSettings};
use std::time::{Instant, Duration};

const BACKGROUND_COLOR: (u8, u8, u8) = (0, 0, 0);
const TEXT_COLOR: (u8, u8, u8) = (255, 255, 255);
const CURSOR_COLOR: (u8, u8, u8) = (0, 255, 0);
const BOUND_COLOR: (u8, u8, u8) = (255, 0, 0);
const BASELINE_COLOR: (u8, u8, u8) = (100, 100, 100);

const SHOW_BOUNDING_RECTANGLES: bool = true;
const DRAW_BASELINES: bool = true;
const DOUBLE_TAP_TIME_MS: u64 = 300; // 300ms window for double tap
const DOUBLE_TAP_DISTANCE: f32 = 50.0; // Maximum distance between taps in pixels

use std::collections::HashMap;

pub struct TextApp {
    pub text_engine: GeometricText,
    pub scroll_y: f32,
    pub smooth_cursor_x: f32,
    pub fade_map: HashMap<(char, u32, u32), f32>,
    keyboard: OnScreenKeyboard,
    last_tap_time: Option<Instant>,
    last_tap_x: f32,
    last_tap_y: f32,
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
            fade_map: HashMap::new(),
            keyboard: OnScreenKeyboard::new(),
            last_tap_time: None,
            last_tap_x: 0.0,
            last_tap_y: 0.0,
        }
    }

    fn draw_rect(
        buffer: &mut [u8],
        width: u32,
        height: u32,
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        alpha: u8,
    ) {
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
                buffer[idx + 3] = alpha;
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
        // Extract all needed values in a block to release borrows
        let (width, height, content_top, keyboard_top, keyboard_bottom_safe, content_bottom) = {
            let shape = state.frame.array.shape();
            let width = shape[1] as f32;
            let height = shape[0] as f32;
            
            let safe_region = &state.frame.safe_region_boundaries;
            
            // Content area uses the safe region bounds
            // Keyboard sits at the bottom of the safe region
            let keyboard_height = 0.30; // 30% of screen height
            let keyboard_bottom_safe = safe_region.y2; // Bottom of safe region
            let keyboard_top = (keyboard_bottom_safe - keyboard_height).max(safe_region.y1);
            let content_top = safe_region.y1 * height; // Top of safe region
            let content_bottom = keyboard_top * height; // Top of keyboard area
            
            (width, height, content_top, keyboard_top, keyboard_bottom_safe, content_bottom)
        };
        
        // Now get mutable buffer (after all immutable borrows are released)
        let buffer = state.frame_buffer_mut();
        
        // Position keyboard partition just above bottom safe region
        // The keyboard's internal coordinates (0-1) work within this partition automatically
        self.keyboard.data_mut().top = keyboard_top;
        self.keyboard.data_mut().bottom = keyboard_bottom_safe;
        self.keyboard.data_mut().left = 0.0;
        self.keyboard.data_mut().right = 1.0;
    
        self.text_engine.tick(width, content_bottom - content_top);
    
        // Clear screen
        for i in (0..buffer.len()).step_by(4) {
            buffer[i + 0] = BACKGROUND_COLOR.0;
            buffer[i + 1] = BACKGROUND_COLOR.1;
            buffer[i + 2] = BACKGROUND_COLOR.2;
            buffer[i + 3] = 0xff;
        }
    
        // Draw baselines (offset by content_top)
        if DRAW_BASELINES {
            for line in &self.text_engine.lines {
                let y = ((line.baseline_y - self.scroll_y) + content_top) as i32;
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
        }
    
        // Draw characters with fade and slide-in (offset by content_top)
        for character in &self.text_engine.characters {
            let fade_key = (character.ch, character.x.to_bits(), character.y.to_bits());
            let fade = self.fade_map.entry(fade_key).or_insert(0.0);
            *fade = (*fade + 0.16).min(1.0); // Fast fade

            let alpha = (*fade * 255.0).round() as u8;

            // Slide in from the right using bitmap width as base
            let slide_offset = (character.width as f32 * 1.0 * (1.0 - *fade)) as i32;
            let px = (character.x as i32) + slide_offset;
            let py = ((character.y - self.scroll_y) + content_top) as i32;
            let pw = character.width as u32;
            let ph = character.height as u32;
    
            for y in 0..character.metrics.height {
                for x in 0..character.metrics.width {
                    let val = character.bitmap[y * character.metrics.width + x];
                    let faded_val = ((val as u16 * alpha as u16) / 255) as u8;
    
                    let sx = px + x as i32;
                    let sy = py + y as i32;
    
                    if sx >= 0 && sx < width as i32 && sy >= 0 && sy < height as i32 {
                        let idx = ((sy as u32 * width as u32 + sx as u32) * 4) as usize;
                        buffer[idx + 0] = ((TEXT_COLOR.0 as u16 * faded_val as u16) / 255) as u8;
                        buffer[idx + 1] = ((TEXT_COLOR.1 as u16 * faded_val as u16) / 255) as u8;
                        buffer[idx + 2] = ((TEXT_COLOR.2 as u16 * faded_val as u16) / 255) as u8;
                        buffer[idx + 3] = faded_val;
                    }
                }
            }
    
            if SHOW_BOUNDING_RECTANGLES {
                Self::draw_rect(buffer, width as u32, height as u32, px, py, pw, ph, alpha);
            }
        }
    
        // Draw cursor (offset by content_top)
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
        
        let cursor_top = ((baseline_y - self.text_engine.ascent - self.scroll_y) + content_top).round() as i32;
        let cursor_bottom = ((baseline_y + self.text_engine.descent - self.scroll_y) + content_top).round() as i32;
        let cx = self.smooth_cursor_x.round() as i32;
        
        for y in cursor_top..cursor_bottom {
            if y >= 0 && y < height as i32 && cx >= 0 && cx < width as i32 {
                let idx = ((y as u32 * width as u32 + cx as u32) * 4) as usize;
                buffer[idx + 0] = CURSOR_COLOR.0;
                buffer[idx + 1] = CURSOR_COLOR.1;
                buffer[idx + 2] = CURSOR_COLOR.2;
                buffer[idx + 3] = 0xff;
            }
        }
    
        // Draw keyboard
        let width_u32 = width as u32;
        let height_u32 = height as u32;
        self.keyboard.draw(buffer, width_u32, height_u32);
    }
    

    fn on_scroll(&mut self, _state: &mut EngineState, _dx: f32, dy: f32) {
        self.scroll_y -= dy * 20.0;
        self.scroll_y = self.scroll_y.max(0.0);
    }

    fn on_key_char(&mut self, _state: &mut EngineState, ch: char) {
        match ch {
            '\t' => {
                self.text_engine.text.push_str("    ");
            }
            '\r' | '\n' => {
                self.text_engine.text.push('\n');
            }
            '\u{8}' => {
                // Backspace
                self.text_engine.text.pop();
            }
            _ => {
                if !ch.is_control() {
                    self.text_engine.text.push(ch);
                }
            }
        }
    }

    fn on_mouse_move(&mut self, state: &mut EngineState) {
        let shape = state.frame.array.shape();
        let width = shape[1] as f32;
        let height = shape[0] as f32;
        
        // Update keyboard hover state
        self.keyboard.update_hover(state.mouse.x, state.mouse.y, width, height);
    }

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        let shape = state.frame.array.shape();
        let width = shape[1] as f32;
        let height = shape[0] as f32;
        
        // Check if click is in keyboard area first
        let keyboard_data = self.keyboard.data();
        let keyboard_left = keyboard_data.left * width;
        let keyboard_right = keyboard_data.right * width;
        let keyboard_top = keyboard_data.top * height;
        let keyboard_bottom = keyboard_data.bottom * height;
        
        let is_in_keyboard_area = state.mouse.x >= keyboard_left 
            && state.mouse.x <= keyboard_right 
            && state.mouse.y >= keyboard_top 
            && state.mouse.y <= keyboard_bottom;
        
        if is_in_keyboard_area {
            // Handle keyboard key press (handles dismiss button internally)
            if let Some(ch) = self.keyboard.check_key_press(state.mouse.x, state.mouse.y, width, height) {
                // Route through on_key_char to ensure it works on all platforms
                self.on_key_char(state, ch);
            }
            return;
        }
        
        // Check for double tap in content area
        let now = Instant::now();
        let is_double_tap = if let Some(last_time) = self.last_tap_time {
            let time_since_last = now.duration_since(last_time);
            let distance = ((state.mouse.x - self.last_tap_x).powi(2) + (state.mouse.y - self.last_tap_y).powi(2)).sqrt();
            
            time_since_last < Duration::from_millis(DOUBLE_TAP_TIME_MS) && distance < DOUBLE_TAP_DISTANCE
        } else {
            false
        };
        
        if is_double_tap {
            // Toggle keyboard
            self.keyboard.toggle_minimize();
            // Reset tap tracking to prevent triple-tap from immediately closing
            self.last_tap_time = None;
        } else {
            // Update tap tracking
            self.last_tap_time = Some(now);
            self.last_tap_x = state.mouse.x;
            self.last_tap_y = state.mouse.y;
        }
    }

    fn on_mouse_up(&mut self, _state: &mut EngineState) {
        // Release all keys when mouse is released
        self.keyboard.release_keys();
    }
    
}
