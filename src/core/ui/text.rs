use crate::rasterizer::fill_rect_buffer;
use crate::rasterizer::text::text_rasterization::TextRasterizer;
use fontdue::{Font, FontSettings};
use std::sync::{Mutex, OnceLock};

static UI_TEXT_FONT: OnceLock<Mutex<Option<Font>>> = OnceLock::new();

fn shared_font() -> Result<Font, String> {
    let lock = UI_TEXT_FONT.get_or_init(|| Mutex::new(None));
    let mut guard = lock
        .lock()
        .map_err(|_| "ui text font mutex poisoned".to_string())?;
    if let Some(font) = guard.as_ref() {
        return Ok(font.clone());
    }

    let font_bytes = include_bytes!("../assets/NotoSans-Medium.ttf");
    let font = Font::from_bytes(font_bytes as &[u8], FontSettings::default())
        .map_err(|e| format!("failed to load ui text font: {e}"))?;
    *guard = Some(font.clone());
    Ok(font)
}

#[derive(Clone, Debug)]
pub struct UiText {
    pub text: String,
    pub x1_norm: f32,
    pub y1_norm: f32,
    pub x2_norm: f32,
    pub y2_norm: f32,
    pub color: (u8, u8, u8, u8),
    pub hitboxes: bool,
    pub baselines: bool,
    pub font_size_px: f32,
}

impl UiText {
    pub fn render(&self, buffer: &mut [u8], frame_width: usize, frame_height: usize) -> Result<(), String> {
        if frame_width == 0 || frame_height == 0 {
            return Ok(());
        }

        let x1 = (self.x1_norm.clamp(0.0, 1.0) * frame_width as f32).round() as i32;
        let y1 = (self.y1_norm.clamp(0.0, 1.0) * frame_height as f32).round() as i32;
        let x2 = (self.x2_norm.clamp(0.0, 1.0) * frame_width as f32).round() as i32;
        let y2 = (self.y2_norm.clamp(0.0, 1.0) * frame_height as f32).round() as i32;

        if x2 <= x1 || y2 <= y1 {
            return Ok(());
        }

        let box_width = (x2 - x1) as f32;
        let box_height = (y2 - y1) as f32;

        let font = shared_font()?;
        let mut rasterizer = TextRasterizer::new(font, self.font_size_px.max(1.0));
        rasterizer.set_text(self.text.clone());
        rasterizer.tick(box_width, box_height);

        if self.baselines {
            let baseline_color = (100, 100, 100, 255);
            for line in &rasterizer.lines {
                let by = y1 + line.baseline_y.round() as i32;
                if by >= y1 && by < y2 {
                    fill_rect_buffer(buffer, frame_width, frame_height, x1, by, x2, by + 1, baseline_color);
                }
            }
        }

        for character in &rasterizer.characters {
            let px = x1 + character.x.round() as i32;
            let py = y1 + character.y.round() as i32;
            if py >= y2 {
                continue;
            }

            for by in 0..character.metrics.height {
                for bx in 0..character.metrics.width {
                    let glyph_alpha = character.bitmap[by * character.metrics.width + bx];
                    if glyph_alpha == 0 {
                        continue;
                    }

                    let sx = px + bx as i32;
                    let sy = py + by as i32;
                    if sx < x1 || sx >= x2 || sy < y1 || sy >= y2 {
                        continue;
                    }

                    let idx = ((sy as usize * frame_width + sx as usize) * 4) as usize;
                    let alpha = (glyph_alpha as f32 / 255.0) * (self.color.3 as f32 / 255.0);
                    let inv_alpha = 1.0 - alpha;

                    buffer[idx] = (self.color.0 as f32 * alpha + buffer[idx] as f32 * inv_alpha) as u8;
                    buffer[idx + 1] = (self.color.1 as f32 * alpha + buffer[idx + 1] as f32 * inv_alpha) as u8;
                    buffer[idx + 2] = (self.color.2 as f32 * alpha + buffer[idx + 2] as f32 * inv_alpha) as u8;
                    buffer[idx + 3] = 0xff;
                }
            }

            if self.hitboxes {
                let gx1 = px;
                let gy1 = py;
                let gx2 = px + character.metrics.width as i32;
                let gy2 = py + character.metrics.height as i32;
                let hitbox_color = (255, 0, 0, 255);
                fill_rect_buffer(buffer, frame_width, frame_height, gx1, gy1, gx2, gy1 + 1, hitbox_color);
                fill_rect_buffer(buffer, frame_width, frame_height, gx1, gy2 - 1, gx2, gy2, hitbox_color);
                fill_rect_buffer(buffer, frame_width, frame_height, gx1, gy1, gx1 + 1, gy2, hitbox_color);
                fill_rect_buffer(buffer, frame_width, frame_height, gx2 - 1, gy1, gx2, gy2, hitbox_color);
            }
        }

        Ok(())
    }
}
