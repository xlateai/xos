use crate::apps::partitions::partition::{Partition, PartitionData};
use fontdue::{Font, FontSettings};

const KEYBOARD_BG_COLOR: (u8, u8, u8) = (0, 0, 0); // Pitch black
const KEY_COLOR: (u8, u8, u8) = (40, 40, 40);
const KEY_PRESSED_COLOR: (u8, u8, u8) = (80, 80, 80);
const KEY_TEXT_COLOR: (u8, u8, u8) = (255, 255, 255);
const DISMISS_BUTTON_COLOR: (u8, u8, u8) = (60, 60, 60);
const DISMISS_BUTTON_HOVER_COLOR: (u8, u8, u8) = (100, 100, 100);
const TOP_LINE_COLOR: (u8, u8, u8) = (0, 255, 0); // Green line

// QWERTY layout
const ROW1: &[char] = &['q', 'w', 'e', 'r', 't', 'y', 'u', 'i', 'o', 'p'];
const ROW2: &[char] = &['a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l'];
const ROW3: &[char] = &['z', 'x', 'c', 'v', 'b', 'n', 'm'];

#[derive(Clone, Copy, PartialEq)]
enum KeyType {
    Char(char),
    Backspace,
    Space,
    Shift,
    Return,
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
    last_pressed_key: Option<KeyType>,
    dismiss_button_hover: bool,
}

impl OnScreenKeyboard {
    pub fn new() -> Self {
        let font_bytes = include_bytes!("../../../assets/JetBrainsMono-Regular.ttf") as &[u8];
        let font = Font::from_bytes(font_bytes, FontSettings::default())
            .expect("Failed to load font");

        // Initialize partition - actual position will be set by text app using safe regions
        // Internal coordinates (0-1) work relative to partition bounds
        let mut keyboard = Self {
            data: PartitionData::new(0.0, 1.0, 0.70, 1.0, KEYBOARD_BG_COLOR),
            keys: Vec::new(),
            font,
            minimized: false,
            shift_pressed: false,
            last_pressed_key: None,
            dismiss_button_hover: false,
        };

        keyboard.layout_keys();
        keyboard
    }

    pub fn is_minimized(&self) -> bool {
        self.minimized
    }

    pub fn toggle_minimize(&mut self) {
        self.minimized = !self.minimized;
    }

    pub fn check_key_press(&mut self, mx: f32, my: f32, w: f32, h: f32) -> Option<char> {
        if self.minimized {
            // Check dismiss button (far right top)
            let kx0 = self.data.right * w - 60.0;
            let kx1 = self.data.right * w;
            let ky0 = self.data.top * h;
            let ky1 = self.data.top * h + 40.0;
            
            if mx >= kx0 && mx <= kx1 && my >= ky0 && my <= ky1 {
                self.toggle_minimize();
                return None;
            }
            return None;
        }

        // Check dismiss button first (far right top)
        let kx0 = self.data.right * w - 60.0;
        let kx1 = self.data.right * w;
        let ky0 = self.data.top * h;
        let ky1 = self.data.top * h + 40.0;
        
        if mx >= kx0 && mx <= kx1 && my >= ky0 && my <= ky1 {
            self.toggle_minimize();
            return None;
        }

        // Check keys
        let keyboard_left = self.data.left * w;
        let keyboard_right = self.data.right * w;
        let keyboard_top = self.data.top * h;
        let keyboard_bottom = self.data.bottom * h;
        let keyboard_width = keyboard_right - keyboard_left;
        let keyboard_height = keyboard_bottom - keyboard_top;
        
        for key in &mut self.keys {
            let key_x0 = keyboard_left + key.x * keyboard_width;
            let key_x1 = key_x0 + key.width * keyboard_width;
            let key_y0 = keyboard_top + key.y * keyboard_height;
            let key_y1 = key_y0 + key.height * keyboard_height;

            if mx >= key_x0 && mx <= key_x1 && my >= key_y0 && my <= key_y1 {
                key.pressed = true;
                
                match key.key_type {
                    KeyType::Char(ch) => {
                        let output_char = if self.shift_pressed {
                            ch.to_uppercase().next().unwrap_or(ch)
                        } else {
                            ch
                        };
                        self.last_pressed_key = Some(key.key_type);
                        return Some(output_char);
                    }
                    KeyType::Backspace => {
                        self.last_pressed_key = Some(key.key_type);
                        return Some('\u{8}'); // Backspace character
                    }
                    KeyType::Space => {
                        self.last_pressed_key = Some(key.key_type);
                        return Some(' ');
                    }
                    KeyType::Shift => {
                        self.shift_pressed = !self.shift_pressed;
                        self.last_pressed_key = Some(key.key_type);
                        return None; // Shift doesn't output a character
                    }
                    KeyType::Return => {
                        self.last_pressed_key = Some(key.key_type);
                        return Some('\n');
                    }
                }
            }
        }

        None
    }

    pub fn release_keys(&mut self) {
        for key in &mut self.keys {
            key.pressed = false;
        }
    }

    pub fn update_hover(&mut self, mx: f32, my: f32, w: f32, h: f32) {
        // Check dismiss button hover
        let kx0 = self.data.right * w - 60.0;
        let kx1 = self.data.right * w;
        let ky0 = self.data.top * h;
        let ky1 = self.data.top * h + 40.0;
        
        self.dismiss_button_hover = mx >= kx0 && mx <= kx1 && my >= ky0 && my <= ky1;
    }

    fn layout_keys(&mut self) {
        self.keys.clear();
        
        // Use a reference size - actual sizing happens in draw_key based on screen dimensions
        // Store relative positions (0.0 to 1.0) that will be scaled to actual screen size
        let top_line_height = 0.01; // Thin green line at top (1% of keyboard height)
        let key_spacing_ratio = 0.015; // 1.5% spacing between keys
        let side_padding_ratio = 0.02; // 2% padding on sides
        
        // We have 4 rows: 3 rows of keys + 1 spacebar row
        let num_key_rows = 3.0;
        
        // Calculate relative positions (will be scaled in draw_key)
        // Store as 0.0-1.0 relative to keyboard area
        let mut key_y = top_line_height;
        let row_height = (1.0 - top_line_height) / (num_key_rows + 1.0); // +1 for spacebar
        
        // Uniform key width for all letter keys
        // Row 1: 10 buttons
        // Row 2: 9 buttons (centered)
        // Row 3: Shift + 7 letters + Backspace
        // Row 4: Spacebar + Return
        
        // Calculate uniform key width based on row with most keys (row 1 has 10)
        let uniform_key_width = (1.0 - side_padding_ratio * 2.0 - key_spacing_ratio * 9.0) / 10.0;
        let special_key_width = uniform_key_width * 1.5; // Shift, Return are 1.5x
        let delete_key_width = uniform_key_width * 1.8; // Delete is slightly wider for "delete" text
        
        // Row 1: 10 buttons (qwertyuiop)
        let row1_total_width = uniform_key_width * 10.0 + key_spacing_ratio * 9.0;
        let row1_start_x = (1.0 - row1_total_width) / 2.0; // Center the row
        let mut x = row1_start_x;
        
        for &ch in ROW1 {
            self.keys.push(Key {
                key_type: KeyType::Char(ch),
                x,
                y: key_y,
                width: uniform_key_width,
                height: row_height,
                pressed: false,
            });
            x += uniform_key_width + key_spacing_ratio;
        }
        
        // Row 2: 9 buttons (asdfghjkl) - centered
        key_y += row_height + key_spacing_ratio;
        let row2_total_width = uniform_key_width * 9.0 + key_spacing_ratio * 8.0;
        let row2_start_x = (1.0 - row2_total_width) / 2.0;
        x = row2_start_x;
        
        for &ch in ROW2 {
            self.keys.push(Key {
                key_type: KeyType::Char(ch),
                x,
                y: key_y,
                width: uniform_key_width,
                height: row_height,
                pressed: false,
            });
            x += uniform_key_width + key_spacing_ratio;
        }
        
        // Row 3: Shift + 7 letters + Delete
        key_y += row_height + key_spacing_ratio;
        let row3_letters_width = uniform_key_width * 7.0 + key_spacing_ratio * 6.0;
        let row3_total_width = special_key_width + key_spacing_ratio + row3_letters_width + key_spacing_ratio + delete_key_width;
        let row3_start_x = (1.0 - row3_total_width) / 2.0;
        
        // Shift on left
        self.keys.push(Key {
            key_type: KeyType::Shift,
            x: row3_start_x,
            y: key_y,
            width: special_key_width,
            height: row_height,
            pressed: false,
        });
        
        // 7 letters (zxcvbnm)
        x = row3_start_x + special_key_width + key_spacing_ratio;
        for &ch in ROW3 {
            self.keys.push(Key {
                key_type: KeyType::Char(ch),
                x,
                y: key_y,
                width: uniform_key_width,
                height: row_height,
                pressed: false,
            });
            x += uniform_key_width + key_spacing_ratio;
        }
        
        // Delete on right (wider for "delete" text)
        self.keys.push(Key {
            key_type: KeyType::Backspace,
            x: row3_start_x + special_key_width + key_spacing_ratio + row3_letters_width + key_spacing_ratio,
            y: key_y,
            width: delete_key_width,
            height: row_height,
            pressed: false,
        });
        
        // Row 4: Spacebar + Return (spacebar takes most width, return on right)
        key_y += row_height + key_spacing_ratio;
        let return_width = special_key_width;
        let spacebar_width = 1.0 - side_padding_ratio * 2.0 - return_width - key_spacing_ratio;
        
        // Spacebar
        self.keys.push(Key {
            key_type: KeyType::Space,
            x: side_padding_ratio,
            y: key_y,
            width: spacebar_width,
            height: 1.0 - key_y, // Extends to bottom
            pressed: false,
        });
        
        // Return on right
        self.keys.push(Key {
            key_type: KeyType::Return,
            x: side_padding_ratio + spacebar_width + key_spacing_ratio,
            y: key_y,
            width: return_width,
            height: 1.0 - key_y, // Extends to bottom
            pressed: false,
        });
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
        let key_size = (key_w as f32).min(key_h as f32);
        let font_size = (key_size * 0.4).max(20.0).min(48.0); // Scale between 20-48px
        let line_metrics = self.font.horizontal_line_metrics(font_size)
            .expect("Font missing horizontal metrics");
        
        // Handle multi-character labels
        let label_chars: Vec<char> = label.chars().collect();
        let mut total_width = 0.0;
        let mut bitmaps = Vec::new();
        
        for &ch in &label_chars {
            let (metrics, bitmap) = self.font.rasterize(ch, font_size);
            total_width += metrics.advance_width;
            bitmaps.push((metrics, bitmap));
        }
        
        let char_height = line_metrics.ascent + line_metrics.descent;
        let mut current_x = x0 as f32 + ((key_w as f32 - total_width) / 2.0);
        let label_y = y0 as f32 + ((key_h as f32 - char_height) / 2.0 + line_metrics.ascent);
        
        for (metrics, bitmap) in bitmaps {
            let char_width = metrics.width;
            
            for (i, &val) in bitmap.iter().enumerate() {
                if val == 0 {
                    continue;
                }
                
                let px = i % char_width;
                let py = i / char_width;
                
                let sx = (current_x + px as f32) as i32;
                let sy = label_y as i32 + py as i32;
                
                if sx >= 0 && sy >= 0 && (sx as u32) < width && (sy as u32) < height {
                    let idx = ((sy as u32 * width + sx as u32) * 4) as usize;
                    buffer[idx + 0] = KEY_TEXT_COLOR.0;
                    buffer[idx + 1] = KEY_TEXT_COLOR.1;
                    buffer[idx + 2] = KEY_TEXT_COLOR.2;
                    buffer[idx + 3] = val;
                }
            }
            
            current_x += metrics.advance_width;
        }
    }

    fn draw_dismiss_button(&self, buffer: &mut [u8], width: u32, height: u32) {
        let w = width as f32;
        let h = height as f32;
        
        let btn_size = 40.0;
        let x0 = ((self.data.right * w - btn_size).round().clamp(0.0, w)) as i32;
        let x1 = ((self.data.right * w).round().clamp(0.0, w)) as i32;
        let y0 = ((self.data.top * h).round().clamp(0.0, h)) as i32;
        let y1 = ((self.data.top * h + btn_size).round().clamp(0.0, h)) as i32;
        
        let btn_w = (x1 - x0).max(0) as u32;
        let btn_h = (y1 - y0).max(0) as u32;
        
        let color = if self.dismiss_button_hover {
            DISMISS_BUTTON_HOVER_COLOR
        } else {
            DISMISS_BUTTON_COLOR
        };
        
        // Draw rounded button
        self.draw_rounded_rect(buffer, width, height, x0, y0, btn_w, btn_h, color, 6.0);
        
        // Draw dismiss label "×"
        let font_size = 24.0;
        let line_metrics = self.font.horizontal_line_metrics(font_size)
            .expect("Font missing horizontal metrics");
        
        let label = "×";
        let ch = label.chars().next().unwrap_or(' ');
        let (metrics, bitmap) = self.font.rasterize(ch, font_size);
        let char_width = metrics.width;
        let char_height = line_metrics.ascent + line_metrics.descent;
        
        let label_x = x0 + ((btn_w as f32 - char_width as f32) / 2.0) as i32;
        let label_y = y0 + ((btn_h as f32 - char_height) / 2.0 + line_metrics.ascent) as i32;
        
        for (i, &val) in bitmap.iter().enumerate() {
            if val == 0 {
                continue;
            }
            
            let px = i % char_width;
            let py = i / char_width;
            
            let sx = label_x + px as i32;
            let sy = label_y + py as i32;
            
            if sx >= 0 && sy >= 0 && (sx as u32) < width && (sy as u32) < height {
                let idx = ((sy as u32 * width + sx as u32) * 4) as usize;
                buffer[idx + 0] = KEY_TEXT_COLOR.0;
                buffer[idx + 1] = KEY_TEXT_COLOR.1;
                buffer[idx + 2] = KEY_TEXT_COLOR.2;
                buffer[idx + 3] = val;
            }
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
            // Only draw thin green line when minimized
            let h = height as f32;
            let y = (self.data.top * h).round() as i32;
            
            if y >= 0 && y < height as i32 {
                for x in 0..width as i32 {
                    let idx = ((y as u32 * width + x as u32) * 4) as usize;
                    buffer[idx + 0] = TOP_LINE_COLOR.0;
                    buffer[idx + 1] = TOP_LINE_COLOR.1;
                    buffer[idx + 2] = TOP_LINE_COLOR.2;
                    buffer[idx + 3] = 0xff;
                }
            }
            
            // Draw dismiss button
            self.draw_dismiss_button(buffer, width, height);
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

        // Draw dismiss button (far right top)
        self.draw_dismiss_button(buffer, width, height);

        // Draw keys
        for key in &self.keys {
            let label = match key.key_type {
                KeyType::Char(ch) => {
                    if self.shift_pressed {
                        ch.to_uppercase().to_string()
                    } else {
                        ch.to_string()
                    }
                }
                KeyType::Backspace => "delete".to_string(),
                KeyType::Space => "Space".to_string(),
                KeyType::Shift => "⇧".to_string(),
                KeyType::Return => "↵".to_string(),
            };
            self.draw_key(buffer, width, height, key, &label);
        }
    }
}
