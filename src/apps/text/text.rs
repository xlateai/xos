use crate::engine::{Application, EngineState};
use crate::text::text_rasterization::TextRasterizer;
use crate::text::onscreen_keyboard::KeyType;
use crate::clipboard;
use crate::keyboard::shortcuts::ShortcutAction;
use fontdue::{Font, FontSettings};
use std::time::{Instant, Duration};

const BACKGROUND_COLOR: (u8, u8, u8) = (0, 0, 0);
const TEXT_COLOR: (u8, u8, u8) = (255, 255, 255);
const CURSOR_COLOR: (u8, u8, u8) = (0, 255, 0);
const BOUND_COLOR: (u8, u8, u8) = (255, 0, 0);
const BASELINE_COLOR: (u8, u8, u8) = (100, 100, 100);
const SELECTION_COLOR: (u8, u8, u8, u8) = (50, 120, 200, 128); // Semi-transparent blue

const SHOW_BOUNDING_RECTANGLES: bool = true;
const DRAW_BASELINES: bool = true;
const DOUBLE_TAP_TIME_MS: u64 = 300; // 300ms window for double tap
const DOUBLE_TAP_DISTANCE: f32 = 50.0; // Maximum distance between taps in pixels

// Scroll physics constants
const SCROLL_MOMENTUM_DECAY: f32 = 0.92; // How quickly momentum decays (0-1, higher = longer momentum)
const SCROLL_MOMENTUM_THRESHOLD: f32 = 0.1; // Stop momentum below this velocity
const SCROLL_ELASTIC_STRENGTH: f32 = 0.08; // Strength of elastic bounce at edges (lower = softer)
const SCROLL_VELOCITY_THRESHOLD_FOR_TAP: f32 = 5.0; // Don't trigger double-tap if scrolling faster than this

// Arrow key characters (using Unicode arrow symbols)
const ARROW_LEFT: char = '\u{2190}';  // ←
const ARROW_RIGHT: char = '\u{2192}'; // →
const ARROW_UP: char = '\u{2191}';    // ↑
const ARROW_DOWN: char = '\u{2193}';  // ↓

use std::collections::HashMap;

pub struct TextApp {
    pub text_rasterizer: TextRasterizer,
    pub scroll_y: f32,
    pub smooth_cursor_x: f32,
    pub fade_map: HashMap<(char, u32, u32), f32>,
    last_tap_time: Option<Instant>,
    last_tap_x: f32,
    last_tap_y: f32,
    last_tap_scrolled: bool, // Track if user scrolled between taps
    pub cursor_position: usize, // Character index where cursor should be
    dragging: bool,
    last_mouse_y: f32,
    touch_started_on_keyboard: bool,
    // Cursor positioning on release
    pending_cursor_tap_x: Option<f32>,
    pending_cursor_tap_y: Option<f32>,
    initial_scroll_y: f32,
    // Scroll momentum
    scroll_velocity: f32,
    last_frame_time: Option<Instant>,
    // Trackpad mode tracking
    trackpad_active: bool,
    trackpad_last_tap_time: Option<Instant>,
    trackpad_selecting: bool,
    trackpad_moved: bool, // Track if mouse moved during tap (to distinguish tap from drag)
    // Trackpad laser pointer
    trackpad_laser_x: Option<f32>, // Screen coordinates
    trackpad_laser_y: Option<f32>, // Screen coordinates
    trackpad_last_mouse_x: Option<f32>,
    trackpad_last_mouse_y: Option<f32>,
    // Clipboard
    clipboard_content: String,
    // Undo/redo history
    undo_stack: Vec<(String, usize)>, // (text, cursor_position)
    redo_stack: Vec<(String, usize)>,
    // Text selection state
    selection_start: Option<usize>, // Character index where selection starts
    selection_end: Option<usize>,   // Character index where selection ends
    selecting: bool,                // True when actively selecting text (dragging)
    // Configuration flags
    pub show_cursor: bool,
    pub show_debug_visuals: bool,
    pub read_only: bool,
}


impl TextApp {
    pub fn new() -> Self {
        let font_bytes = include_bytes!("../../../assets/JetBrainsMono-Regular.ttf") as &[u8];
        // let font_bytes = include_bytes!("../../../assets/NotoSansJP-Regular.ttf") as &[u8];
        let font = Font::from_bytes(font_bytes, FontSettings::default()).expect("Failed to load font");

        // Increase font size by 10% on iOS
        let base_font_size = 48.0;
        let font_size = if cfg!(target_os = "ios") {
            base_font_size * 1.1 // 10% larger on iOS
        } else {
            base_font_size
        };

        let mut text_rasterizer = TextRasterizer::new(font, font_size);

        // Set default text on iOS
        let initial_cursor_pos = if cfg!(target_os = "ios") {
            let default_text = "double tap screen to open keyboard".to_string();
            let cursor_pos = default_text.chars().count();
            text_rasterizer.set_text(default_text);
            cursor_pos
        } else {
            0
        };

        Self {
            text_rasterizer,
            scroll_y: 0.0, // Always start at 0 (top of safe region)
            smooth_cursor_x: 0.0,
            fade_map: HashMap::new(),
            last_tap_time: None,
            last_tap_x: 0.0,
            last_tap_y: 0.0,
            last_tap_scrolled: false,
            cursor_position: initial_cursor_pos,
            dragging: false,
            last_mouse_y: 0.0,
            touch_started_on_keyboard: false,
            pending_cursor_tap_x: None,
            pending_cursor_tap_y: None,
            initial_scroll_y: 0.0,
            scroll_velocity: 0.0,
            last_frame_time: None,
            trackpad_active: false,
            trackpad_last_tap_time: None,
            trackpad_selecting: false,
            trackpad_moved: false,
            trackpad_laser_x: None,
            trackpad_laser_y: None,
            trackpad_last_mouse_x: None,
            trackpad_last_mouse_y: None,
            clipboard_content: String::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            selection_start: None,
            selection_end: None,
            selecting: false,
            show_cursor: true,
            show_debug_visuals: true,
            read_only: false,
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
    fn setup(&mut self, state: &mut EngineState) -> Result<(), String> {
        // On iOS, initialize scroll to 0 (text starts at safe region top)
        if cfg!(target_os = "ios") && !self.text_rasterizer.text.is_empty() {
            let shape = state.frame.array.shape();
            let height = shape[0] as f32;
            let safe_region = &state.frame.safe_region_boundaries;
            let content_top = safe_region.y1 * height;
            let content_height = height - content_top;
            
            // Tick the engine once to calculate line positions
            self.text_rasterizer.tick(shape[1] as f32, content_height);
            
            // Start with scroll at 0 (text at top of safe region)
            self.scroll_y = 0.0;
        }
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        // Process any pending keyboard characters first
        while let Some(ch) = state.keyboard.onscreen.pop_pending_char() {
            self.on_key_char(state, ch);
        }
        
        // Process action keys from keyboard
        if let Some(action) = state.keyboard.onscreen.get_last_action_key() {
            self.handle_action_key(action, state);
        }
        
        // Process action key repeats (for undo/redo hold)
        let now = Instant::now();
        if let Some(action) = state.keyboard.onscreen.check_action_key_hold_repeat(now) {
            self.handle_action_key(action, state);
        }
        
        // Extract all needed values in a block to release borrows
        let (width, height, content_top, content_bottom, is_trackpad_mode, is_keyboard_shown) = {
            let shape = state.frame.array.shape();
            let width = shape[1] as f32;
            let height = shape[0] as f32;
            
            let safe_region = &state.frame.safe_region_boundaries;
            
            // Get keyboard top edge (whether visible or not)
            let (_, keyboard_top_y, _, _) = state.keyboard.onscreen.top_edge_coordinates();
            
            let content_top = safe_region.y1 * height; // Top of safe region
            let content_bottom = keyboard_top_y * height; // Top of keyboard area
            
            let is_trackpad_mode = state.keyboard.onscreen.is_trackpad_mode();
            let is_keyboard_shown = state.keyboard.onscreen.is_shown();
            
            (width, height, content_top, content_bottom, is_trackpad_mode, is_keyboard_shown)
        };
        
        // === SIMPLIFIED SCROLL PHYSICS ===
        
        // 1. Apply momentum with decay
        let current_time = Instant::now();
        if let Some(last_time) = self.last_frame_time {
            let dt = current_time.duration_since(last_time).as_secs_f32();
            
            // Apply velocity to scroll position
            if self.scroll_velocity.abs() > SCROLL_MOMENTUM_THRESHOLD {
                self.scroll_y += self.scroll_velocity * dt * 60.0; // Normalize to 60 FPS
                self.scroll_velocity *= SCROLL_MOMENTUM_DECAY; // Decay momentum
            } else {
                self.scroll_velocity = 0.0; // Stop if below threshold
            }
        }
        self.last_frame_time = Some(current_time);
        
        // 2. Calculate elastic bounds based on safe regions
        // Text should be able to scroll from top safe region to bottom safe region
        // With symmetric overscroll allowance beyond both edges
        let visible_height = content_bottom - content_top;
        
        // Natural bounds: 0 (top of text) to height of all text
        let line_height = self.text_rasterizer.ascent + self.text_rasterizer.descent.abs() + self.text_rasterizer.line_gap;
        let text_content_height = if !self.text_rasterizer.lines.is_empty() {
            let first_y = self.text_rasterizer.lines.first().map(|l| l.baseline_y).unwrap_or(0.0);
            let last_y = self.text_rasterizer.lines.last().map(|l| l.baseline_y).unwrap_or(0.0);
            (last_y - first_y).abs() + line_height * 2.0
        } else {
            line_height
        };
        
        // 3. Apply elastic resistance when beyond natural bounds
        // Keep text on screen: don't let it scroll completely off either edge
        let natural_min = 0.0;
        let natural_max = (text_content_height - visible_height).max(0.0); // Keep at least one screen visible
        
        if self.scroll_y < natural_min {
            // Scrolled above top - pull back
            let overshoot = natural_min - self.scroll_y;
            self.scroll_y += overshoot * SCROLL_ELASTIC_STRENGTH;
            self.scroll_velocity *= 0.8; // Dampen momentum at edge
        } else if self.scroll_y > natural_max {
            // Scrolled below bottom - pull back
            let overshoot = self.scroll_y - natural_max;
            self.scroll_y -= overshoot * SCROLL_ELASTIC_STRENGTH;
            self.scroll_velocity *= 0.8; // Dampen momentum at edge
        }
        
        // 4. Hard clamp to keep text on screen (with small overscroll allowance)
        let hard_min = natural_min - line_height; // Allow one line height overscroll up
        let hard_max = natural_max + line_height; // Allow one line height overscroll down
        self.scroll_y = self.scroll_y.max(hard_min).min(hard_max);
        
        // Now get mutable buffer (after all immutable borrows are released)
        let buffer = state.frame_buffer_mut();
    
        self.text_rasterizer.tick(width, content_bottom - content_top);
    
        // Clear screen
        for i in (0..buffer.len()).step_by(4) {
            buffer[i + 0] = BACKGROUND_COLOR.0;
            buffer[i + 1] = BACKGROUND_COLOR.1;
            buffer[i + 2] = BACKGROUND_COLOR.2;
            buffer[i + 3] = 0xff;
        }
    
        // Draw baselines (offset by content_top)
        if DRAW_BASELINES && self.show_debug_visuals {
            for line in &self.text_rasterizer.lines {
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
        
        // Draw selection highlighting
        if let (Some(sel_start), Some(sel_end)) = (self.selection_start, self.selection_end) {
            let (start_idx, end_idx) = if sel_start <= sel_end {
                (sel_start, sel_end)
            } else {
                (sel_end, sel_start)
            };
            
            // Group selected characters by line and find min/max x positions per line
            use std::collections::HashMap;
            let mut line_selections: HashMap<usize, (f32, f32, f32)> = HashMap::new(); // line_idx -> (min_x, max_x, baseline_y)
            
            for character in &self.text_rasterizer.characters {
                if character.char_index >= start_idx && character.char_index < end_idx {
                    let char_left = character.x;
                    let char_right = character.x + character.metrics.advance_width;
                    
                    line_selections.entry(character.line_index)
                        .and_modify(|(min_x, max_x, baseline_y)| {
                            *min_x = min_x.min(char_left);
                            *max_x = max_x.max(char_right);
                            *baseline_y = self.text_rasterizer.lines.get(character.line_index)
                                .map(|line| line.baseline_y)
                                .unwrap_or(*baseline_y);
                        })
                        .or_insert_with(|| {
                            let baseline_y = self.text_rasterizer.lines.get(character.line_index)
                                .map(|line| line.baseline_y)
                                .unwrap_or(0.0);
                            (char_left, char_right, baseline_y)
                        });
                }
            }
            
            // Draw selection rectangles per line
            for (_line_idx, (min_x, max_x, baseline_y)) in line_selections.iter() {
                let sel_left = *min_x as i32;
                let sel_right = *max_x as i32;
                let sel_top = ((baseline_y - self.text_rasterizer.ascent - self.scroll_y) + content_top) as i32;
                let sel_bottom = ((baseline_y + self.text_rasterizer.descent - self.scroll_y) + content_top) as i32;
                
                // Draw semi-transparent selection rectangle
                for y in sel_top..sel_bottom {
                    for x in sel_left..sel_right {
                        if x >= 0 && x < width as i32 && y >= 0 && y < height as i32 {
                            let idx = ((y as u32 * width as u32 + x as u32) * 4) as usize;
                            // Alpha blend the selection color
                            let alpha = SELECTION_COLOR.3 as f32 / 255.0;
                            let inv_alpha = 1.0 - alpha;
                            buffer[idx + 0] = (buffer[idx + 0] as f32 * inv_alpha + SELECTION_COLOR.0 as f32 * alpha) as u8;
                            buffer[idx + 1] = (buffer[idx + 1] as f32 * inv_alpha + SELECTION_COLOR.1 as f32 * alpha) as u8;
                            buffer[idx + 2] = (buffer[idx + 2] as f32 * inv_alpha + SELECTION_COLOR.2 as f32 * alpha) as u8;
                        }
                    }
                }
            }
        }
    
        // Draw characters with fade and slide-in (offset by content_top)
        for character in &self.text_rasterizer.characters {
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
    
            if SHOW_BOUNDING_RECTANGLES && self.show_debug_visuals {
                Self::draw_rect(buffer, width as u32, height as u32, px, py, pw, ph, alpha);
            }
        }
    
        // Draw cursor (offset by content_top)
        if self.show_cursor {
            let (target_x, baseline_y) = self.get_cursor_screen_position();
        
            // Smooth the x-position (linear interpolation)
            self.smooth_cursor_x += (target_x - self.smooth_cursor_x) * 0.2;
            
            let cursor_top = ((baseline_y - self.text_rasterizer.ascent - self.scroll_y) + content_top).round() as i32;
            let cursor_bottom = ((baseline_y + self.text_rasterizer.descent - self.scroll_y) + content_top).round() as i32;
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
        }
        
        // Draw trackpad laser pointer if in trackpad mode AND keyboard is visible
        if is_trackpad_mode && is_keyboard_shown {
            // Initialize laser if not already set
            if self.trackpad_laser_x.is_none() || self.trackpad_laser_y.is_none() {
                // Center of available content area (using already extracted values)
                self.trackpad_laser_x = Some(width / 2.0);
                self.trackpad_laser_y = Some((content_top + content_bottom) / 2.0);
            }
            
            if let (Some(laser_x), Some(laser_y)) = (self.trackpad_laser_x, self.trackpad_laser_y) {
                let dot_radius = 4.0;
                let dot_x_i = laser_x.round() as i32;
                let dot_y_i = laser_y.round() as i32;
                
                // Draw a simple solid red dot
                for dy in -(dot_radius as i32)..=(dot_radius as i32) {
                    for dx in -(dot_radius as i32)..=(dot_radius as i32) {
                        let distance = ((dx * dx + dy * dy) as f32).sqrt();
                        if distance <= dot_radius {
                            let x = dot_x_i + dx;
                            let y = dot_y_i + dy;
                            if x >= 0 && x < width as i32 && y >= 0 && y < height as i32 {
                                let idx = ((y as u32 * width as u32 + x as u32) * 4) as usize;
                                // Simple bright red dot
                                buffer[idx + 0] = 255; // R
                                buffer[idx + 1] = 0;   // G
                                buffer[idx + 2] = 0;   // B
                                buffer[idx + 3] = 255; // A
                            }
                        }
                    }
                }
            }
        }
    }
    

    fn on_scroll(&mut self, _state: &mut EngineState, _dx: f32, dy: f32) {
        // Add to scroll velocity for momentum (accumulates with multiple flicks)
        self.scroll_velocity += dy;
        // Mark that scrolling occurred (for double-tap detection)
        self.last_tap_scrolled = true;
    }

    fn on_key_char(&mut self, _state: &mut EngineState, ch: char) {
        // Don't process keys if read-only
        if self.read_only {
            return;
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
                self.save_undo_state();
                // Delete selection if present
                self.delete_selection();
                // Insert tab at cursor position
                let text_chars: Vec<char> = self.text_rasterizer.text.chars().collect();
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
                self.text_rasterizer.text = new_text;
                self.cursor_position += 4;
            }
            '\r' | '\n' => {
                self.save_undo_state();
                // Delete selection if present
                self.delete_selection();
                // Insert newline at cursor position
                let text_chars: Vec<char> = self.text_rasterizer.text.chars().collect();
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
                self.text_rasterizer.text = new_text;
                self.cursor_position += 1;
            }
            '\u{8}' => {
                // Backspace - delete selection if present, otherwise delete character before cursor
                self.save_undo_state();
                if !self.delete_selection() {
                    // No selection, delete character before cursor
                    if self.cursor_position > 0 {
                        let text_chars: Vec<char> = self.text_rasterizer.text.chars().collect();
                        let mut new_text = String::new();
                        for (i, &c) in text_chars.iter().enumerate() {
                            if i != self.cursor_position - 1 {
                                new_text.push(c);
                            }
                        }
                        self.text_rasterizer.text = new_text;
                        self.cursor_position -= 1;
                    }
                }
            }
            _ => {
                if !ch.is_control() {
                    self.save_undo_state();
                    // Delete selection if present
                    self.delete_selection();
                    // Insert character at cursor position
                    let text_chars: Vec<char> = self.text_rasterizer.text.chars().collect();
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
                    self.text_rasterizer.text = new_text;
                    self.cursor_position += 1;
                }
            }
        }
    }

    fn on_mouse_move(&mut self, state: &mut EngineState) {
        let shape = state.frame.array.shape();
        let width = shape[1] as f32;
        let height = shape[0] as f32;
        
        // Check if we're in trackpad mode AND actively using it
        if state.keyboard.onscreen.is_trackpad_mode() {
            // Initialize laser if not set (center of content area)
            if self.trackpad_laser_x.is_none() || self.trackpad_laser_y.is_none() {
                let safe_region = &state.frame.safe_region_boundaries;
                let content_top = safe_region.y1 * height;
                let (_, keyboard_top_y, _, _) = state.keyboard.onscreen.top_edge_coordinates();
                let content_bottom = keyboard_top_y * height;
                
                // Center of available content area
                self.trackpad_laser_x = Some(width / 2.0);
                self.trackpad_laser_y = Some((content_top + content_bottom) / 2.0);
            }
            
            // If mouse is in trackpad area and active (dragging), move the laser
            if self.trackpad_active && state.mouse.is_left_clicking {
                if let (Some(laser_x), Some(laser_y), Some(last_mouse_x), Some(last_mouse_y)) = 
                    (self.trackpad_laser_x, self.trackpad_laser_y, self.trackpad_last_mouse_x, self.trackpad_last_mouse_y) {
                    
                    let mouse_dx = state.mouse.x - last_mouse_x;
                    let mouse_dy = state.mouse.y - last_mouse_y;
                    
                    // Track if mouse moved (for tap vs drag detection)
                    if mouse_dx.abs() > 2.0 || mouse_dy.abs() > 2.0 {
                        self.trackpad_moved = true;
                    }
                    
                    // Move laser 2x with mouse movement (double speed)
                    let new_laser_x = (laser_x + mouse_dx * 2.0).max(0.0).min(width);
                    
                    // Constrain laser to stay above keyboard
                    let safe_region = &state.frame.safe_region_boundaries;
                    let content_top = safe_region.y1 * height;
                    let (_, keyboard_top_y, _, _) = state.keyboard.onscreen.top_edge_coordinates();
                    let keyboard_top = keyboard_top_y * height;
                    
                    let new_laser_y = (laser_y + mouse_dy * 2.0).max(content_top).min(keyboard_top);
                    
                    self.trackpad_laser_x = Some(new_laser_x);
                    self.trackpad_laser_y = Some(new_laser_y);
                    
                    // Update cursor position based on laser
                    let safe_region = &state.frame.safe_region_boundaries;
                    let content_top = safe_region.y1 * height;
                    let text_x = new_laser_x;
                    let text_y = new_laser_y - content_top + self.scroll_y;
                    
                    let char_index = self.find_nearest_char_index(text_x, text_y);
                    
                    // If selecting, update selection end; otherwise just move cursor
                    if self.trackpad_selecting {
                        self.selection_end = Some(char_index);
                    }
                    self.cursor_position = char_index;
                }
                
                // Update last mouse position
                self.trackpad_last_mouse_x = Some(state.mouse.x);
                self.trackpad_last_mouse_y = Some(state.mouse.y);
                
                // Don't allow normal scrolling when actively using trackpad
                return;
            }
        } else {
            // Not in trackpad mode - clear laser
            self.trackpad_laser_x = None;
            self.trackpad_laser_y = None;
            self.trackpad_last_mouse_x = None;
            self.trackpad_last_mouse_y = None;
        }
        
        // Don't allow scrolling if touch started on keyboard
        if self.touch_started_on_keyboard {
            return;
        }
        
        // Check if mouse moved significantly from tap position (start dragging or selecting)
        if !self.dragging && !self.selecting && state.mouse.is_left_clicking {
            let dx = (state.mouse.x - self.last_tap_x).abs();
            let dy = (state.mouse.y - self.last_tap_y).abs();
            // Start dragging/selecting if moved more than 5 pixels
            if dx > 5.0 || dy > 5.0 {
                // When keyboard is shown (mobile mode): vertical is scroll, horizontal is selection
                // When keyboard is hidden (desktop mode): horizontal is selection, vertical is scroll
                if state.keyboard.onscreen.is_shown() {
                    // Mobile mode (keyboard visible): vertical drag scrolls, horizontal drag selects
                    if dy > dx {
                        // Vertical movement dominates - scroll
                        self.dragging = true;
                        self.last_mouse_y = state.mouse.y;
                    } else {
                        // Horizontal movement dominates - select
                        self.selecting = true;
                        // Get character index at initial tap position for selection start
                        let safe_region = &state.frame.safe_region_boundaries;
                        let content_top = safe_region.y1 * height;
                        let text_x = self.last_tap_x;
                        let text_y = self.last_tap_y - content_top + self.scroll_y;
                        let start_char_idx = self.find_nearest_char_index(text_x, text_y);
                        
                        self.selection_start = Some(start_char_idx);
                        self.selection_end = Some(start_char_idx);
                        self.cursor_position = start_char_idx;
                    }
                } else {
                    // Desktop mode (keyboard hidden): horizontal drag selects, vertical drag scrolls
                    if dx > dy {
                        self.selecting = true;
                        let safe_region = &state.frame.safe_region_boundaries;
                        let content_top = safe_region.y1 * height;
                        let text_x = self.last_tap_x;
                        let text_y = self.last_tap_y - content_top + self.scroll_y;
                        let start_char_idx = self.find_nearest_char_index(text_x, text_y);
                        
                        self.selection_start = Some(start_char_idx);
                        self.selection_end = Some(start_char_idx);
                        self.cursor_position = start_char_idx;
                    } else {
                        self.dragging = true;
                        self.last_mouse_y = state.mouse.y;
                    }
                }
            }
        }
        
        // Handle text selection while dragging
        if self.selecting && state.mouse.is_left_clicking {
            let safe_region = &state.frame.safe_region_boundaries;
            let content_top = safe_region.y1 * height;
            
            // Convert mouse coordinates to text coordinates
            let text_x = state.mouse.x;
            let text_y = state.mouse.y - content_top + self.scroll_y;
            
            // Find nearest character to mouse position
            let char_index = self.find_nearest_char_index(text_x, text_y);
            self.selection_end = Some(char_index);
            self.cursor_position = char_index;
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
        let height = shape[0] as f32;
        
        // Check if keyboard handled the event
        if state.keyboard.onscreen.on_mouse_down(state.mouse.x, state.mouse.y, shape[1] as f32, height) {
            // Mark that touch started on keyboard to prevent scrolling
            self.touch_started_on_keyboard = true;
            return;
        }
        
        // Check if we're in trackpad mode and clicking in the trackpad area
        if state.keyboard.onscreen.is_trackpad_mode() {
            let (_, keyboard_top_y, _, _) = state.keyboard.onscreen.top_edge_coordinates();
            let keyboard_region_top = keyboard_top_y * height;
            
            // Check if clicking in the trackpad area (keyboard region)
            if state.mouse.y >= keyboard_region_top {
                self.trackpad_active = true;
                
                // Initialize laser position and last mouse position
                if self.trackpad_laser_x.is_none() || self.trackpad_laser_y.is_none() {
                    let safe_region = &state.frame.safe_region_boundaries;
                    let content_top = safe_region.y1 * height;
                    let shape = state.frame.array.shape();
                    let width = shape[1] as f32;
                    let content_bottom = keyboard_region_top;
                    
                    // Center of available content area
                    self.trackpad_laser_x = Some(width / 2.0);
                    self.trackpad_laser_y = Some((content_top + content_bottom) / 2.0);
                }
                
                self.trackpad_last_mouse_x = Some(state.mouse.x);
                self.trackpad_last_mouse_y = Some(state.mouse.y);
                
                // Check for double-tap to start selection
                let now = Instant::now();
                let is_double_tap = if let Some(last_time) = self.trackpad_last_tap_time {
                    let time_since_last = now.duration_since(last_time);
                    time_since_last < Duration::from_millis(DOUBLE_TAP_TIME_MS)
                } else {
                    false
                };
                
                if is_double_tap {
                    // Start selection mode
                    self.trackpad_selecting = true;
                    self.selection_start = Some(self.cursor_position);
                    self.selection_end = Some(self.cursor_position);
                    self.trackpad_last_tap_time = None; // Reset to prevent triple-tap
                } else {
                    // Record tap time (selection will be cleared on release if no drag)
                    self.trackpad_last_tap_time = Some(now);
                }
                
                // Reset moved flag
                self.trackpad_moved = false;
                
                return;
            }
        }
        
        // Touch started outside keyboard or keyboard is hidden
        self.touch_started_on_keyboard = false;
        
        // Get keyboard region for double-tap detection
        let (_, keyboard_top_y, _, _) = state.keyboard.onscreen.top_edge_coordinates();
        let keyboard_region_top = keyboard_top_y * height;
        
        // Check for double tap in content area OR in keyboard region (even if hidden)
        // Only trigger if user didn't scroll between taps AND velocity is low
        let now = Instant::now();
        let is_double_tap = if let Some(last_time) = self.last_tap_time {
            let time_since_last = now.duration_since(last_time);
            let distance = ((state.mouse.x - self.last_tap_x).powi(2) + (state.mouse.y - self.last_tap_y).powi(2)).sqrt();
            
            time_since_last < Duration::from_millis(DOUBLE_TAP_TIME_MS) 
                && distance < DOUBLE_TAP_DISTANCE 
                && !self.last_tap_scrolled // Don't trigger if user scrolled
                && self.scroll_velocity.abs() < SCROLL_VELOCITY_THRESHOLD_FOR_TAP // Don't trigger if fast scrolling
        } else {
            false
        };
        
        if is_double_tap {
            // Toggle keyboard (works from content area or keyboard region)
            state.keyboard.onscreen.toggle_minimize();
            // Reset tap tracking to prevent triple-tap from immediately closing
            self.last_tap_time = None;
            self.last_tap_scrolled = false;
            // Clear pending cursor position
            self.pending_cursor_tap_x = None;
            self.pending_cursor_tap_y = None;
            return; // Don't process cursor positioning for double-tap
        }
        
        // If single tap in keyboard region while keyboard is hidden, don't move cursor
        if state.mouse.y >= keyboard_region_top && !state.keyboard.onscreen.is_shown() {
            // Just update tap tracking for potential double-tap
            self.last_tap_time = Some(now);
            self.last_tap_x = state.mouse.x;
            self.last_tap_y = state.mouse.y;
            self.last_tap_scrolled = false; // Reset scroll flag for new tap
            return;
        }
        
        // Normal single tap in content area
        // Single tap - record position but don't move cursor yet
        // We'll move cursor on mouse up if user didn't scroll
        self.pending_cursor_tap_x = Some(state.mouse.x);
        self.pending_cursor_tap_y = Some(state.mouse.y);
        self.initial_scroll_y = self.scroll_y;
        
        // Clear any existing selection when starting a new interaction
        self.selection_start = None;
        self.selection_end = None;
        
        // Update tap tracking
        self.last_tap_time = Some(now);
        self.last_tap_x = state.mouse.x;
        self.last_tap_y = state.mouse.y;
        self.last_tap_scrolled = false; // Reset scroll flag for new tap
        
        // Don't start dragging immediately - wait for mouse movement
        // Dragging will be started in on_mouse_move if mouse moves significantly
    }

    fn on_mouse_up(&mut self, state: &mut EngineState) {
        // Release all keyboard keys
        state.keyboard.onscreen.on_mouse_up();
        
        // Check if we should move cursor (only if user didn't scroll and didn't drag/select)
        if let (Some(tap_x), Some(tap_y)) = (self.pending_cursor_tap_x, self.pending_cursor_tap_y) {
            // Check if user scrolled (scroll_y changed significantly)
            let scroll_delta = (self.scroll_y - self.initial_scroll_y).abs();
            let scroll_threshold = 1.0; // pixels
            
            // Check if user dragged (moved mouse significantly)
            let drag_distance = ((state.mouse.x - tap_x).powi(2) + (state.mouse.y - tap_y).powi(2)).sqrt();
            let drag_threshold = 10.0; // pixels
            
            // Only move cursor if user didn't scroll and didn't drag/select
            if scroll_delta < scroll_threshold && !self.selecting && (!self.dragging || drag_distance < drag_threshold) {
                // Move cursor to tap location
                let shape = state.frame.array.shape();
                let height = shape[0] as f32;
                let safe_region = &state.frame.safe_region_boundaries;
                let content_top = safe_region.y1 * height;
                
                // Convert screen coordinates to text coordinates (use current scroll_y)
                let text_x = tap_x;
                let text_y = tap_y - content_top + self.scroll_y;
                
                let char_index = self.find_nearest_char_index(text_x, text_y);
                self.cursor_position = char_index;
                
                // Clear selection when clicking without drag
                self.selection_start = None;
                self.selection_end = None;
            }
            
            // Clear pending cursor position
            self.pending_cursor_tap_x = None;
            self.pending_cursor_tap_y = None;
        }
        
        // Stop dragging and selecting
        self.dragging = false;
        self.selecting = false;
        // Reset touch tracking
        self.touch_started_on_keyboard = false;
        // Clear selection on tap release if no drag occurred
        if self.trackpad_active && !self.trackpad_moved && !self.trackpad_selecting {
            self.selection_start = None;
            self.selection_end = None;
        }
        
        // Clear trackpad tracking (but keep laser visible)
        self.trackpad_active = false;
        self.trackpad_selecting = false;
        self.trackpad_moved = false;
        self.trackpad_last_mouse_x = None;
        self.trackpad_last_mouse_y = None;
    }
    
    fn on_key_shortcut(&mut self, state: &mut EngineState, shortcut: ShortcutAction) {
        // Map shortcuts to KeyType actions
        let key_type = match shortcut {
            ShortcutAction::Copy => KeyType::Copy,
            ShortcutAction::Cut => KeyType::Cut,
            ShortcutAction::Paste => KeyType::Paste,
            ShortcutAction::SelectAll => KeyType::SelectAll,
            ShortcutAction::Undo => KeyType::Undo,
            ShortcutAction::Redo => KeyType::Redo,
        };
        self.handle_action_key(key_type, state);
    }
}

impl TextApp {
    fn move_cursor_left(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
        }
    }

    fn move_cursor_right(&mut self) {
        let text_len = self.text_rasterizer.text.chars().count();
        if self.cursor_position < text_len {
            self.cursor_position += 1;
        }
    }

    fn move_cursor_up(&mut self) {
        // Find current line
        let line_idx_opt = self.text_rasterizer.lines.iter()
            .enumerate()
            .find(|(_, line)| {
                line.start_index <= self.cursor_position && self.cursor_position <= line.end_index
            })
            .map(|(idx, _)| idx);
        
        if let Some(line_idx) = line_idx_opt {
            if line_idx > 0 {
                // Move to previous line
                let prev_line = &self.text_rasterizer.lines[line_idx - 1];
                
                // Find current x position in current line
                let current_x = if let Some(char_at_cursor) = self.text_rasterizer.characters.iter()
                    .find(|c| c.char_index == self.cursor_position) {
                    char_at_cursor.x
                } else if let Some(last_in_line) = self.text_rasterizer.characters.iter()
                    .filter(|c| c.line_index == line_idx)
                    .last() {
                    last_in_line.x + last_in_line.metrics.advance_width
                } else {
                    0.0
                };
                
                // Find character in previous line closest to current_x
                let mut best_char_index = prev_line.end_index;
                let mut min_distance = f32::MAX;
                
                for character in self.text_rasterizer.characters.iter()
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
        let line_idx_opt = self.text_rasterizer.lines.iter()
            .enumerate()
            .find(|(_, line)| {
                line.start_index <= self.cursor_position && self.cursor_position <= line.end_index
            })
            .map(|(idx, _)| idx);
        
        if let Some(line_idx) = line_idx_opt {
            if line_idx < self.text_rasterizer.lines.len() - 1 {
                // Move to next line
                let next_line = &self.text_rasterizer.lines[line_idx + 1];
                
                // Find current x position in current line
                let current_x = if let Some(char_at_cursor) = self.text_rasterizer.characters.iter()
                    .find(|c| c.char_index == self.cursor_position) {
                    char_at_cursor.x
                } else if let Some(last_in_line) = self.text_rasterizer.characters.iter()
                    .filter(|c| c.line_index == line_idx)
                    .last() {
                    last_in_line.x + last_in_line.metrics.advance_width
                } else {
                    0.0
                };
                
                // Find character in next line closest to current_x
                let mut best_char_index = next_line.end_index;
                let mut min_distance = f32::MAX;
                
                for character in self.text_rasterizer.characters.iter()
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
                self.cursor_position = self.text_rasterizer.text.chars().count();
            }
        }
    }
    
    fn find_nearest_char_index(&self, text_x: f32, text_y: f32) -> usize {
        // Check if tap is on an empty line
        for (line_idx, line) in self.text_rasterizer.lines.iter().enumerate() {
            let line_y = line.baseline_y;
            
            // Check if tap is within this line's vertical bounds
            if text_y >= line_y - self.text_rasterizer.ascent && text_y <= line_y + self.text_rasterizer.descent {
                // Check if this line is empty (no characters in this line)
                let has_chars = self.text_rasterizer.characters.iter()
                    .any(|c| c.line_index == line_idx);
                
                if !has_chars {
                    // Empty line - place cursor at start of line
                    return line.start_index;
                }
            }
        }
        
        // If not on empty line, find nearest character
        let mut nearest_char_index = self.text_rasterizer.text.chars().count();
        let mut min_distance_sq = f32::MAX;
        
        for character in &self.text_rasterizer.characters {
            let char_center_x = character.x + character.width / 2.0;
            let char_center_y = character.y + character.height / 2.0;
            
            let dx = text_x - char_center_x;
            let dy = text_y - char_center_y;
            let distance_sq = dx * dx + dy * dy;
            
            // Check if tap is before this character horizontally
            if text_x < character.x && character.line_index == 0 {
                // Tap is before this character, cursor should be at this character's index
                if distance_sq < min_distance_sq {
                    min_distance_sq = distance_sq;
                    nearest_char_index = character.char_index;
                }
            } else if distance_sq < min_distance_sq {
                min_distance_sq = distance_sq;
                // If tap is to the right of character center, cursor goes after it
                if text_x > char_center_x {
                    nearest_char_index = character.char_index + 1;
                } else {
                    nearest_char_index = character.char_index;
                }
            }
        }
        
        nearest_char_index.min(self.text_rasterizer.text.chars().count())
    }
    
    fn save_undo_state(&mut self) {
        // Save current state to undo stack
        self.undo_stack.push((self.text_rasterizer.text.clone(), self.cursor_position));
        // Limit undo stack size
        if self.undo_stack.len() > 100 {
            self.undo_stack.remove(0);
        }
        // Clear redo stack when new action is performed
        self.redo_stack.clear();
    }
    
    /// Get the cursor position in text coordinates (x, y)
    fn get_cursor_screen_position(&self) -> (f32, f32) {
        // Find cursor position based on cursor_position index
        // First, find which line the cursor is on
        let line_info_with_idx = self.text_rasterizer.lines.iter()
            .enumerate()
            .find(|(_, line)| {
                line.start_index <= self.cursor_position && self.cursor_position <= line.end_index
            });
        
        if let Some((line_idx, line)) = line_info_with_idx {
            // Found the line - check if there are characters in this line
            let chars_in_line: Vec<_> = self.text_rasterizer.characters.iter()
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
                    
                    for character in self.text_rasterizer.characters.iter() {
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
            if let Some(first_line) = self.text_rasterizer.lines.first() {
                (0.0, first_line.baseline_y)
            } else {
                (0.0, self.text_rasterizer.ascent)
            }
        } else if self.cursor_position >= self.text_rasterizer.text.chars().count() {
            // Cursor at end of text
            if let Some(last_line) = self.text_rasterizer.lines.last() {
                // Find the line index
                let last_line_idx = self.text_rasterizer.lines.len() - 1;
                // Check if last line has characters
                let chars_in_last_line: Vec<_> = self.text_rasterizer.characters.iter()
                    .filter(|c| c.line_index == last_line_idx)
                    .collect();
                
                if chars_in_last_line.is_empty() {
                    (0.0, last_line.baseline_y)
                } else if let Some(last_char) = chars_in_last_line.last() {
                    (last_char.x + last_char.metrics.advance_width, last_line.baseline_y)
                } else {
                    (0.0, last_line.baseline_y)
                }
            } else if let Some(last) = self.text_rasterizer.characters.last() {
                (last.x + last.metrics.advance_width, self.text_rasterizer.lines.last().map_or(self.text_rasterizer.ascent, |line| line.baseline_y))
            } else {
                (0.0, self.text_rasterizer.ascent)
            }
        } else {
            (0.0, self.text_rasterizer.ascent)
        }
    }
    
    /// Delete the current selection and return true if a selection was deleted
    fn delete_selection(&mut self) -> bool {
        if let (Some(start), Some(end)) = (self.selection_start, self.selection_end) {
            let (start_idx, end_idx) = if start <= end { (start, end) } else { (end, start) };
            let text_chars: Vec<char> = self.text_rasterizer.text.chars().collect();
            
            // Build new text without selected range
            let mut new_text = String::new();
            for (i, &c) in text_chars.iter().enumerate() {
                if i < start_idx || i >= end_idx {
                    new_text.push(c);
                }
            }
            
            self.text_rasterizer.text = new_text;
            self.cursor_position = start_idx;
            
            // Clear selection
            self.selection_start = None;
            self.selection_end = None;
            
            true
        } else {
            false
        }
    }
    
    fn handle_action_key(&mut self, action: KeyType, _state: &mut EngineState) {
        match action {
            KeyType::Mouse => {
                // Mouse/trackpad toggle is handled by the keyboard itself
                // Just clear any active state here
                self.trackpad_active = false;
                self.trackpad_selecting = false;
            }
            KeyType::Copy => {
                // Copy selected text to clipboard
                if let (Some(start), Some(end)) = (self.selection_start, self.selection_end) {
                    let (start_idx, end_idx) = if start <= end { (start, end) } else { (end, start) };
                    let text_chars: Vec<char> = self.text_rasterizer.text.chars().collect();
                    let selected_text: String = text_chars[start_idx..end_idx.min(text_chars.len())].iter().collect();
                    
                    // Store in internal clipboard as fallback
                    self.clipboard_content = selected_text.clone();
                    
                    // Copy to system clipboard
                    let _ = clipboard::set_contents(&selected_text);
                    
                    eprintln!("Copied: {}", selected_text);
                }
            }
            KeyType::Cut => {
                // Cut selected text (copy + delete)
                if let (Some(start), Some(end)) = (self.selection_start, self.selection_end) {
                    self.save_undo_state();
                    
                    let (start_idx, end_idx) = if start <= end { (start, end) } else { (end, start) };
                    let text_chars: Vec<char> = self.text_rasterizer.text.chars().collect();
                    let selected_text: String = text_chars[start_idx..end_idx.min(text_chars.len())].iter().collect();
                    
                    // Store in internal clipboard as fallback
                    self.clipboard_content = selected_text.clone();
                    
                    // Copy to system clipboard
                    let _ = clipboard::set_contents(&selected_text);
                    
                    eprintln!("Cut: {}", selected_text);
                    
                    // Delete selected text
                    let mut new_text = String::new();
                    for (i, &c) in text_chars.iter().enumerate() {
                        if i < start_idx || i >= end_idx {
                            new_text.push(c);
                        }
                    }
                    self.text_rasterizer.text = new_text;
                    self.cursor_position = start_idx;
                    self.selection_start = None;
                    self.selection_end = None;
                }
            }
            KeyType::Paste => {
                // Paste from clipboard
                self.save_undo_state();
                
                // Get from system clipboard, fall back to internal clipboard
                let clipboard_text = clipboard::get_contents()
                    .unwrap_or_else(|| self.clipboard_content.clone());
                
                if !clipboard_text.is_empty() {
                    // Delete selection if any
                    if let (Some(start), Some(end)) = (self.selection_start, self.selection_end) {
                        let (start_idx, end_idx) = if start <= end { (start, end) } else { (end, start) };
                        let text_chars: Vec<char> = self.text_rasterizer.text.chars().collect();
                        let mut new_text = String::new();
                        for (i, &c) in text_chars.iter().enumerate() {
                            if i < start_idx || i >= end_idx {
                                new_text.push(c);
                            }
                        }
                        self.text_rasterizer.text = new_text;
                        self.cursor_position = start_idx;
                        self.selection_start = None;
                        self.selection_end = None;
                    }
                    
                    // Insert clipboard text
                    let text_chars: Vec<char> = self.text_rasterizer.text.chars().collect();
                    let mut new_text = String::new();
                    for (i, &c) in text_chars.iter().enumerate() {
                        if i == self.cursor_position {
                            new_text.push_str(&clipboard_text);
                        }
                        new_text.push(c);
                    }
                    if self.cursor_position >= text_chars.len() {
                        new_text.push_str(&clipboard_text);
                    }
                    self.text_rasterizer.text = new_text;
                    self.cursor_position += clipboard_text.chars().count();
                }
            }
            KeyType::SelectAll => {
                let text_len = self.text_rasterizer.text.chars().count();
                // Toggle: if everything is selected, deselect; otherwise select all
                if self.selection_start == Some(0) && self.selection_end == Some(text_len) {
                    // Deselect all
                    self.selection_start = None;
                    self.selection_end = None;
                } else {
                    // Select all text
                    self.selection_start = Some(0);
                    self.selection_end = Some(text_len);
                    self.cursor_position = text_len;
                }
            }
            KeyType::Undo => {
                if let Some((text, cursor)) = self.undo_stack.pop() {
                    // Save current state to redo stack
                    self.redo_stack.push((self.text_rasterizer.text.clone(), self.cursor_position));
                    // Restore previous state
                    self.text_rasterizer.text = text;
                    self.cursor_position = cursor;
                    // Clear selection
                    self.selection_start = None;
                    self.selection_end = None;
                }
            }
            KeyType::Redo => {
                if let Some((text, cursor)) = self.redo_stack.pop() {
                    // Save current state to undo stack
                    self.undo_stack.push((self.text_rasterizer.text.clone(), self.cursor_position));
                    // Restore redone state
                    self.text_rasterizer.text = text;
                    self.cursor_position = cursor;
                    // Clear selection
                    self.selection_start = None;
                    self.selection_end = None;
                }
            }
            _ => {}
        }
    }
}
