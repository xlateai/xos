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

// Arrow key characters (using Unicode arrow symbols)
const ARROW_LEFT: char = '\u{2190}';  // ←
const ARROW_RIGHT: char = '\u{2192}'; // →
const ARROW_UP: char = '\u{2191}';    // ↑
const ARROW_DOWN: char = '\u{2193}';  // ↓

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
    cursor_position: usize, // Character index where cursor should be
    dragging: bool,
    last_mouse_y: f32,
    // Keyboard repeat tracking
    held_key: Option<char>,
    held_key_start_time: Option<Instant>,
    last_repeat_time: Option<Instant>,
    touch_started_on_keyboard: bool,
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
            cursor_position: 0,
            dragging: false,
            last_mouse_y: 0.0,
            held_key: None,
            held_key_start_time: None,
            last_repeat_time: None,
            touch_started_on_keyboard: false,
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
    
        // Handle keyboard repeat for physical keyboard (2x faster than normal)
        let now = Instant::now();
        let mut keys_to_process: Vec<char> = Vec::new();
        
        if let Some(held_ch) = self.held_key {
            if let Some(start_time) = self.held_key_start_time {
                // Normal repeat delay is typically ~500ms, so 2x faster = 250ms
                let repeat_delay = Duration::from_millis(250);
                let repeat_interval = Duration::from_millis(50); // Fast repeat interval
                
                if now.duration_since(start_time) >= repeat_delay {
                    if let Some(last_repeat) = self.last_repeat_time {
                        if now.duration_since(last_repeat) >= repeat_interval {
                            keys_to_process.push(held_ch);
                            self.last_repeat_time = Some(now);
                        }
                    } else {
                        keys_to_process.push(held_ch);
                        self.last_repeat_time = Some(now);
                    }
                }
            }
        }
        
        // Handle on-screen keyboard repeat
        if let Some(ch) = self.keyboard.check_key_hold_repeat(now) {
            keys_to_process.push(ch);
        }
        
        // Store keys to process after we're done with buffer
        let keys_to_process_after = keys_to_process;
    
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
        // Find cursor position based on cursor_position index
        // First, find which line the cursor is on
        let line_info_with_idx = self.text_engine.lines.iter()
            .enumerate()
            .find(|(_, line)| {
                line.start_index <= self.cursor_position && self.cursor_position <= line.end_index
            });
        
        let (target_x, baseline_y) = if let Some((line_idx, line)) = line_info_with_idx {
            // Found the line - check if there are characters in this line
            let chars_in_line: Vec<_> = self.text_engine.characters.iter()
                .filter(|c| c.line_index == line_idx)
                .collect();
            
            if chars_in_line.is_empty() {
                // Empty line - cursor at start
                (0.0, line.baseline_y)
            } else {
                // Line has characters - find the appropriate x position
                // Check if cursor is at the start of the line
                if self.cursor_position == line.start_index {
                    (0.0, line.baseline_y)
                } else {
                    // Find character at or before cursor position
                    let mut found_char = None;
                    let mut char_after = None;
                    
                    for character in self.text_engine.characters.iter() {
                        if character.char_index == self.cursor_position {
                            found_char = Some(character);
                            break;
                        } else if character.char_index > self.cursor_position && character.line_index == line_idx {
                            char_after = Some(character);
                            break;
                        }
                    }
                    
                    if let Some(char_at_cursor) = found_char {
                        // Cursor is before this character
                        (char_at_cursor.x, line.baseline_y)
                    } else if let Some(char_after_cursor) = char_after {
                        // Cursor is before this character (on same line)
                        (char_after_cursor.x, line.baseline_y)
                    } else {
                        // Cursor is at end of line - find last character's end position
                        if let Some(last_in_line) = chars_in_line.last() {
                            (last_in_line.x + last_in_line.metrics.advance_width, line.baseline_y)
                        } else {
                            (0.0, line.baseline_y)
                        }
                    }
                }
            }
        } else if self.cursor_position == 0 {
            // Cursor at very start (before any lines)
            if let Some(first_line) = self.text_engine.lines.first() {
                (0.0, first_line.baseline_y)
            } else {
                (0.0, self.text_engine.ascent)
            }
        } else if self.cursor_position >= self.text_engine.text.chars().count() {
            // Cursor at end of text
            if let Some(last_line) = self.text_engine.lines.last() {
                // Find the line index
                let last_line_idx = self.text_engine.lines.len() - 1;
                // Check if last line has characters
                let chars_in_last_line: Vec<_> = self.text_engine.characters.iter()
                    .filter(|c| c.line_index == last_line_idx)
                    .collect();
                
                if chars_in_last_line.is_empty() {
                    (0.0, last_line.baseline_y)
                } else if let Some(last_char) = chars_in_last_line.last() {
                    (last_char.x + last_char.metrics.advance_width, last_line.baseline_y)
                } else {
                    (0.0, last_line.baseline_y)
                }
            } else if let Some(last) = self.text_engine.characters.last() {
                (last.x + last.metrics.advance_width, self.text_engine.lines.last().map_or(self.text_engine.ascent, |line| line.baseline_y))
            } else {
                (0.0, self.text_engine.ascent)
            }
        } else {
            // Fallback: cursor position is out of bounds somehow
            // Try to find nearest line or character
            if let Some(last) = self.text_engine.characters.last() {
                (last.x + last.metrics.advance_width, self.text_engine.lines.last().map_or(self.text_engine.ascent, |line| line.baseline_y))
            } else {
                (0.0, self.text_engine.ascent)
            }
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
        
        // Draw black area with green border below keyboard (in unsafe region)
        let keyboard_bottom_px = keyboard_bottom_safe * height;
        let screen_bottom = height;
        
        if keyboard_bottom_px < screen_bottom {
            let border_y = keyboard_bottom_px.round() as i32;
            let fill_start_y = (border_y + 1).max(0);
            let fill_end_y = screen_bottom as i32;
            
            // Draw green border line
            if border_y >= 0 && border_y < height as i32 {
                for x in 0..width as i32 {
                    let idx = ((border_y as u32 * width as u32 + x as u32) * 4) as usize;
                    if idx + 3 < buffer.len() {
                        buffer[idx + 0] = 0;   // R
                        buffer[idx + 1] = 255; // G
                        buffer[idx + 2] = 0;   // B
                        buffer[idx + 3] = 0xff; // A
                    }
                }
            }
            
            // Fill black pixels below keyboard
            for y in fill_start_y..fill_end_y {
                if y >= 0 && y < height as i32 {
                    for x in 0..width as i32 {
                        let idx = ((y as u32 * width as u32 + x as u32) * 4) as usize;
                        if idx + 3 < buffer.len() {
                            buffer[idx + 0] = 0;   // R
                            buffer[idx + 1] = 0;   // G
                            buffer[idx + 2] = 0;   // B
                            buffer[idx + 3] = 0xff; // A
                        }
                    }
                }
            }
        }
        
        // Process repeated keys now that we're done with buffer
        // Buffer borrow ends here, so we can borrow state again
        for ch in keys_to_process_after {
            self.on_key_char(state, ch);
        }
    }
    

    fn on_scroll(&mut self, _state: &mut EngineState, _dx: f32, dy: f32) {
        // Use same scrolling approach as scroll.rs
        self.scroll_y += dy;
        // Don't clamp to 0 - allow scrolling up to see content above
    }

    fn on_key_char(&mut self, _state: &mut EngineState, ch: char) {
        // Track held key for repeat (only for certain keys that should repeat)
        let should_repeat = match ch {
            ARROW_LEFT | ARROW_RIGHT | ARROW_UP | ARROW_DOWN | '\u{8}' | ' ' => true,
            _ if !ch.is_control() => true, // Regular characters
            _ => false,
        };
        
        if should_repeat {
            self.held_key = Some(ch);
            self.held_key_start_time = Some(Instant::now());
            self.last_repeat_time = None;
        }
        
        match ch {
            ARROW_LEFT => {
                self.move_cursor_left();
            }
            ARROW_RIGHT => {
                self.move_cursor_right();
            }
            ARROW_UP => {
                self.move_cursor_up();
            }
            ARROW_DOWN => {
                self.move_cursor_down();
            }
            '\t' => {
                // Insert tab at cursor position
                let text_chars: Vec<char> = self.text_engine.text.chars().collect();
                let mut new_text = String::new();
                for (i, &c) in text_chars.iter().enumerate() {
                    if i == self.cursor_position {
                        new_text.push_str("    ");
                    }
                    new_text.push(c);
                }
                if self.cursor_position >= text_chars.len() {
                    new_text.push_str("    ");
                }
                self.text_engine.text = new_text;
                self.cursor_position += 4;
            }
            '\r' | '\n' => {
                // Insert newline at cursor position
                let text_chars: Vec<char> = self.text_engine.text.chars().collect();
                let mut new_text = String::new();
                for (i, &c) in text_chars.iter().enumerate() {
                    if i == self.cursor_position {
                        new_text.push('\n');
                    }
                    new_text.push(c);
                }
                if self.cursor_position >= text_chars.len() {
                    new_text.push('\n');
                }
                self.text_engine.text = new_text;
                self.cursor_position += 1;
            }
            '\u{8}' => {
                // Backspace - delete character before cursor
                if self.cursor_position > 0 {
                    let text_chars: Vec<char> = self.text_engine.text.chars().collect();
                    let mut new_text = String::new();
                    for (i, &c) in text_chars.iter().enumerate() {
                        if i != self.cursor_position - 1 {
                            new_text.push(c);
                        }
                    }
                    self.text_engine.text = new_text;
                    self.cursor_position -= 1;
                }
            }
            _ => {
                if !ch.is_control() {
                    // Insert character at cursor position
                    let text_chars: Vec<char> = self.text_engine.text.chars().collect();
                    let mut new_text = String::new();
                    for (i, &c) in text_chars.iter().enumerate() {
                        if i == self.cursor_position {
                            new_text.push(ch);
                        }
                        new_text.push(c);
                    }
                    if self.cursor_position >= text_chars.len() {
                        new_text.push(ch);
                    }
                    self.text_engine.text = new_text;
                    self.cursor_position += 1;
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
        
        // Check if mouse moved significantly from tap position (start dragging)
        if !self.dragging && state.mouse.is_left_clicking {
            let dx = (state.mouse.x - self.last_tap_x).abs();
            let dy = (state.mouse.y - self.last_tap_y).abs();
            // Start dragging if moved more than 5 pixels
            if dx > 5.0 || dy > 5.0 {
                self.dragging = true;
                self.last_mouse_y = state.mouse.y;
            }
        }
        
        // Handle dragging for scrolling (like scroll.rs) - only scroll, don't move cursor
        if self.dragging {
            let dy = state.mouse.y - self.last_mouse_y;
            self.scroll_y -= dy;
            self.last_mouse_y = state.mouse.y;
        }
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
            // Mark that touch started on keyboard to prevent scrolling
            self.touch_started_on_keyboard = true;
            // Handle keyboard key press (handles dismiss button internally)
            if let Some(ch) = self.keyboard.check_key_press(state.mouse.x, state.mouse.y, width, height, Instant::now()) {
                // Route through on_key_char to ensure it works on all platforms
                self.on_key_char(state, ch);
            }
            return;
        }
        
        // Touch started outside keyboard
        self.touch_started_on_keyboard = false;
        
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
            // Single tap - move cursor to tap location (but don't start dragging yet)
            let safe_region = &state.frame.safe_region_boundaries;
            let content_top = safe_region.y1 * height;
            
            // Convert screen coordinates to text coordinates
            let tap_x = state.mouse.x;
            let tap_y = state.mouse.y - content_top + self.scroll_y;
            
            // Check if tap is on an empty line
            let mut found_line: Option<usize> = None;
            for (line_idx, line) in self.text_engine.lines.iter().enumerate() {
                let line_y = line.baseline_y;
                
                // Check if tap is within this line's vertical bounds
                if tap_y >= line_y - self.text_engine.ascent && tap_y <= line_y + self.text_engine.descent {
                    // Check if this line is empty (no characters in this line)
                    let has_chars = self.text_engine.characters.iter()
                        .any(|c| c.line_index == line_idx);
                    
                    if !has_chars {
                        // Empty line - place cursor at start of line
                        found_line = Some(line_idx);
                        self.cursor_position = line.start_index;
                        break;
                    }
                }
            }
            
            // If not on empty line, find nearest character
            if found_line.is_none() {
                let mut nearest_char_index = self.text_engine.text.chars().count();
                let mut min_distance_sq = f32::MAX;
                
                for character in &self.text_engine.characters {
                    let char_center_x = character.x + character.width / 2.0;
                    let char_center_y = character.y + character.height / 2.0;
                    
                    let dx = tap_x - char_center_x;
                    let dy = tap_y - char_center_y;
                    let distance_sq = dx * dx + dy * dy;
                    
                    // Check if tap is before this character horizontally
                    if tap_x < character.x && character.line_index == 0 {
                        // Tap is before this character, cursor should be at this character's index
                        if distance_sq < min_distance_sq {
                            min_distance_sq = distance_sq;
                            nearest_char_index = character.char_index;
                        }
                    } else if distance_sq < min_distance_sq {
                        min_distance_sq = distance_sq;
                        // If tap is to the right of character center, cursor goes after it
                        if tap_x > char_center_x {
                            nearest_char_index = character.char_index + 1;
                        } else {
                            nearest_char_index = character.char_index;
                        }
                    }
                }
                
                self.cursor_position = nearest_char_index.min(self.text_engine.text.chars().count());
            }
            
            // Update tap tracking
            self.last_tap_time = Some(now);
            self.last_tap_x = state.mouse.x;
            self.last_tap_y = state.mouse.y;
            
            // Don't start dragging immediately - wait for mouse movement
            // Dragging will be started in on_mouse_move if mouse moves significantly
        }
    }

    fn on_mouse_up(&mut self, _state: &mut EngineState) {
        // Release all keys when mouse is released
        self.keyboard.release_keys();
        // Stop dragging
        self.dragging = false;
        // Reset keyboard repeat tracking
        self.held_key = None;
        self.held_key_start_time = None;
        self.last_repeat_time = None;
        self.touch_started_on_keyboard = false;
    }
}

impl TextApp {
    fn move_cursor_left(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
        }
    }

    fn move_cursor_right(&mut self) {
        let text_len = self.text_engine.text.chars().count();
        if self.cursor_position < text_len {
            self.cursor_position += 1;
        }
    }

    fn move_cursor_up(&mut self) {
        // Find current line
        let line_idx_opt = self.text_engine.lines.iter()
            .enumerate()
            .find(|(_, line)| {
                line.start_index <= self.cursor_position && self.cursor_position <= line.end_index
            })
            .map(|(idx, _)| idx);
        
        if let Some(line_idx) = line_idx_opt {
            if line_idx > 0 {
                // Move to previous line
                let prev_line = &self.text_engine.lines[line_idx - 1];
                
                // Find current x position in current line
                let current_x = if let Some(char_at_cursor) = self.text_engine.characters.iter()
                    .find(|c| c.char_index == self.cursor_position) {
                    char_at_cursor.x
                } else if let Some(last_in_line) = self.text_engine.characters.iter()
                    .filter(|c| c.line_index == line_idx)
                    .last() {
                    last_in_line.x + last_in_line.metrics.advance_width
                } else {
                    0.0
                };
                
                // Find character in previous line closest to current_x
                let mut best_char_index = prev_line.end_index;
                let mut min_distance = f32::MAX;
                
                for character in self.text_engine.characters.iter()
                    .filter(|c| c.line_index == line_idx - 1) {
                    let distance = (character.x - current_x).abs();
                    if distance < min_distance {
                        min_distance = distance;
                        best_char_index = character.char_index;
                    }
                    // Also check position after this character
                    let after_distance = (character.x + character.metrics.advance_width - current_x).abs();
                    if after_distance < min_distance {
                        min_distance = after_distance;
                        best_char_index = character.char_index + 1;
                    }
                }
                
                self.cursor_position = best_char_index.min(prev_line.end_index);
            } else {
                // Already at first line, move to start
                self.cursor_position = 0;
            }
        }
    }

    fn move_cursor_down(&mut self) {
        // Find current line
        let line_idx_opt = self.text_engine.lines.iter()
            .enumerate()
            .find(|(_, line)| {
                line.start_index <= self.cursor_position && self.cursor_position <= line.end_index
            })
            .map(|(idx, _)| idx);
        
        if let Some(line_idx) = line_idx_opt {
            if line_idx < self.text_engine.lines.len() - 1 {
                // Move to next line
                let next_line = &self.text_engine.lines[line_idx + 1];
                
                // Find current x position in current line
                let current_x = if let Some(char_at_cursor) = self.text_engine.characters.iter()
                    .find(|c| c.char_index == self.cursor_position) {
                    char_at_cursor.x
                } else if let Some(last_in_line) = self.text_engine.characters.iter()
                    .filter(|c| c.line_index == line_idx)
                    .last() {
                    last_in_line.x + last_in_line.metrics.advance_width
                } else {
                    0.0
                };
                
                // Find character in next line closest to current_x
                let mut best_char_index = next_line.end_index;
                let mut min_distance = f32::MAX;
                
                for character in self.text_engine.characters.iter()
                    .filter(|c| c.line_index == line_idx + 1) {
                    let distance = (character.x - current_x).abs();
                    if distance < min_distance {
                        min_distance = distance;
                        best_char_index = character.char_index;
                    }
                    // Also check position after this character
                    let after_distance = (character.x + character.metrics.advance_width - current_x).abs();
                    if after_distance < min_distance {
                        min_distance = after_distance;
                        best_char_index = character.char_index + 1;
                    }
                }
                
                self.cursor_position = best_char_index.min(next_line.end_index);
            } else {
                // Already at last line, move to end
                self.cursor_position = self.text_engine.text.chars().count();
            }
        }
    }
}
