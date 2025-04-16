use crate::engine::{Application, EngineState};
use crate::apps::text::geometric::GeometricText;
use fontdue::{Font, FontSettings};

const BACKGROUND_COLOR: (u8, u8, u8) = (0, 0, 0);
const TEXT_COLOR: (u8, u8, u8) = (255, 255, 255);
const KEY_COLOR: (u8, u8, u8) = (20, 20, 20);
const BORDER_COLOR: (u8, u8, u8) = (255, 255, 255);

const BASE_KEY_WIDTH: f32 = 60.0;
const BASE_KEY_HEIGHT: f32 = 60.0;
const BASE_KEY_SPACING: f32 = 8.0;
const BASE_FONT_SIZE: f32 = 22.0;

struct Key {
    label: &'static str,
    width_units: f32,
    text: GeometricText,
}

pub struct KeyboardApp {
    layout: Vec<Vec<Key>>,
    font: Font,
}

impl KeyboardApp {
    pub fn new() -> Self {
        let font_bytes = include_bytes!("../../assets/JetBrainsMono-Regular.ttf") as &[u8];
        let font = Font::from_bytes(font_bytes, FontSettings::default()).unwrap();

        let layout_raw = [
            vec!["`", "1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "-", "=", "Backspace"],
            vec!["Tab", "Q", "W", "E", "R", "T", "Y", "U", "I", "O", "P", "[", "]", "\\"],
            vec!["Caps", "A", "S", "D", "F", "G", "H", "J", "K", "L", ";", "'", "Enter"],
            vec!["Shift", "Z", "X", "C", "V", "B", "N", "M", ",", ".", "/", "Shift"],
            vec!["Ctrl", "Alt", "Space", "Alt", "Ctrl"],
        ];

        let unit_width = |label: &str| match label {
            "Backspace" => 2.0,
            "Tab" => 1.5,
            "Caps" => 1.75,
            "Enter" => 2.25,
            "Shift" => 2.25,
            "Ctrl" | "Alt" => 1.25,
            "Space" => 8.4, // ⬅️ Buffed from 6.0 to ~40% larger
            _ => 1.0,
        };

        let layout = layout_raw
            .iter()
            .map(|row| {
                row.iter()
                    .map(|&label| {
                        let width = unit_width(label);
                        let mut text = GeometricText::new(font.clone(), BASE_FONT_SIZE);
                        text.set_text(label.to_string());
                        Key { label, width_units: width, text }
                    })
                    .collect()
            })
            .collect();

        Self { layout, font }
    }

    fn draw_key(
        buffer: &mut [u8],
        frame_width: u32,
        frame_height: u32,
        x: i32,
        y: i32,
        w: u32,
        h: u32,
    ) {
        if x + w as i32 <= 0 || y + h as i32 <= 0 || x >= frame_width as i32 || y >= frame_height as i32 {
            return;
        }

        for dy in 0..h {
            for dx in 0..w {
                let px = x + dx as i32;
                let py = y + dy as i32;
                if px >= 0 && px < frame_width as i32 && py >= 0 && py < frame_height as i32 {
                    let idx = ((py as u32 * frame_width + px as u32) * 4) as usize;
                    buffer[idx + 0] = KEY_COLOR.0;
                    buffer[idx + 1] = KEY_COLOR.1;
                    buffer[idx + 2] = KEY_COLOR.2;
                    buffer[idx + 3] = 0xff;
                }
            }
        }

        for dx in 0..w {
            for &dy in &[0, h - 1] {
                let px = x + dx as i32;
                let py = y + dy as i32;
                if px >= 0 && px < frame_width as i32 && py >= 0 && py < frame_height as i32 {
                    let idx = ((py as u32 * frame_width + px as u32) * 4) as usize;
                    buffer[idx + 0] = BORDER_COLOR.0;
                    buffer[idx + 1] = BORDER_COLOR.1;
                    buffer[idx + 2] = BORDER_COLOR.2;
                    buffer[idx + 3] = 0xff;
                }
            }
        }

        for dy in 0..h {
            for &dx in &[0, w - 1] {
                let px = x + dx as i32;
                let py = y + dy as i32;
                if px >= 0 && px < frame_width as i32 && py >= 0 && py < frame_height as i32 {
                    let idx = ((py as u32 * frame_width + px as u32) * 4) as usize;
                    buffer[idx + 0] = BORDER_COLOR.0;
                    buffer[idx + 1] = BORDER_COLOR.1;
                    buffer[idx + 2] = BORDER_COLOR.2;
                    buffer[idx + 3] = 0xff;
                }
            }
        }
    }
}

impl Application for KeyboardApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        let buffer = &mut state.frame.buffer;
        let width = state.frame.width as f32;
        let height = state.frame.height as f32;
    
        // Use horizontal width only to keep a nice wide layout
        let scale = width / 960.0;
        let key_spacing = BASE_KEY_SPACING * scale;
        let key_height = BASE_KEY_HEIGHT * scale;
        let font_size = BASE_FONT_SIZE * scale;
    
        // Clear screen
        for i in (0..buffer.len()).step_by(4) {
            buffer[i + 0] = BACKGROUND_COLOR.0;
            buffer[i + 1] = BACKGROUND_COLOR.1;
            buffer[i + 2] = BACKGROUND_COLOR.2;
            buffer[i + 3] = 0xff;
        }
    
        // Compute total keyboard height with this horizontal-derived scaling
        let total_height = self.layout.len() as f32 * (key_height + key_spacing) - key_spacing;
        let mut y = ((height - total_height) / 2.0).round();
    
        for row in &mut self.layout {
            let total_units: f32 = row.iter().map(|k| k.width_units).sum();
            let total_spacing = (row.len() - 1) as f32 * key_spacing;
            let unit_width = (width - total_spacing) / total_units;
    
            let mut x = 0.0;
            let row_len = row.len();
    
            for (i, key) in row.iter_mut().enumerate() {
                let w = (key.width_units * unit_width).round() as u32;
                let h = key_height.round() as u32;
    
                // Adjust outer keys to eliminate edge gaps
                let is_leftmost = i == 0;
                let is_rightmost = i == row_len - 1;
                let expand_left = if is_leftmost { (x as f32).floor() } else { 0.0 };
                let expand_right = if is_rightmost { width - (x + w as f32) } else { 0.0 };
    
                let px = (x - expand_left).round() as i32;
                let py = y.round() as i32;
                let pw = (w as f32 + expand_left + expand_right).round() as u32;
    
                key.text.set_font_size(font_size);
                key.text.tick(width, height);
    
                Self::draw_key(buffer, state.frame.width, state.frame.height, px, py, pw, h);
    
                for ch in &key.text.characters {
                    let cx = px + (pw as i32 - ch.width as i32) / 2;
                    let cy = py + (h as i32 - ch.height as i32) / 2;
    
                    for y in 0..ch.metrics.height {
                        for x in 0..ch.metrics.width {
                            let val = ch.bitmap[y * ch.metrics.width + x];
                            let sx = cx + x as i32;
                            let sy = cy + y as i32;
    
                            if sx >= 0 && sx < state.frame.width as i32 && sy >= 0 && sy < state.frame.height as i32 {
                                let idx = ((sy as u32 * state.frame.width + sx as u32) * 4) as usize;
                                buffer[idx + 0] = ((TEXT_COLOR.0 as u16 * val as u16) / 255) as u8;
                                buffer[idx + 1] = ((TEXT_COLOR.1 as u16 * val as u16) / 255) as u8;
                                buffer[idx + 2] = ((TEXT_COLOR.2 as u16 * val as u16) / 255) as u8;
                                buffer[idx + 3] = val;
                            }
                        }
                    }
                }
    
                x += key.width_units * unit_width + key_spacing;
            }
    
            y += key_height + key_spacing;
        }
    }
    
    
}
