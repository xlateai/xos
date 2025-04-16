use crate::engine::{Application, EngineState};
use crate::apps::text::geometric::GeometricText;
use fontdue::{Font, FontSettings};

const BACKGROUND_COLOR: (u8, u8, u8) = (20, 20, 30);
const TEXT_COLOR: (u8, u8, u8) = (230, 230, 240);
const KEY_COLOR: (u8, u8, u8) = (40, 40, 60);
const BORDER_COLOR: (u8, u8, u8) = (100, 100, 130);

const KEY_HEIGHT: f32 = 60.0;
const KEY_WIDTH: f32 = 60.0;
const KEY_SPACING: f32 = 8.0;
const KEY_FONT_SIZE: f32 = 22.0;

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
        let mut total_widths: Vec<f32> = vec![];
        for row in &layout {
            let mut row_width = 0.0;
            for label in row {
                row_width += unit_width(label) * KEY_WIDTH + KEY_SPACING;
            }
            total_widths.push(row_width - KEY_SPACING);
        }

        let mut y = 40.0;
        for (row_idx, row) in layout.iter().enumerate() {
            let row_width = total_widths[row_idx];
            let mut x = ((1280.0 - row_width) / 2.0).max(0.0); // center

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
        if x + w as i32 <= 0 || y + h as i32 <= 0 || x >= frame_width as i32 || y >= frame_height as i32 {
            return; // fully off-screen
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

        // border
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
        let width = state.frame.width;
        let height = state.frame.height;

        for i in (0..buffer.len()).step_by(4) {
            buffer[i + 0] = BACKGROUND_COLOR.0;
            buffer[i + 1] = BACKGROUND_COLOR.1;
            buffer[i + 2] = BACKGROUND_COLOR.2;
            buffer[i + 3] = 0xff;
        }

        for (key, x, y) in &mut self.keys {
            let px = *x as i32;
            let py = *y as i32;
            let pw = (key.width_units * KEY_WIDTH) as u32;
            let ph = KEY_HEIGHT as u32;

            key.text.tick(width as f32, height as f32);

            Self::draw_key(buffer, width, height, px, py, pw, ph);

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
                            buffer[idx + 0] = ((TEXT_COLOR.0 as u16 * val as u16) / 255) as u8;
                            buffer[idx + 1] = ((TEXT_COLOR.1 as u16 * val as u16) / 255) as u8;
                            buffer[idx + 2] = ((TEXT_COLOR.2 as u16 * val as u16) / 255) as u8;
                            buffer[idx + 3] = val;
                        }
                    }
                }
            }
        }
    }
}
