//! Global FPS overlay (top-right, green, JetBrains Mono) for all xos applications.
//! Matches the former coder-app style; composited after each frame tick and keyboard pass.

use super::EngineState;
use crate::text::text_rasterization::TextRasterizer;
use std::time::Instant;

const REF_SHORT_EDGE: f32 = 920.0;

pub struct FpsOverlay {
    rasterizer: TextRasterizer,
    last_instant: Option<Instant>,
    smoothed_fps: f32,
}

impl std::fmt::Debug for FpsOverlay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FpsOverlay").finish_non_exhaustive()
    }
}

impl FpsOverlay {
    pub fn new() -> Self {
        let font_data = include_bytes!("../../../assets/JetBrainsMono-Regular.ttf");
        let font = fontdue::Font::from_bytes(font_data as &[u8], fontdue::FontSettings::default())
            .expect("Failed to load font for FPS overlay");
        let mut rasterizer = TextRasterizer::new(font, 18.0);
        rasterizer.set_text("— FPS".to_string());
        Self {
            rasterizer,
            last_instant: None,
            smoothed_fps: 60.0,
        }
    }

    #[inline]
    fn ui_scale(short_edge: f32) -> f32 {
        (short_edge / REF_SHORT_EDGE).clamp(0.28, 1.0)
    }

    #[inline]
    fn padding_scaled(scale: f32) -> i32 {
        (10.0_f32 * scale).max(4.0).round() as i32
    }
}

/// Update smoothed FPS and composite the label into the frame (after app + keyboard).
pub fn tick_fps_overlay(state: &mut EngineState) {
    let shape = state.frame.array.shape();
    let width = shape[1] as f32;
    let height = shape[0] as f32;
    if width < 1.0 || height < 1.0 {
        return;
    }

    let ui_scale = FpsOverlay::ui_scale(width.min(height));
    let pad = FpsOverlay::padding_scaled(ui_scale) as f32;
    let safe_top = state.frame.safe_region_boundaries.y1 * height;

    {
        let overlay = &mut state.fps_overlay;
        overlay.rasterizer.set_font_size(18.0 * ui_scale);
        let now = Instant::now();
        if let Some(prev) = overlay.last_instant {
            let dt = now.duration_since(prev).as_secs_f32().max(1e-5);
            let instant_fps = 1.0 / dt;
            overlay.smoothed_fps = overlay.smoothed_fps * 0.9 + instant_fps * 0.1;
        }
        overlay.last_instant = Some(now);
        let fps_display = overlay.smoothed_fps.round().max(0.0) as u32;
        overlay.rasterizer.set_text(format!("{fps_display} FPS"));
        overlay.rasterizer.tick(width, height);
    }

    let fps_text_color = (0, 255, 0);
    let overlay = &state.fps_overlay;
    let fps_text_width: f32 = overlay
        .rasterizer
        .characters
        .iter()
        .map(|c| c.metrics.advance_width)
        .sum();
    let fps_origin_x = width - fps_text_width - pad;
    let fps_origin_y = safe_top + pad;

    let buffer = state.frame.buffer_mut();
    for character in &overlay.rasterizer.characters {
        let char_x = fps_origin_x + character.x;
        let char_y = fps_origin_y + character.y;
        let cw = character.width as usize;
        if cw == 0 {
            continue;
        }
        for (bitmap_y, row) in character.bitmap.chunks(cw).enumerate() {
            for (bitmap_x, &alpha) in row.iter().enumerate() {
                if alpha == 0 {
                    continue;
                }
                let px = (char_x + bitmap_x as f32) as i32;
                let py = (char_y + bitmap_y as f32) as i32;
                if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                    let idx = ((py as u32 * width as u32 + px as u32) * 4) as usize;
                    let alpha_f = alpha as f32 / 255.0;
                    buffer[idx + 0] = ((fps_text_color.0 as f32 * alpha_f)
                        + (buffer[idx + 0] as f32 * (1.0 - alpha_f))) as u8;
                    buffer[idx + 1] = ((fps_text_color.1 as f32 * alpha_f)
                        + (buffer[idx + 1] as f32 * (1.0 - alpha_f))) as u8;
                    buffer[idx + 2] = ((fps_text_color.2 as f32 * alpha_f)
                        + (buffer[idx + 2] as f32 * (1.0 - alpha_f))) as u8;
                }
            }
        }
    }
}
