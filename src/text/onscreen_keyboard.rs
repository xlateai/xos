use crate::apps::partitions::partition::{Partition, PartitionData};
use fontdue::{Font, FontSettings};
use std::time::{Instant, Duration};

const KEYBOARD_BG_COLOR: (u8, u8, u8) = (0, 0, 0); // Pitch black
const KEY_COLOR: (u8, u8, u8) = (40, 40, 40);
const KEY_PRESSED_COLOR: (u8, u8, u8) = (80, 80, 80);
const KEY_TEXT_COLOR: (u8, u8, u8) = (255, 255, 255);
const TOP_LINE_COLOR: (u8, u8, u8) = (0, 255, 0); // Green line

// QWERTY layout
const ROW1: &[char] = &['q', 'w', 'e', 'r', 't', 'y', 'u', 'i', 'o', 'p'];
const ROW2: &[char] = &['a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l'];
const ROW3: &[char] = &['z', 'x', 'c', 'v', 'b', 'n', 'm'];

// Symbols1 layout
const SYMBOL1_ROW1: &[char] = &['1', '2', '3', '4', '5', '6', '7', '8', '9', '0'];
const SYMBOL1_ROW2: &[char] = &['-', '/', ':', ';', '(', ')', '$', '&', '@', '"'];
const SYMBOL1_ROW3: &[char] = &['.', ',', '?', '!', '\''];

// Symbols2 layout
const SYMBOL2_ROW1: &[char] = &['[', ']', '{', '}', '#', '%', '^', '*', '+', '='];
const SYMBOL2_ROW2: &[char] = &['_', '\\', '|', '~', '<', '>', '€', '£', '¥', '•'];
const SYMBOL2_ROW3: &[char] = &['.', ',', '?', '!', '\''];

#[derive(Clone, Copy, PartialEq)]
enum SymbolMode {
    Standard,
    Symbols1,
    Symbols2,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum KeyType {
    Char(char),
    Backspace,
    Space,
    Shift,
    Return,
    Symbol,      // Left of spacebar: "123" (Standard) or "ABC" (Symbols1/Symbols2)
    SymbolToggle, // Row 3: "#+=" (Symbols1) or "123" (Symbols2), toggles Symbols1 ↔ Symbols2
    // Action keys (top row)
    Mouse,       // Toggle trackpad mode
    Copy,
    Cut,
    Paste,
    SelectAll,
    Undo,
    Redo,
}

struct Key {
    key_type: KeyType,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    pressed: bool,
}

pub struct OnScreenKeyboard {
    data: PartitionData,
    keys: Vec<Key>,
    font: Font,
    minimized: bool,
    shift_pressed: bool,
    symbol_mode: SymbolMode, // Standard, Symbols1, or Symbols2
    last_pressed_key: Option<KeyType>,
    // Hold-to-repeat tracking
    held_key_type: Option<KeyType>,
    held_key_start_time: Option<Instant>,
    last_repeat_time: Option<Instant>,
    // Shift tap tracking (for quick tap vs hold)
    shift_press_time: Option<Instant>,
    // Queue for pending characters
    pending_chars: Vec<char>,
    // Trackpad mode
    trackpad_mode: bool,
}

impl std::fmt::Debug for OnScreenKeyboard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OnScreenKeyboard")
            .field("minimized", &self.minimized)
            .field("shift_pressed", &self.shift_pressed)
            .field("num_keys", &self.keys.len())
            .finish()
    }
}

impl OnScreenKeyboard {
    pub fn new() -> Self {
        let font_bytes = include_bytes!("../../assets/JetBrainsMono-Regular.ttf") as &[u8];
        let font = Font::from_bytes(font_bytes, FontSettings::default())
            .expect("Failed to load font");

        // Initialize partition - actual position will be set by text app using safe regions
        // Internal coordinates (0-1) work relative to partition bounds
        // Always start minimized (hidden by default)
        let minimized_default = true;
        let mut keyboard = Self {
            data: PartitionData::new(0.0, 1.0, 0.70, 1.0, KEYBOARD_BG_COLOR),
            keys: Vec::new(),
            font,
            minimized: minimized_default,
            shift_pressed: false,
            symbol_mode: SymbolMode::Standard,
            last_pressed_key: None,
            held_key_type: None,
            held_key_start_time: None,
            last_repeat_time: None,
            shift_press_time: None,
            pending_chars: Vec::new(),
            trackpad_mode: false,
        };

        keyboard.layout_keys();
        keyboard
    }

    /// Main tick function to handle keyboard rendering and logic
    /// Call this from the engine before drawing
    pub fn tick(&mut self, buffer: &mut [u8], width: u32, height: u32, mouse_x: f32, mouse_y: f32, safe_region: &crate::engine::SafeRegionBoundingRectangle) {
        // Position keyboard partition just above bottom safe region
        let keyboard_height = 0.30; // 30% of screen height
        let keyboard_bottom_safe = safe_region.y2; // Bottom of safe region
        let keyboard_top = (keyboard_bottom_safe - keyboard_height).max(safe_region.y1);
        
        self.data_mut().top = keyboard_top;
        self.data_mut().bottom = keyboard_bottom_safe;
        self.data_mut().left = 0.0;
        self.data_mut().right = 1.0;

        // Handle on-screen keyboard repeat
        let now = Instant::now();
        if let Some(ch) = self.check_key_hold_repeat(now) {
            self.pending_chars.push(ch);
        }

        // Update hover state
        self.update_hover(mouse_x, mouse_y, width as f32, height as f32);

        // Draw keyboard
        self.draw(buffer, width, height);
        
        // Draw black area with green border below keyboard (in unsafe region)
        // Only draw if keyboard is visible
        if !self.is_minimized() {
            let height_f = height as f32;
            let keyboard_bottom_px = keyboard_bottom_safe * height_f;
            let screen_bottom = height_f;
            
            if keyboard_bottom_px < screen_bottom {
                let border_y = keyboard_bottom_px.round() as i32;
                let fill_start_y = (border_y + 1).max(0);
                let fill_end_y = screen_bottom as i32;
                
                // Draw green border line
                if border_y >= 0 && border_y < height as i32 {
                    for x in 0..width as i32 {
                        let idx = ((border_y as u32 * width + x as u32) * 4) as usize;
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
                            let idx = ((y as u32 * width + x as u32) * 4) as usize;
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
        }
    }

    /// Pop and return a pending character from the queue, if any
    pub fn pop_pending_char(&mut self) -> Option<char> {
        if self.pending_chars.is_empty() {
            None
        } else {
            Some(self.pending_chars.remove(0))
        }
    }

    /// Show the keyboard
    pub fn show(&mut self) {
        self.minimized = false;
    }

    /// Hide the keyboard
    pub fn hide(&mut self) {
        self.minimized = true;
    }

    /// Check if the keyboard is shown
    pub fn is_shown(&self) -> bool {
        !self.minimized
    }

    /// Get the top edge coordinates of the keyboard (or bottom of screen if hidden)
    /// Returns (x1, y1, x2, y2) where (x1, y1) is the left top corner and (x2, y2) is the right top corner
    /// All values are normalized from 0.0 to 1.0
    pub fn top_edge_coordinates(&self) -> (f32, f32, f32, f32) {
        if self.minimized {
            // When hidden, return the bottom edge of the safe region
            (0.0, self.data.bottom, 1.0, self.data.bottom)
        } else {
            // When visible, return the top edge of the keyboard
            (self.data.left, self.data.top, self.data.right, self.data.top)
        }
    }

    /// Handle mouse up event
    pub fn on_mouse_up(&mut self) {
        self.release_keys();
    }

    /// Handle mouse down event - returns true if the keyboard handled the event
    pub fn on_mouse_down(&mut self, mouse_x: f32, mouse_y: f32, width: f32, height: f32) -> bool {
        if self.minimized {
            return false;
        }

        // Check if click is within keyboard partition bounds
        let keyboard_left = self.data.left * width;
        let keyboard_right = self.data.right * width;
        let keyboard_top = self.data.top * height;
        let keyboard_bottom = self.data.bottom * height;
        
        // Only process if click is within keyboard bounds
        if mouse_x < keyboard_left || mouse_x > keyboard_right || mouse_y < keyboard_top || mouse_y > keyboard_bottom {
            return false;
        }

        // Handle key press
        let now = Instant::now();
        if let Some(ch) = self.check_key_press(mouse_x, mouse_y, width, height, now) {
            self.pending_chars.push(ch);
        }

        true // Event was handled by keyboard
    }

    pub fn is_minimized(&self) -> bool {
        self.minimized
    }

    pub fn get_held_key_type(&self) -> Option<KeyType> {
        self.held_key_type
    }
    
    pub fn is_trackpad_mode(&self) -> bool {
        self.trackpad_mode
    }

    pub fn check_key_type_at_position(&self, mx: f32, my: f32, w: f32, h: f32) -> Option<KeyType> {
        if self.minimized {
            return None;
        }

        // Check if click is within keyboard partition bounds
        let keyboard_left = self.data.left * w;
        let keyboard_right = self.data.right * w;
        let keyboard_top = self.data.top * h;
        let keyboard_bottom = self.data.bottom * h;
        let keyboard_width = keyboard_right - keyboard_left;
        let keyboard_height = keyboard_bottom - keyboard_top;
        
        // Only process if click is within keyboard bounds
        if mx < keyboard_left || mx > keyboard_right || my < keyboard_top || my > keyboard_bottom {
            return None;
        }
        
        // Find the key at this position
        for key in &self.keys {
            let key_x0 = keyboard_left + key.x * keyboard_width;
            let key_x1 = key_x0 + key.width * keyboard_width;
            let key_y0 = keyboard_top + key.y * keyboard_height;
            let key_y1 = key_y0 + key.height * keyboard_height;

            if mx >= key_x0 && mx <= key_x1 && my >= key_y0 && my <= key_y1 {
                return Some(key.key_type);
            }
        }
        
        None
    }

    pub fn toggle_minimize(&mut self) {
        self.minimized = !self.minimized;
    }

    pub fn check_key_hold_repeat(&mut self, now: Instant) -> Option<char> {
        if let Some(key_type) = self.held_key_type {
            if let Some(start_time) = self.held_key_start_time {
                // 0.5s delay before starting repeat
                let repeat_delay = Duration::from_millis(500);
                let repeat_interval = Duration::from_millis(50); // Fast repeat interval
                
                if now.duration_since(start_time) >= repeat_delay {
                    if let Some(last_repeat) = self.last_repeat_time {
                        if now.duration_since(last_repeat) >= repeat_interval {
                            self.last_repeat_time = Some(now);
                            return self.key_type_to_char(key_type);
                        }
                    } else {
                        self.last_repeat_time = Some(now);
                        return self.key_type_to_char(key_type);
                    }
                }
            }
        }
        None
    }
    
    fn key_type_to_char(&self, key_type: KeyType) -> Option<char> {
        match key_type {
            KeyType::Char(ch) => {
                let output_char = if self.shift_pressed && self.symbol_mode == SymbolMode::Standard && ch.is_alphabetic() {
                    ch.to_uppercase().next().unwrap_or(ch)
                } else {
                    ch
                };
                Some(output_char)
            }
            KeyType::Backspace => Some('\u{8}'),
            KeyType::Space => Some(' '),
            KeyType::Return => Some('\n'),
            _ => None, // Shift, Symbol, SymbolToggle, and action keys don't output characters
        }
    }
    
    /// Check if a key was pressed and return its type (for action keys)
    pub fn get_last_action_key(&mut self) -> Option<KeyType> {
        if let Some(key_type) = self.last_pressed_key {
            match key_type {
                KeyType::Copy | KeyType::Cut | KeyType::Paste | 
                KeyType::SelectAll | KeyType::Undo | KeyType::Redo => {
                    self.last_pressed_key = None; // Clear after reading
                    return Some(key_type);
                }
                _ => {}
            }
        }
        None
    }

    pub fn check_key_press(&mut self, mx: f32, my: f32, w: f32, h: f32, now: Instant) -> Option<char> {
        if self.minimized {
            return None;
        }

        // Check if click is within keyboard partition bounds
        let keyboard_left = self.data.left * w;
        let keyboard_right = self.data.right * w;
        let keyboard_top = self.data.top * h;
        let keyboard_bottom = self.data.bottom * h;
        let keyboard_width = keyboard_right - keyboard_left;
        let keyboard_height = keyboard_bottom - keyboard_top;
        
        // Only process if click is within keyboard bounds
        if mx < keyboard_left || mx > keyboard_right || my < keyboard_top || my > keyboard_bottom {
            return None;
        }
        
        // First, try to find a key that directly contains the click
        let mut clicked_key: Option<usize> = None;
        for (i, key) in self.keys.iter().enumerate() {
            let key_x0 = keyboard_left + key.x * keyboard_width;
            let key_x1 = key_x0 + key.width * keyboard_width;
            let key_y0 = keyboard_top + key.y * keyboard_height;
            let key_y1 = key_y0 + key.height * keyboard_height;

            if mx >= key_x0 && mx <= key_x1 && my >= key_y0 && my <= key_y1 {
                clicked_key = Some(i);
                break;
            }
        }
        
        // If no key was directly clicked, find the nearest key
        let key_index = if let Some(idx) = clicked_key {
            idx
        } else {
            // Find nearest key by calculating distance to center of each key
            let mut nearest_idx = 0;
            let mut min_distance_sq = f32::MAX;
            
            for (i, key) in self.keys.iter().enumerate() {
                let key_x0 = keyboard_left + key.x * keyboard_width;
                let key_x1 = key_x0 + key.width * keyboard_width;
                let key_y0 = keyboard_top + key.y * keyboard_height;
                let key_y1 = key_y0 + key.height * keyboard_height;
                
                // Calculate center of key
                let key_center_x = (key_x0 + key_x1) / 2.0;
                let key_center_y = (key_y0 + key_y1) / 2.0;
                
                // Calculate squared distance (avoiding sqrt for performance)
                let dx = mx - key_center_x;
                let dy = my - key_center_y;
                let distance_sq = dx * dx + dy * dy;
                
                if distance_sq < min_distance_sq {
                    min_distance_sq = distance_sq;
                    nearest_idx = i;
                }
            }
            nearest_idx
        };
        
        // Activate the selected key and get its type
        let key_type = {
            let key = &mut self.keys[key_index];
            key.pressed = true;
            key.key_type
        };
        
        // Track held key for repeat (only for keys that should repeat)
        // Shift is tracked separately for cursor movement, not repeat
        let should_repeat = match key_type {
            KeyType::Char(_) | KeyType::Backspace | KeyType::Space | KeyType::Return => true,
            KeyType::Shift => {
                // Track shift for cursor movement, but don't repeat
                self.held_key_type = Some(key_type);
                self.held_key_start_time = Some(now);
                self.last_repeat_time = None;
                false
            },
            _ => false,
        };
        
        if should_repeat {
            self.held_key_type = Some(key_type);
            self.held_key_start_time = Some(now);
            self.last_repeat_time = None;
        }
        
        match key_type {
            KeyType::Char(ch) => {
                // Only apply shift to letters, not symbols
                let output_char = if self.shift_pressed && self.symbol_mode == SymbolMode::Standard && ch.is_alphabetic() {
                    ch.to_uppercase().next().unwrap_or(ch)
                } else {
                    ch
                };
                self.last_pressed_key = Some(key_type);
                Some(output_char)
            }
            KeyType::Backspace => {
                self.last_pressed_key = Some(key_type);
                Some('\u{8}') // Backspace character
            }
            KeyType::Space => {
                self.last_pressed_key = Some(key_type);
                Some(' ')
            }
            KeyType::Shift => {
                // Record when shift was pressed (for quick tap detection)
                self.shift_press_time = Some(now);
                // Don't toggle shift_pressed yet - wait to see if it's a quick tap
                self.last_pressed_key = Some(key_type);
                None // Shift doesn't output a character
            }
            KeyType::Return => {
                self.last_pressed_key = Some(key_type);
                Some('\n')
            }
            KeyType::Symbol => {
                // Left of spacebar: toggle between Standard and Symbols1
                self.symbol_mode = match self.symbol_mode {
                    SymbolMode::Standard => SymbolMode::Symbols1,
                    SymbolMode::Symbols1 => SymbolMode::Standard,
                    SymbolMode::Symbols2 => SymbolMode::Standard,
                };
                // Relayout keys to show symbols/letters
                self.layout_keys();
                self.last_pressed_key = Some(key_type);
                None // Symbol toggle doesn't output a character
            }
            KeyType::SymbolToggle => {
                // Row 3: toggle between Symbols1 and Symbols2
                self.symbol_mode = match self.symbol_mode {
                    SymbolMode::Standard => SymbolMode::Standard, // Shouldn't happen
                    SymbolMode::Symbols1 => SymbolMode::Symbols2,
                    SymbolMode::Symbols2 => SymbolMode::Symbols1,
                };
                // Relayout keys to show symbols
                self.layout_keys();
                self.last_pressed_key = Some(key_type);
                None // SymbolToggle doesn't output a character
            }
            KeyType::Mouse => {
                // Toggle trackpad mode
                self.trackpad_mode = !self.trackpad_mode;
                // Relayout keys to show/hide keyboard
                self.layout_keys();
                self.last_pressed_key = Some(key_type);
                None
            }
            KeyType::Copy | KeyType::Cut | KeyType::Paste | 
            KeyType::SelectAll | KeyType::Undo | KeyType::Redo => {
                // Action keys - store for app to handle
                self.last_pressed_key = Some(key_type);
                None // Action keys don't output characters
            }
        }
    }

    pub fn release_keys(&mut self) {
        let now = Instant::now();
        
        // Check if shift was released quickly (quick tap)
        if let Some(shift_press_time) = self.shift_press_time {
            let hold_duration = now.duration_since(shift_press_time);
            let quick_tap_threshold = Duration::from_millis(300); // 300ms threshold
            
            // If shift was held for less than threshold, it's a quick tap - toggle shift
            if hold_duration < quick_tap_threshold && self.held_key_type == Some(KeyType::Shift) {
                self.shift_pressed = !self.shift_pressed;
            }
        }
        
        for key in &mut self.keys {
            key.pressed = false;
        }
        // Clear hold-to-repeat tracking
        self.held_key_type = None;
        self.held_key_start_time = None;
        self.last_repeat_time = None;
        self.shift_press_time = None;
    }

    pub fn update_hover(&mut self, _mx: f32, _my: f32, _w: f32, _h: f32) {
        // Hover state is now handled per-key in the keys vector
    }

    fn layout_keys(&mut self) {
        self.keys.clear();
        
        // Use a reference size - actual sizing happens in draw_key based on screen dimensions
        // Store relative positions (0.0 to 1.0) that will be scaled to actual screen size
        let top_line_height = 0.01; // Thin green line at top (1% of keyboard height)
        let key_spacing_ratio = 0.0075; // 0.75% spacing between keys (50% of original)
        // No side padding - all rows span exactly 0.0 to 1.0
        
        // We have 5 rows: 1 action row + 3 rows of keys + 1 spacebar row
        let num_key_rows = 4.0; // action row + 3 letter rows
        
        // Calculate relative positions (will be scaled in draw_key)
        // Store as 0.0-1.0 relative to keyboard area
        // Start below green line
        let mut key_y = top_line_height;
        let row_height = (1.0 - top_line_height) / (num_key_rows + 1.0); // +1 for spacebar
        
        // Action row (top): Mouse, Copy, Cut, Paste, Select All, Undo, Redo (7 buttons)
        let action_key_width = (1.0 - key_spacing_ratio * 6.0) / 7.0;
        let action_keys = [KeyType::Mouse, KeyType::Copy, KeyType::Cut, KeyType::Paste, KeyType::SelectAll, KeyType::Undo, KeyType::Redo];
        let mut x = 0.0;
        for &key_type in &action_keys {
            self.keys.push(Key {
                key_type,
                x,
                y: key_y,
                width: action_key_width,
                height: row_height,
                pressed: false,
            });
            x += action_key_width + key_spacing_ratio;
        }
        
        // Move to next row
        key_y += row_height + key_spacing_ratio;
        
        // If in trackpad mode, skip drawing the rest of the keys
        if self.trackpad_mode {
            return;
        }
        
        match self.symbol_mode {
            SymbolMode::Standard => {
                // Standard QWERTY layout
                // Row 1: 10 buttons (qwertyuiop)
                let row1_key_width = (1.0 - key_spacing_ratio * 9.0) / 10.0;
                let mut x = 0.0;
                for &ch in ROW1 {
                    self.keys.push(Key {
                        key_type: KeyType::Char(ch),
                        x,
                        y: key_y,
                        width: row1_key_width,
                        height: row_height,
                        pressed: false,
                    });
                    x += row1_key_width + key_spacing_ratio;
                }
                
                // Row 2: 9 buttons (asdfghjkl)
                key_y += row_height + key_spacing_ratio;
                let row2_key_width = (1.0 - key_spacing_ratio * 8.0) / 9.0;
                x = 0.0;
                for &ch in ROW2 {
                    self.keys.push(Key {
                        key_type: KeyType::Char(ch),
                        x,
                        y: key_y,
                        width: row2_key_width,
                        height: row_height,
                        pressed: false,
                    });
                    x += row2_key_width + key_spacing_ratio;
                }
                
                // Row 3: Shift + 7 letters + Backspace
                key_y += row_height + key_spacing_ratio;
                let shift_key_width = row2_key_width * 1.2;
                let backspace_key_width = row2_key_width * 1.3;
                let row3_char_width = (1.0 - shift_key_width - backspace_key_width - key_spacing_ratio * 8.0) / 7.0;
                
                x = 0.0;
                self.keys.push(Key {
                    key_type: KeyType::Shift,
                    x,
                    y: key_y,
                    width: shift_key_width,
                    height: row_height,
                    pressed: false,
                });
                x += shift_key_width + key_spacing_ratio;
                
                for &ch in ROW3 {
                    self.keys.push(Key {
                        key_type: KeyType::Char(ch),
                        x,
                        y: key_y,
                        width: row3_char_width,
                        height: row_height,
                        pressed: false,
                    });
                    x += row3_char_width + key_spacing_ratio;
                }
                
                self.keys.push(Key {
                    key_type: KeyType::Backspace,
                    x,
                    y: key_y,
                    width: backspace_key_width,
                    height: row_height,
                    pressed: false,
                });
                
                // Row 4: Symbol(123) + Space + Return
                key_y += row_height + key_spacing_ratio;
                let symbol_key_width = row2_key_width * 1.5;
                let return_key_width = row2_key_width * 1.5;
                let spacebar_width = 1.0 - symbol_key_width - return_key_width - key_spacing_ratio * 2.0;
                
                self.keys.push(Key {
                    key_type: KeyType::Symbol,
                    x: 0.0,
                    y: key_y,
                    width: symbol_key_width,
                    height: 1.0 - key_y,
                    pressed: false,
                });
                self.keys.push(Key {
                    key_type: KeyType::Space,
                    x: symbol_key_width + key_spacing_ratio,
                    y: key_y,
                    width: spacebar_width,
                    height: 1.0 - key_y,
                    pressed: false,
                });
                self.keys.push(Key {
                    key_type: KeyType::Return,
                    x: symbol_key_width + key_spacing_ratio + spacebar_width + key_spacing_ratio,
                    y: key_y,
                    width: return_key_width,
                    height: 1.0 - key_y,
                    pressed: false,
                });
            }
            SymbolMode::Symbols1 => {
                // Symbols1 layout
                // Row 1: 10 numbers (1234567890)
                let row1_key_width = (1.0 - key_spacing_ratio * 9.0) / 10.0;
                let mut x = 0.0;
                for &ch in SYMBOL1_ROW1 {
                    self.keys.push(Key {
                        key_type: KeyType::Char(ch),
                        x,
                        y: key_y,
                        width: row1_key_width,
                        height: row_height,
                        pressed: false,
                    });
                    x += row1_key_width + key_spacing_ratio;
                }
                
                // Row 2: 10 symbols (-/:;()$&@")
                key_y += row_height + key_spacing_ratio;
                let row2_key_width = (1.0 - key_spacing_ratio * 9.0) / 10.0;
                x = 0.0;
                for &ch in SYMBOL1_ROW2 {
                    self.keys.push(Key {
                        key_type: KeyType::Char(ch),
                        x,
                        y: key_y,
                        width: row2_key_width,
                        height: row_height,
                        pressed: false,
                    });
                    x += row2_key_width + key_spacing_ratio;
                }
                
                // Row 3: SymbolToggle(#+=) + 5 symbols (.,?!') + Backspace
                key_y += row_height + key_spacing_ratio;
                let symbol_toggle_width = row2_key_width * 1.2; // Same size as shift
                let backspace_key_width = row2_key_width * 1.3;
                let row3_char_width = (1.0 - symbol_toggle_width - backspace_key_width - key_spacing_ratio * 6.0) / 5.0;
                
                x = 0.0;
                self.keys.push(Key {
                    key_type: KeyType::SymbolToggle,
                    x,
                    y: key_y,
                    width: symbol_toggle_width,
                    height: row_height,
                    pressed: false,
                });
                x += symbol_toggle_width + key_spacing_ratio;
                
                for &ch in SYMBOL1_ROW3 {
                    self.keys.push(Key {
                        key_type: KeyType::Char(ch),
                        x,
                        y: key_y,
                        width: row3_char_width,
                        height: row_height,
                        pressed: false,
                    });
                    x += row3_char_width + key_spacing_ratio;
                }
                
                self.keys.push(Key {
                    key_type: KeyType::Backspace,
                    x,
                    y: key_y,
                    width: backspace_key_width,
                    height: row_height,
                    pressed: false,
                });
                
                // Row 4: Symbol(ABC) + Space + Return
                key_y += row_height + key_spacing_ratio;
                let symbol_key_width = row2_key_width * 1.5;
                let return_key_width = row2_key_width * 1.5;
                let spacebar_width = 1.0 - symbol_key_width - return_key_width - key_spacing_ratio * 2.0;
                
                self.keys.push(Key {
                    key_type: KeyType::Symbol,
                    x: 0.0,
                    y: key_y,
                    width: symbol_key_width,
                    height: 1.0 - key_y,
                    pressed: false,
                });
                self.keys.push(Key {
                    key_type: KeyType::Space,
                    x: symbol_key_width + key_spacing_ratio,
                    y: key_y,
                    width: spacebar_width,
                    height: 1.0 - key_y,
                    pressed: false,
                });
                self.keys.push(Key {
                    key_type: KeyType::Return,
                    x: symbol_key_width + key_spacing_ratio + spacebar_width + key_spacing_ratio,
                    y: key_y,
                    width: return_key_width,
                    height: 1.0 - key_y,
                    pressed: false,
                });
            }
            SymbolMode::Symbols2 => {
                // Symbols2 layout
                // Row 1: 10 symbols ([]{}#%^*+=)
                let row1_key_width = (1.0 - key_spacing_ratio * 9.0) / 10.0;
                let mut x = 0.0;
                for &ch in SYMBOL2_ROW1 {
                    self.keys.push(Key {
                        key_type: KeyType::Char(ch),
                        x,
                        y: key_y,
                        width: row1_key_width,
                        height: row_height,
                        pressed: false,
                    });
                    x += row1_key_width + key_spacing_ratio;
                }
                
                // Row 2: 10 symbols (_\|~<>€£¥•)
                key_y += row_height + key_spacing_ratio;
                let row2_key_width = (1.0 - key_spacing_ratio * 9.0) / 10.0;
                x = 0.0;
                for &ch in SYMBOL2_ROW2 {
                    self.keys.push(Key {
                        key_type: KeyType::Char(ch),
                        x,
                        y: key_y,
                        width: row2_key_width,
                        height: row_height,
                        pressed: false,
                    });
                    x += row2_key_width + key_spacing_ratio;
                }
                
                // Row 3: SymbolToggle(123) + 5 symbols (.,?!') + Backspace
                key_y += row_height + key_spacing_ratio;
                let symbol_toggle_width = row2_key_width * 1.2; // Same size as shift
                let backspace_key_width = row2_key_width * 1.3;
                let row3_char_width = (1.0 - symbol_toggle_width - backspace_key_width - key_spacing_ratio * 6.0) / 5.0;
                
                x = 0.0;
                self.keys.push(Key {
                    key_type: KeyType::SymbolToggle,
                    x,
                    y: key_y,
                    width: symbol_toggle_width,
                    height: row_height,
                    pressed: false,
                });
                x += symbol_toggle_width + key_spacing_ratio;
                
                for &ch in SYMBOL2_ROW3 {
                    self.keys.push(Key {
                        key_type: KeyType::Char(ch),
                        x,
                        y: key_y,
                        width: row3_char_width,
                        height: row_height,
                        pressed: false,
                    });
                    x += row3_char_width + key_spacing_ratio;
                }
                
                self.keys.push(Key {
                    key_type: KeyType::Backspace,
                    x,
                    y: key_y,
                    width: backspace_key_width,
                    height: row_height,
                    pressed: false,
                });
                
                // Row 4: Symbol(ABC) + Space + Return
                key_y += row_height + key_spacing_ratio;
                let symbol_key_width = row2_key_width * 1.5;
                let return_key_width = row2_key_width * 1.5;
                let spacebar_width = 1.0 - symbol_key_width - return_key_width - key_spacing_ratio * 2.0;
                
                self.keys.push(Key {
                    key_type: KeyType::Symbol,
                    x: 0.0,
                    y: key_y,
                    width: symbol_key_width,
                    height: 1.0 - key_y,
                    pressed: false,
                });
                self.keys.push(Key {
                    key_type: KeyType::Space,
                    x: symbol_key_width + key_spacing_ratio,
                    y: key_y,
                    width: spacebar_width,
                    height: 1.0 - key_y,
                    pressed: false,
                });
                self.keys.push(Key {
                    key_type: KeyType::Return,
                    x: symbol_key_width + key_spacing_ratio + spacebar_width + key_spacing_ratio,
                    y: key_y,
                    width: return_key_width,
                    height: 1.0 - key_y,
                    pressed: false,
                });
            }
        }
    }

    fn draw_rounded_rect(
        &self,
        buffer: &mut [u8],
        width: u32,
        height: u32,
        x0: i32,
        y0: i32,
        w: u32,
        h: u32,
        color: (u8, u8, u8),
        radius: f32,
    ) {
        let radius_i = radius as i32;
        let w_i = w as i32;
        let h_i = h as i32;
        
        for dy in 0..h_i {
            for dx in 0..w_i {
                let sx = x0 + dx;
                let sy = y0 + dy;
                
                if sx < 0 || sy < 0 || (sx as u32) >= width || (sy as u32) >= height {
                    continue;
                }
                
                // Check if pixel is in rounded corner area (exclude if outside radius)
                let mut in_corner = false;
                
                // Top-left corner
                if dx < radius_i && dy < radius_i {
                    let dist = ((dx as f32 - radius) * (dx as f32 - radius) + 
                               (dy as f32 - radius) * (dy as f32 - radius)).sqrt();
                    in_corner = dist > radius;
                }
                // Top-right corner
                else if dx >= w_i - radius_i && dy < radius_i {
                    let dist = ((dx as f32 - (w_i - radius_i) as f32) * (dx as f32 - (w_i - radius_i) as f32) + 
                               (dy as f32 - radius) * (dy as f32 - radius)).sqrt();
                    in_corner = dist > radius;
                }
                // Bottom-left corner
                else if dx < radius_i && dy >= h_i - radius_i {
                    let dist = ((dx as f32 - radius) * (dx as f32 - radius) + 
                               (dy as f32 - (h_i - radius_i) as f32) * (dy as f32 - (h_i - radius_i) as f32)).sqrt();
                    in_corner = dist > radius;
                }
                // Bottom-right corner
                else if dx >= w_i - radius_i && dy >= h_i - radius_i {
                    let dist = ((dx as f32 - (w_i - radius_i) as f32) * (dx as f32 - (w_i - radius_i) as f32) + 
                               (dy as f32 - (h_i - radius_i) as f32) * (dy as f32 - (h_i - radius_i) as f32)).sqrt();
                    in_corner = dist > radius;
                }
                
                if !in_corner {
                    let idx = ((sy as u32 * width + sx as u32) * 4) as usize;
                    buffer[idx + 0] = color.0;
                    buffer[idx + 1] = color.1;
                    buffer[idx + 2] = color.2;
                    buffer[idx + 3] = 0xff;
                }
            }
        }
    }

    fn draw_key(
        &self,
        buffer: &mut [u8],
        width: u32,
        height: u32,
        key: &Key,
        label: &str,
    ) {
        let w = width as f32;
        let h = height as f32;
        
        // Calculate keyboard area dimensions
        let keyboard_left = self.data.left * w;
        let keyboard_right = self.data.right * w;
        let keyboard_top = self.data.top * h;
        let keyboard_bottom = self.data.bottom * h;
        let keyboard_width = keyboard_right - keyboard_left;
        let keyboard_height = keyboard_bottom - keyboard_top;
        
        // Convert relative positions (0.0-1.0) to absolute pixel positions
        let x0 = (keyboard_left + key.x * keyboard_width).round().clamp(0.0, w) as i32;
        let x1 = (keyboard_left + (key.x + key.width) * keyboard_width).round().clamp(0.0, w) as i32;
        let y0 = (keyboard_top + key.y * keyboard_height).round().clamp(0.0, h) as i32;
        let y1 = (keyboard_top + (key.y + key.height) * keyboard_height).round().clamp(0.0, h) as i32;
        
        let key_w = (x1 - x0).max(0) as u32;
        let key_h = (y1 - y0).max(0) as u32;
        
        if key_w == 0 || key_h == 0 {
            return;
        }
        
        let color = if key.pressed {
            KEY_PRESSED_COLOR
        } else {
            KEY_COLOR
        };
        
        // Draw rounded rectangle key
        self.draw_rounded_rect(buffer, width, height, x0, y0, key_w, key_h, color, 8.0);
        
        // Draw key label (centered) - scale font size with key size
        // Use the same approach as geometric.rs for accurate rendering
        let key_size = (key_w as f32).min(key_h as f32);
        let font_size = (key_size * 0.36).max(18.0).min(43.2); // 10% smaller: 0.4 * 0.9 = 0.36, 20*0.9=18, 48*0.9=43.2
        let line_metrics = self.font.horizontal_line_metrics(font_size)
            .expect("Font missing horizontal metrics");
        
        // Calculate baseline_y for proper vertical centering
        let baseline_y = y0 as f32 + (key_h as f32 / 2.0) + (line_metrics.ascent - line_metrics.descent) / 2.0;
        
        // Handle multi-character labels - calculate total width first
        let label_chars: Vec<char> = label.chars().collect();
        let mut total_width = 0.0;
        let mut character_data = Vec::new();
        
        for &ch in &label_chars {
            let (metrics, bitmap) = self.font.rasterize(ch, font_size);
            total_width += metrics.advance_width;
            character_data.push((ch, metrics, bitmap));
        }
        
        // Start x position (centered)
        let mut current_x = x0 as f32 + ((key_w as f32 - total_width) / 2.0);
        
        // Render each character using the same approach as geometric.rs
        for (_ch, metrics, bitmap) in character_data {
            // Calculate y position using the same formula as geometric.rs
            let char_y = baseline_y - metrics.height as f32 - metrics.ymin as f32;
            let char_x = current_x;
            
            // Render bitmap using same indexing as geometric.rs: bitmap[y * width + x]
            for y in 0..metrics.height {
                for x in 0..metrics.width {
                    let val = bitmap[y * metrics.width + x];
                    if val == 0 {
                        continue;
                    }
                    
                    let sx = (char_x + x as f32) as i32;
                    let sy = (char_y + y as f32) as i32;
                    
                    if sx >= 0 && sy >= 0 && (sx as u32) < width && (sy as u32) < height {
                        let idx = ((sy as u32 * width + sx as u32) * 4) as usize;
                        buffer[idx + 0] = KEY_TEXT_COLOR.0;
                        buffer[idx + 1] = KEY_TEXT_COLOR.1;
                        buffer[idx + 2] = KEY_TEXT_COLOR.2;
                        buffer[idx + 3] = val;
                    }
                }
            }
            
            current_x += metrics.advance_width;
        }
    }

}

impl Partition for OnScreenKeyboard {
    fn data(&self) -> &PartitionData {
        &self.data
    }

    fn data_mut(&mut self) -> &mut PartitionData {
        &mut self.data
    }

    fn draw(&self, buffer: &mut [u8], width: u32, height: u32) {
        if self.minimized {
            // When minimized, don't draw anything (green line is hidden)
            return;
        }

        let w = width as f32;
        let h = height as f32;

        let x0 = (self.data.left * w).round().clamp(0.0, w) as i32;
        let x1 = (self.data.right * w).round().clamp(0.0, w) as i32;
        let y0 = (self.data.top * h).round().clamp(0.0, h) as i32;
        let y1 = (self.data.bottom * h).round().clamp(0.0, h) as i32;

        let rect_w = (x1 - x0).max(0) as u32;
        let rect_h = (y1 - y0).max(0) as u32;

        // Draw keyboard background (pitch black)
        for dy in 0..rect_h {
            for dx in 0..rect_w {
                let sx = x0 + dx as i32;
                let sy = y0 + dy as i32;

                if sx >= 0 && sy >= 0 && (sx as u32) < width && (sy as u32) < height {
                    let idx = ((sy as u32 * width + sx as u32) * 4) as usize;
                    buffer[idx + 0] = KEYBOARD_BG_COLOR.0;
                    buffer[idx + 1] = KEYBOARD_BG_COLOR.1;
                    buffer[idx + 2] = KEYBOARD_BG_COLOR.2;
                    buffer[idx + 3] = 0xff;
                }
            }
        }

        // Draw thin green line at top
        let top_y = y0;
        if top_y >= 0 && top_y < height as i32 {
            for x in x0..x1 {
                if x >= 0 && (x as u32) < width {
                    let idx = ((top_y as u32 * width + x as u32) * 4) as usize;
                    buffer[idx + 0] = TOP_LINE_COLOR.0;
                    buffer[idx + 1] = TOP_LINE_COLOR.1;
                    buffer[idx + 2] = TOP_LINE_COLOR.2;
                    buffer[idx + 3] = 0xff;
                }
            }
        }

        // Draw keys
        for key in &self.keys {
            let label = match key.key_type {
                KeyType::Char(ch) => {
                    // Only show uppercase for letters when shift is pressed, not for symbols
                    if self.shift_pressed && self.symbol_mode == SymbolMode::Standard && ch.is_alphabetic() {
                        ch.to_uppercase().to_string()
                    } else {
                        ch.to_string()
                    }
                }
                KeyType::Backspace => "del".to_string(),
                KeyType::Space => "Space".to_string(),
                KeyType::Shift => "⇧".to_string(),
                KeyType::Return => "enter".to_string(),
                KeyType::Symbol => match self.symbol_mode {
                    SymbolMode::Standard => "123".to_string(),
                    SymbolMode::Symbols1 => "ABC".to_string(),
                    SymbolMode::Symbols2 => "ABC".to_string(),
                },
                KeyType::SymbolToggle => match self.symbol_mode {
                    SymbolMode::Standard => "".to_string(), // Shouldn't appear
                    SymbolMode::Symbols1 => "#+=".to_string(),
                    SymbolMode::Symbols2 => "123".to_string(),
                },
                // Action keys
                KeyType::Mouse => if self.trackpad_mode { "keys" } else { "mouse" }.to_string(),
                KeyType::Copy => "copy".to_string(),
                KeyType::Cut => "cut".to_string(),
                KeyType::Paste => "paste".to_string(),
                KeyType::SelectAll => "all".to_string(),
                KeyType::Undo => "undo".to_string(),
                KeyType::Redo => "redo".to_string(),
            };
            self.draw_key(buffer, width, height, key, &label);
        }
    }
}
