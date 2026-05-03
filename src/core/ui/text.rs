use crate::rasterizer::fill_rect_buffer;
use crate::rasterizer::text::fonts::{self, FontFamily};
use crate::rasterizer::text::text_rasterization::{
    quantize_viewport_raster_px, sync_rasterizer_to_default_font, TextRasterizer,
};
use fontdue::Font;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Optional interaction hints for viewport text blocks (`UiText`, `RichText` in Python).
/// The rasterizer consumes only geometry and strings; hosts use these flags for selection,
/// `on_key_char` routing, and mobile software keyboard wiring.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UiTextInteraction {
    pub selectable: bool,
    pub editable: bool,
    pub use_software_keyboard: bool,
}

static UI_TEXT_FONT: OnceLock<Mutex<Option<Font>>> = OnceLock::new();
static UI_TEXT_FONT_FAMILY: OnceLock<Mutex<FontFamily>> = OnceLock::new();

pub(crate) fn shared_ui_text_font() -> Result<Font, String> {
    shared_font_inner()
}

/// Clip a `gw × gh` glyph with top-left `(px, py)` to framebuffer rect `[cx1, cx2) × [cy1, cy2)`.
///
/// Returned ranges index the glyph bitmap `[bx_lo, bx_hi)` × `[by_lo, by_hi)` (same convention as
/// standalone `text` app viewport culling: cost scales with blended pixels, not full bitmap bounds).
///
/// Fully outside rects return `None` so callers skip the blend loop entirely.
#[inline]
pub(crate) fn viewport_glyph_blit_ranges(
    px: i32,
    py: i32,
    gw: usize,
    gh: usize,
    cx1: i32,
    cy1: i32,
    cx2: i32,
    cy2: i32,
) -> Option<(usize, usize, usize, usize)> {
    if gw == 0 || gh == 0 {
        return None;
    }
    let gw_i = gw as i32;
    let gh_i = gh as i32;
    if px >= cx2 || py >= cy2 || px + gw_i <= cx1 || py + gh_i <= cy1 {
        return None;
    }
    let bx_lo = (cx1 - px).clamp(0, gw_i) as usize;
    let bx_hi = (cx2 - px).clamp(0, gw_i) as usize;
    let by_lo = (cy1 - py).clamp(0, gh_i) as usize;
    let by_hi = (cy2 - py).clamp(0, gh_i) as usize;
    if bx_lo >= bx_hi || by_lo >= by_hi {
        return None;
    }
    Some((bx_lo, bx_hi, by_lo, by_hi))
}

fn shared_font_inner() -> Result<Font, String> {
    let lock = UI_TEXT_FONT.get_or_init(|| Mutex::new(None));
    let family_lock = UI_TEXT_FONT_FAMILY.get_or_init(|| Mutex::new(fonts::default_font_family()));
    let mut guard = lock
        .lock()
        .map_err(|_| "ui text font mutex poisoned".to_string())?;
    let mut family_guard = family_lock
        .lock()
        .map_err(|_| "ui text font family mutex poisoned".to_string())?;
    let current_family = fonts::default_font_family();
    if *family_guard != current_family {
        *guard = None;
        *family_guard = current_family;
    }
    if let Some(font) = guard.as_ref() {
        return Ok(font.clone());
    }

    let font = fonts::default_font();
    *guard = Some(font.clone());
    Ok(font)
}

struct ViewportUiRasterSlot {
    last_font_sync_ver: u64,
    raster: TextRasterizer,
}

struct ViewportUiRasterPool {
    font_epoch: u64,
    by_size_bits: HashMap<u32, ViewportUiRasterSlot>,
}

static VIEWPORT_UI_RASTER_POOL: OnceLock<Mutex<ViewportUiRasterPool>> = OnceLock::new();

fn viewport_ui_raster_pool() -> &'static Mutex<ViewportUiRasterPool> {
    VIEWPORT_UI_RASTER_POOL.get_or_init(|| {
        Mutex::new(ViewportUiRasterPool {
            font_epoch: fonts::default_font_version(),
            by_size_bits: HashMap::new(),
        })
    })
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
    /// When set (e.g. viewport Study over a filled card), glyphs composite against this RGB instead of reading
    /// the framebuffer — same math as Porter–Duff SRC‑over onto a solid plate, avoids RMW bandwidth like the
    /// standalone [`crate::apps::text::TextApp`] glyph path (`text.rs`).
    pub opaque_under_rgb: Option<(u8, u8, u8)>,
}

#[derive(Clone, Debug, Default)]
pub struct UiTextRenderState {
    /// Character count per wrapped line.
    pub lines: Vec<u32>,
    /// Per character, normalized axis-aligned box: `[[x1,y1],[x2,y2]]` (top-left, bottom-right) in [0,1]².
    /// Matches tensor layout `(N, 2, 2)` with N = number of rendered glyphs.
    pub hitboxes: Vec<[[f32; 2]; 2]>,
    /// Per wrapped line, baseline segment in normalized coords: `[[x1,y1],[x2,y2]]` (segment endpoints).
    /// Matches tensor layout `(L, 2, 2)` with L = number of lines.
    pub baselines: Vec<[[f32; 2]; 2]>,
}

impl UiText {
    pub fn render(&self, buffer: &mut [u8], frame_width: usize, frame_height: usize) -> Result<UiTextRenderState, String> {
        let mut state = UiTextRenderState::default();
        if frame_width == 0 || frame_height == 0 {
            return Ok(state);
        }

        let x1 = (self.x1_norm.clamp(0.0, 1.0) * frame_width as f32).round() as i32;
        let y1 = (self.y1_norm.clamp(0.0, 1.0) * frame_height as f32).round() as i32;
        let x2 = (self.x2_norm.clamp(0.0, 1.0) * frame_width as f32).round() as i32;
        let y2 = (self.y2_norm.clamp(0.0, 1.0) * frame_height as f32).round() as i32;

        if x2 <= x1 || y2 <= y1 {
            return Ok(state);
        }

        let box_width = (x2 - x1) as f32;
        let box_height = (y2 - y1) as f32;

        let fs = quantize_viewport_raster_px(self.font_size_px.max(1.0)).max(1.0);
        let font = shared_font_inner()?;
        let mut pool = viewport_ui_raster_pool()
            .lock()
            .map_err(|_| "viewport raster pool mutex poisoned".to_string())?;
        let v = fonts::default_font_version();
        if pool.font_epoch != v {
            pool.font_epoch = v;
            pool.by_size_bits.clear();
        }
        let fb = fs.to_bits();
        let slot = pool.by_size_bits.entry(fb).or_insert_with(|| ViewportUiRasterSlot {
            last_font_sync_ver: 0,
            raster: TextRasterizer::new_viewport_global_glyph_cache(font.clone(), fs),
        });
        let _ = sync_rasterizer_to_default_font(&mut slot.raster, &mut slot.last_font_sync_ver);

        slot.raster.set_text(self.text.clone());
        slot.raster.tick(box_width, box_height);
        let rasterizer = &slot.raster;

        // Count characters per wrapped line.
        for line in &rasterizer.lines {
            state.lines.push((line.end_index.saturating_sub(line.start_index)) as u32);
        }

        if self.baselines {
            let baseline_color = (100, 100, 100, 255);
            for line in &rasterizer.lines {
                let by = y1 + line.baseline_y.round() as i32;
                let y_norm = (by as f32 / frame_height as f32).clamp(0.0, 1.0);
                state.baselines.push([
                    [
                        (x1 as f32 / frame_width as f32).clamp(0.0, 1.0),
                        y_norm,
                    ],
                    [
                        (x2 as f32 / frame_width as f32).clamp(0.0, 1.0),
                        y_norm,
                    ],
                ]);
                if by >= y1 && by < y2 {
                    fill_rect_buffer(buffer, frame_width, frame_height, x1, by, x2, by + 1, baseline_color);
                }
            }
        } else {
            for line in &rasterizer.lines {
                let by = y1 + line.baseline_y.round() as i32;
                let y_norm = (by as f32 / frame_height as f32).clamp(0.0, 1.0);
                state.baselines.push([
                    [
                        (x1 as f32 / frame_width as f32).clamp(0.0, 1.0),
                        y_norm,
                    ],
                    [
                        (x2 as f32 / frame_width as f32).clamp(0.0, 1.0),
                        y_norm,
                    ],
                ]);
            }
        }

        for character in &rasterizer.characters {
            let px = x1 + character.x.round() as i32;
            let py = y1 + character.y.round() as i32;
            let gx1 = px;
            let gy1 = py;
            let gx2 = px + character.metrics.width as i32;
            let gy2 = py + character.metrics.height as i32;
            state.hitboxes.push([
                [
                    (gx1 as f32 / frame_width as f32).clamp(0.0, 1.0),
                    (gy1 as f32 / frame_height as f32).clamp(0.0, 1.0),
                ],
                [
                    (gx2 as f32 / frame_width as f32).clamp(0.0, 1.0),
                    (gy2 as f32 / frame_height as f32).clamp(0.0, 1.0),
                ],
            ]);
            let Some((bx_lo, bx_hi, by_lo, by_hi)) = viewport_glyph_blit_ranges(
                px,
                py,
                character.metrics.width,
                character.metrics.height,
                x1,
                y1,
                x2,
                y2,
            ) else {
                continue;
            };

            let w_bm = character.metrics.width;
            let fg_a = self.color.3 as f32 / 255.0;

            match (self.opaque_under_rgb, self.color.3 >= 253) {
                (Some((ur, ug, ub)), true) => {
                    for by in by_lo..by_hi {
                        let row = by * w_bm;
                        for bx in bx_lo..bx_hi {
                            let glyph_alpha = character.bitmap[row + bx];
                            if glyph_alpha == 0 {
                                continue;
                            }
                            let sx = px + bx as i32;
                            let sy = py + by as i32;
                            let idx = ((sy as usize * frame_width + sx as usize) * 4) as usize;
                            let ga = glyph_alpha as u16;
                            let inv = (255_u16).saturating_sub(ga);
                            buffer[idx] =
                                ((self.color.0 as u16 * ga + ur as u16 * inv + 127) / 255) as u8;
                            buffer[idx + 1] =
                                ((self.color.1 as u16 * ga + ug as u16 * inv + 127) / 255) as u8;
                            buffer[idx + 2] =
                                ((self.color.2 as u16 * ga + ub as u16 * inv + 127) / 255) as u8;
                            buffer[idx + 3] = 0xff;
                        }
                    }
                }
                (Some((ur, ug, ub)), false) => {
                    for by in by_lo..by_hi {
                        let row = by * w_bm;
                        for bx in bx_lo..bx_hi {
                            let glyph_alpha = character.bitmap[row + bx];
                            if glyph_alpha == 0 {
                                continue;
                            }
                            let sx = px + bx as i32;
                            let sy = py + by as i32;
                            let idx = ((sy as usize * frame_width + sx as usize) * 4) as usize;
                            let alpha = (glyph_alpha as f32 / 255.0) * fg_a;
                            let inv_alpha = 1.0 - alpha;

                            buffer[idx] =
                                (self.color.0 as f32 * alpha + ur as f32 * inv_alpha) as u8;
                            buffer[idx + 1] =
                                (self.color.1 as f32 * alpha + ug as f32 * inv_alpha) as u8;
                            buffer[idx + 2] =
                                (self.color.2 as f32 * alpha + ub as f32 * inv_alpha) as u8;
                            buffer[idx + 3] = 0xff;
                        }
                    }
                }
                (None, true) => {
                    for by in by_lo..by_hi {
                        let row = by * w_bm;
                        for bx in bx_lo..bx_hi {
                            let glyph_alpha = character.bitmap[row + bx];
                            if glyph_alpha == 0 {
                                continue;
                            }
                            let sx = px + bx as i32;
                            let sy = py + by as i32;
                            let idx = ((sy as usize * frame_width + sx as usize) * 4) as usize;
                            let ga = glyph_alpha as u16;
                            let inv = (255_u16).saturating_sub(ga);
                            buffer[idx] =
                                ((self.color.0 as u16 * ga + buffer[idx] as u16 * inv + 127) / 255) as u8;
                            buffer[idx + 1] = ((self.color.1 as u16 * ga
                                + buffer[idx + 1] as u16 * inv
                                + 127)
                                / 255) as u8;
                            buffer[idx + 2] = ((self.color.2 as u16 * ga
                                + buffer[idx + 2] as u16 * inv
                                + 127)
                                / 255) as u8;
                            buffer[idx + 3] = 0xff;
                        }
                    }
                }
                (None, false) => {
                    for by in by_lo..by_hi {
                        let row = by * w_bm;
                        for bx in bx_lo..bx_hi {
                            let glyph_alpha = character.bitmap[row + bx];
                            if glyph_alpha == 0 {
                                continue;
                            }
                            let sx = px + bx as i32;
                            let sy = py + by as i32;
                            let idx = ((sy as usize * frame_width + sx as usize) * 4) as usize;
                            let alpha = (glyph_alpha as f32 / 255.0) * fg_a;
                            let inv_alpha = 1.0 - alpha;

                            buffer[idx] =
                                (self.color.0 as f32 * alpha + buffer[idx] as f32 * inv_alpha) as u8;
                            buffer[idx + 1] = (self.color.1 as f32 * alpha
                                + buffer[idx + 1] as f32 * inv_alpha)
                                as u8;
                            buffer[idx + 2] = (self.color.2 as f32 * alpha
                                + buffer[idx + 2] as f32 * inv_alpha)
                                as u8;
                            buffer[idx + 3] = 0xff;
                        }
                    }
                }
            }

            if self.hitboxes {
                let hitbox_color = (255, 0, 0, 255);
                fill_rect_buffer(buffer, frame_width, frame_height, gx1, gy1, gx2, gy1 + 1, hitbox_color);
                fill_rect_buffer(buffer, frame_width, frame_height, gx1, gy2 - 1, gx2, gy2, hitbox_color);
                fill_rect_buffer(buffer, frame_width, frame_height, gx1, gy1, gx1 + 1, gy2, hitbox_color);
                fill_rect_buffer(buffer, frame_width, frame_height, gx2 - 1, gy1, gx2, gy2, hitbox_color);
            }
        }

        Ok(state)
    }
}

#[inline]
fn normalized_selection(selection_start: Option<usize>, selection_end: Option<usize>) -> Option<(usize, usize)> {
    let (start, end) = (selection_start?, selection_end?);
    Some(if start <= end { (start, end) } else { (end, start) })
}

pub fn save_undo_state(
    undo_stack: &mut Vec<(String, usize)>,
    redo_stack: &mut Vec<(String, usize)>,
    text: &str,
    cursor_position: usize,
) {
    undo_stack.push((text.to_string(), cursor_position));
    if undo_stack.len() > 100 {
        undo_stack.remove(0);
    }
    redo_stack.clear();
}

pub fn delete_selection(
    text: &mut String,
    cursor_position: &mut usize,
    selection_start: &mut Option<usize>,
    selection_end: &mut Option<usize>,
) -> bool {
    let Some((start_idx, end_idx)) = normalized_selection(*selection_start, *selection_end) else {
        return false;
    };
    let text_chars: Vec<char> = text.chars().collect();
    let mut new_text = String::new();
    for (i, &c) in text_chars.iter().enumerate() {
        if i < start_idx || i >= end_idx {
            new_text.push(c);
        }
    }
    *text = new_text;
    *cursor_position = start_idx;
    *selection_start = None;
    *selection_end = None;
    true
}

pub fn copy_selection(text: &str, selection_start: Option<usize>, selection_end: Option<usize>) -> Option<String> {
    let (start_idx, end_idx) = normalized_selection(selection_start, selection_end)?;
    let text_chars: Vec<char> = text.chars().collect();
    let end_idx = end_idx.min(text_chars.len());
    if start_idx >= end_idx {
        return None;
    }
    let selected: String = text_chars[start_idx..end_idx].iter().collect();
    if selected.is_empty() {
        None
    } else {
        Some(selected)
    }
}

pub fn insert_text_at_cursor(text: &mut String, cursor_position: &mut usize, insert: &str) {
    if insert.is_empty() {
        return;
    }
    let text_chars: Vec<char> = text.chars().collect();
    let mut new_text = String::new();
    for (i, &c) in text_chars.iter().enumerate() {
        if i == *cursor_position {
            new_text.push_str(insert);
        }
        new_text.push(c);
    }
    if *cursor_position >= text_chars.len() {
        new_text.push_str(insert);
    }
    *text = new_text;
    *cursor_position += insert.chars().count();
}

pub fn paste_at_cursor(
    text: &mut String,
    cursor_position: &mut usize,
    selection_start: &mut Option<usize>,
    selection_end: &mut Option<usize>,
    clipboard_fallback: &str,
) -> Option<String> {
    let clipboard_text = crate::clipboard::get_contents().unwrap_or_else(|| clipboard_fallback.to_string());
    if clipboard_text.is_empty() {
        return None;
    }
    let _ = delete_selection(text, cursor_position, selection_start, selection_end);
    insert_text_at_cursor(text, cursor_position, &clipboard_text);
    Some(clipboard_text)
}

pub fn select_all_toggle(
    text: &str,
    cursor_position: &mut usize,
    selection_start: &mut Option<usize>,
    selection_end: &mut Option<usize>,
) {
    let text_len = text.chars().count();
    if *selection_start == Some(0) && *selection_end == Some(text_len) {
        *selection_start = None;
        *selection_end = None;
        return;
    }
    *selection_start = Some(0);
    *selection_end = Some(text_len);
    *cursor_position = text_len;
}
