use crate::engine::{Application, EngineState};
use crate::apps::text::geometric::GeometricText;
use fontdue::{Font, FontSettings};

const BACKGROUND_COLOR: (u8, u8, u8) = (0, 0, 0);
const TEXT_COLOR: (u8, u8, u8) = (255, 255, 255);
const KEY_COLOR: (u8, u8, u8) = (50, 50, 50);
const BORDER_COLOR: (u8, u8, u8) = (100, 100, 100);

const KEY_HEIGHT: f32 = 60.0;
const KEY_WIDTH: f32 = 60.0;
const KEY_SPACING: f32 = 8.0;
const KEY_FONT_SIZE: f32 = 24.0;

struct Key {
    label: &'static str,
    width_units: f32,
    text: GeometricText,
}

pub struct KeyboardApp {
    keys: Vec<(Key, f32, f32)>, // (key, x, y)
}

impl KeyboardApp {
    pub fn new() -> Self {
        let font_bytes = include_bytes!("../../assets/JetBrainsMono-Regular.ttf") as &[u8];
        let font = Font::from_bytes(font_bytes, FontSettings::default()).unwrap();

        let layout = [
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
            "Space" => 6.0,
            _ => 1.0,
        };

        let mut keys = vec![];
        let mut y = 40.0;

        for row in layout {
            let mut x = 40.0;
            for label in row {
                let mut text = GeometricText::new(font.clone(), KEY_FONT_SIZE);
                text.set_text(label.to_string());
                let width = unit_width(label);
                keys.push((Key { label, width_units: width, text }, x, y));
                x += width * KEY_WIDTH + KEY_SPACING;
            }
            y += KEY_HEIGHT + KEY_SPACING;
        }

        Self { keys }
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

        // draw border
        for dx in 0..w {
            let top = ((y * frame_width as i32 + (x + dx as i32)) * 4) as usize;
            let bottom = (((y + h as i32 - 1) * frame_width as i32 + (x + dx as i32)) * 4) as usize;
            if top < buffer.len() && bottom < buffer.len() {
                buffer[top + 0] = BORDER_COLOR.0;
                buffer[top + 1] = BORDER_COLOR.1;
                buffer[top + 2] = BORDER_COLOR.2;
                buffer[top + 3] = 0xff;
                buffer[bottom + 0] = BORDER_COLOR.0;
                buffer[bottom + 1] = BORDER_COLOR.1;
                buffer[bottom + 2] = BORDER_COLOR.2;
                buffer[bottom + 3] = 0xff;
            }
        }
        for dy in 0..h {
            let left = (((y + dy as i32) * frame_width as i32 + x) * 4) as usize;
            let right = (((y + dy as i32) * frame_width as i32 + (x + w as i32 - 1)) * 4) as usize;
            if left < buffer.len() && right < buffer.len() {
                buffer[left + 0] = BORDER_COLOR.0;
                buffer[left + 1] = BORDER_COLOR.1;
                buffer[left + 2] = BORDER_COLOR.2;
                buffer[left + 3] = 0xff;
                buffer[right + 0] = BORDER_COLOR.0;
                buffer[right + 1] = BORDER_COLOR.1;
                buffer[right + 2] = BORDER_COLOR.2;
                buffer[right + 3] = 0xff;
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
        let width = state.frame.width;
        let height = state.frame.height;

        // Clear
        for i in (0..buffer.len()).step_by(4) {
            buffer[i + 0] = BACKGROUND_COLOR.0;
            buffer[i + 1] = BACKGROUND_COLOR.1;
            buffer[i + 2] = BACKGROUND_COLOR.2;
            buffer[i + 3] = 0xff;
        }

        for (key, x, y) in &mut self.keys {
            key.text.tick(width as f32, height as f32);
            let px = *x as i32;
            let py = *y as i32;
            let pw = (key.width_units * KEY_WIDTH) as u32;
            let ph = KEY_HEIGHT as u32;

            Self::draw_key(buffer, width, height, px, py, pw, ph);

            // render center-aligned
            for ch in &key.text.characters {
                let cx = px + (pw as i32 - ch.width as i32) / 2;
                let cy = py + (ph as i32 - ch.height as i32) / 2;
                for y in 0..ch.metrics.height {
                    for x in 0..ch.metrics.width {
                        let val = ch.bitmap[y * ch.metrics.width + x];
                        let sx = cx + x as i32;
                        let sy = cy + y as i32;
                        if sx >= 0 && sx < width as i32 && sy >= 0 && sy < height as i32 {
                            let idx = ((sy as u32 * width + sx as u32) * 4) as usize;
                            buffer[idx + 0] = TEXT_COLOR.0;
                            buffer[idx + 1] = TEXT_COLOR.1;
                            buffer[idx + 2] = TEXT_COLOR.2;
                            buffer[idx + 3] = val;
                        }
                    }
                }
            }
        }
    }
}
