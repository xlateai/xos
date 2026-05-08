use crate::rasterizer::fill_rect_buffer;
use crate::rasterizer::text::fonts::{self, FontFamily};
use crate::rasterizer::text::text_rasterization::{
    character_may_appear_in_viewport, line_band_intersects_doc_viewport, TextLayoutAlign,
    TextRasterizer,
};
use crate::rasterizer::text::ui_markup;
use fontdue::Font;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

static UI_TEXT_FONT: OnceLock<Mutex<Option<Font>>> = OnceLock::new();
static UI_TEXT_FONT_FAMILY: OnceLock<Mutex<FontFamily>> = OnceLock::new();

/// Mirrors standalone [`crate::apps::text::TextApp`] selection tint.
const SELECTION_OVERLAY_RGBA: (u8, u8, u8, u8) = (50, 120, 200, 128);
const TRACKPAD_DOT_RADIUS_PX: f32 = 3.0;

fn shared_font() -> Result<Font, String> {
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
    pub size_px: f32,
    /// When true (and layout succeeded), draws a caret at [`Self::cursor_position`] (Unicode scalar index).
    pub show_cursor: bool,
    pub cursor_position: usize,
    pub selection_start: Option<usize>,
    pub selection_end: Option<usize>,
    /// Full-frame pixel position for the trackpad laser (embedded editor only).
    pub trackpad_pointer_px: Option<(f32, f32)>,
    /// Document-space vertical scroll copied from embedded [`crate::apps::text::TextApp::scroll_y`].
    pub viewport_scroll_y: f32,
    /// Per-glyph RGB overrides keyed by Unicode-scalar indices into [`Self::text`] (from `[label](color=NAME)`).
    pub color_spans: Vec<(usize, usize, (u8, u8, u8))>,
    /// Relative scale spans from `[label](size=…)` (same indices as [`Self::text`]).
    pub scale_spans: Vec<(usize, usize, f32)>,
    /// Per-glyph hitbox enable spans (cascade / inline overrides from markup directives).
    pub hitbox_spans: Vec<(usize, usize, bool)>,
    /// Per-glyph baseline enable spans (cascade / inline overrides from markup directives).
    pub baseline_spans: Vec<(usize, usize, bool)>,
    /// Normalized alignment (`0,0` top-left; `1,1` bottom-right), matching Python `xos.ui.Text.alignment`.
    pub alignment: (f32, f32),
    /// Start-to-start spacing multipliers `(x, y)`, matching Python `xos.ui.Text.spacing`.
    pub spacing: (f32, f32),
}

#[inline]
fn glyph_bool_with_spans(
    char_index: usize,
    base: bool,
    spans: &[(usize, usize, bool)],
) -> bool {
    spans
        .iter()
        .rev()
        .find(|(s, e, _)| char_index >= *s && char_index < *e)
        .map(|span| span.2)
        .unwrap_or(base)
}

/// Caret x and baseline y in the same layout space as [`TextRasterizer::characters`] (after `tick`).
fn cursor_xy_in_layout(r: &TextRasterizer, cursor_position: usize) -> (f32, f32) {
    let line_info_with_idx = r
        .lines
        .iter()
        .enumerate()
        .find(|(_, line)| {
            line.start_index <= cursor_position && cursor_position <= line.end_index
        });

    if let Some((line_idx, line)) = line_info_with_idx {
        let chars_in_line: Vec<_> = r
            .characters
            .iter()
            .filter(|c| c.line_index == line_idx)
            .collect();

        if chars_in_line.is_empty() {
            (0.0, line.baseline_y)
        } else if cursor_position == line.start_index {
            (0.0, line.baseline_y)
        } else {
            let mut found_char = None;
            let mut char_after = None;

            for character in r.characters.iter() {
                if character.char_index == cursor_position {
                    found_char = Some(character);
                    break;
                } else if character.char_index > cursor_position && character.line_index == line_idx {
                    char_after = Some(character);
                    break;
                }
            }

            if let Some(char_at_cursor) = found_char {
                (char_at_cursor.x, line.baseline_y)
            } else if let Some(char_after_cursor) = char_after {
                (char_after_cursor.x, line.baseline_y)
            } else if let Some(last_in_line) = chars_in_line.last() {
                (
                    last_in_line.x + last_in_line.metrics.advance_width,
                    line.baseline_y,
                )
            } else {
                (0.0, line.baseline_y)
            }
        }
    } else if cursor_position == 0 {
        if let Some(first_line) = r.lines.first() {
            (0.0, first_line.baseline_y)
        } else {
            (0.0, r.ascent)
        }
    } else if cursor_position >= r.text.chars().count() {
        if let Some(last_line) = r.lines.last() {
            let last_line_idx = r.lines.len().saturating_sub(1);
            let chars_in_last_line: Vec<_> = r
                .characters
                .iter()
                .filter(|c| c.line_index == last_line_idx)
                .collect();

            if chars_in_last_line.is_empty() {
                (0.0, last_line.baseline_y)
            } else if let Some(last_char) = chars_in_last_line.last() {
                (
                    last_char.x + last_char.metrics.advance_width,
                    last_line.baseline_y,
                )
            } else {
                (0.0, last_line.baseline_y)
            }
        } else if let Some(last) = r.characters.last() {
            (
                last.x + last.metrics.advance_width,
                r.lines
                    .last()
                    .map(|line| line.baseline_y)
                    .unwrap_or(r.ascent),
            )
        } else {
            (0.0, r.ascent)
        }
    } else {
        (0.0, r.ascent)
    }
}

#[derive(Clone, Debug, Default)]
pub struct UiTextRenderState {
    /// Character count per wrapped line that currently intersects the viewport (chunked like on-screen rows).
    pub lines: Vec<u32>,
    /// Per visible glyph (`character_may_appear_in_viewport`), normalized axis-aligned box: `[[x1,y1],[x2,y2]]` in [0,1]².
    /// Matches `(N, 2, 2)`. Omitted entirely when callers pass `include_hitboxes=false`.
    pub hitboxes: Vec<[[f32; 2]; 2]>,
    /// Per viewport-visible wrapped line, baseline segment in normalized coords: `[[x1,y1],[x2,y2]]`.
    /// Same ordering/length as [`Self::lines`].
    pub baselines: Vec<[[f32; 2]; 2]>,
}

/// Builds line counts (viewport-visible lines only), matching baselines, and optional viewport-visible glyph hitboxes.
/// Mirrors chunked [`UiText::render`] tensors.
pub fn collect_ui_text_render_state(
    rasterizer: &TextRasterizer,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    sy: f32,
    frame_width: usize,
    frame_height: usize,
    include_hitboxes: bool,
) -> UiTextRenderState {
    let mut state = UiTextRenderState::default();

    let fw = frame_width as f32;
    let fh = frame_height as f32;
    let vis_doc_h = (y2 - y1).max(0) as f32;
    let layout_w = (x2 - x1).max(0) as f32;

    for line in &rasterizer.lines {
        if !line_band_intersects_doc_viewport(
            line.baseline_y,
            rasterizer.ascent,
            rasterizer.descent,
            rasterizer.line_gap,
            rasterizer.font_size,
            sy,
            vis_doc_h,
        ) {
            continue;
        }

        state
            .lines
            .push((line.end_index.saturating_sub(line.start_index)) as u32);

        let by = y1 + (line.baseline_y - sy).round() as i32;
        let y_norm = (by as f32 / fh).clamp(0.0, 1.0);
        state.baselines.push([
            [(x1 as f32 / fw).clamp(0.0, 1.0), y_norm],
            [(x2 as f32 / fw).clamp(0.0, 1.0), y_norm],
        ]);
    }

    if include_hitboxes {
        for character in &rasterizer.characters {
            if !character_may_appear_in_viewport(character, layout_w, sy, vis_doc_h) {
                continue;
            }
            let px = x1 + character.x.round() as i32;
            let py = y1 + (character.y - sy).round() as i32;
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
        }
    }

    state
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
        let sy = self.viewport_scroll_y;

        let font = shared_font()?;
        let mut rasterizer = TextRasterizer::new(font, self.size_px.max(1.0));
        rasterizer.set_text(self.text.clone());
        rasterizer.set_spacing(self.spacing.0.max(0.0), self.spacing.1.max(0.0));
        rasterizer.glyph_scale_spans.clone_from(&self.scale_spans);
        rasterizer.tick_aligned(box_width, box_height, TextLayoutAlign {
            x: self.alignment.0.clamp(0.0, 1.0),
            y: self.alignment.1.clamp(0.0, 1.0),
        });

        let baseline_color = (100, 100, 100, 255);
        for (line_idx, line) in rasterizer.lines.iter().enumerate() {
            if !line_band_intersects_doc_viewport(
                line.baseline_y,
                rasterizer.ascent,
                rasterizer.descent,
                rasterizer.line_gap,
                rasterizer.font_size,
                sy,
                box_height,
            ) {
                continue;
            }

            state
                .lines
                .push((line.end_index.saturating_sub(line.start_index)) as u32);

            let by = y1 + (line.baseline_y - sy).round() as i32;
            let y_norm = (by as f32 / frame_height as f32).clamp(0.0, 1.0);
            let line_chars: Vec<_> = rasterizer
                .characters
                .iter()
                .filter(|c| c.line_index == line_idx)
                .collect();
            let mut has_enabled = false;
            let mut all_enabled = true;
            let mut min_x = f32::MAX;
            let mut max_x = f32::MIN;
            for ch in rasterizer.characters.iter().filter(|c| c.line_index == line_idx) {
                let enabled = glyph_bool_with_spans(ch.char_index, self.baselines, &self.baseline_spans);
                if enabled {
                    has_enabled = true;
                    min_x = min_x.min(ch.x);
                    max_x = max_x.max(ch.x + ch.metrics.advance_width);
                } else {
                    all_enabled = false;
                }
            }
            if line_chars.is_empty() {
                has_enabled = self.baselines;
                all_enabled = self.baselines;
            }
            if has_enabled {
                let (bx1, bx2) = if all_enabled {
                    (x1, x2)
                } else {
                    (x1 + min_x.round() as i32, x1 + max_x.round() as i32)
                };
                state.baselines.push([
                    [
                        (bx1 as f32 / frame_width as f32).clamp(0.0, 1.0),
                        y_norm,
                    ],
                    [
                        (bx2 as f32 / frame_width as f32).clamp(0.0, 1.0),
                        y_norm,
                    ],
                ]);
                if by >= y1 && by < y2 {
                    fill_rect_buffer(
                        buffer,
                        frame_width,
                        frame_height,
                        bx1,
                        by,
                        bx2.max(bx1 + 1),
                        by + 1,
                        baseline_color,
                    );
                }
            }
        }

        if let Some((start_idx, end_idx)) = normalized_selection(self.selection_start, self.selection_end) {
            if end_idx > start_idx {
                let mut line_selections: HashMap<usize, (f32, f32, f32)> = HashMap::new();
                for character in &rasterizer.characters {
                    let line_vis = rasterizer
                        .lines
                        .get(character.line_index)
                        .map(|line| {
                            line_band_intersects_doc_viewport(
                                line.baseline_y,
                                rasterizer.ascent,
                                rasterizer.descent,
                                rasterizer.line_gap,
                                rasterizer.font_size,
                                sy,
                                box_height,
                            )
                        })
                        .unwrap_or(false);
                    if !line_vis {
                        continue;
                    }
                    if character.char_index >= start_idx && character.char_index < end_idx {
                        let char_left = character.x;
                        let char_right = character.x + character.metrics.advance_width;
                        line_selections
                            .entry(character.line_index)
                            .and_modify(|(min_x, max_x, baseline_y)| {
                                *min_x = min_x.min(char_left);
                                *max_x = max_x.max(char_right);
                                *baseline_y = rasterizer
                                    .lines
                                    .get(character.line_index)
                                    .map(|line| line.baseline_y)
                                    .unwrap_or(*baseline_y);
                            })
                            .or_insert_with(|| {
                                let baseline_y = rasterizer
                                    .lines
                                    .get(character.line_index)
                                    .map(|line| line.baseline_y)
                                    .unwrap_or(0.0);
                                (char_left, char_right, baseline_y)
                            });
                    }
                }

                let fw_i = frame_width as i32;
                let fh_i = frame_height as i32;
                for (_line_idx, (min_x, max_x, baseline_y)) in line_selections.iter() {
                    let sel_left = (*min_x as i32) + x1;
                    let sel_right = (*max_x as i32) + x1;
                    let sel_top = y1 + (baseline_y - rasterizer.ascent - sy).round() as i32;
                    let sel_bottom = y1 + (baseline_y + rasterizer.descent - sy).round() as i32;

                    let y_lo = sel_top.min(sel_bottom).max(y1).max(0).min(fh_i.min(y2));
                    let y_hi = sel_top.max(sel_bottom).max(y1).max(0).min(fh_i.min(y2));
                    let x_lo = sel_left.min(sel_right).max(x1).max(0).min(fw_i.min(x2));
                    let x_hi = sel_left.max(sel_right).max(x1).max(0).min(fw_i.min(x2));

                    let alpha_sel = SELECTION_OVERLAY_RGBA.3 as f32 / 255.0;
                    let inv_sel = 1.0 - alpha_sel;
                    for y in y_lo..y_hi {
                        for x in x_lo..x_hi {
                            let idx = ((y as usize * frame_width + x as usize) * 4) as usize;
                            if idx + 3 >= buffer.len() {
                                continue;
                            }
                            buffer[idx] = (buffer[idx] as f32 * inv_sel
                                + SELECTION_OVERLAY_RGBA.0 as f32 * alpha_sel)
                                as u8;
                            buffer[idx + 1] = (buffer[idx + 1] as f32 * inv_sel
                                + SELECTION_OVERLAY_RGBA.1 as f32 * alpha_sel)
                                as u8;
                            buffer[idx + 2] = (buffer[idx + 2] as f32 * inv_sel
                                + SELECTION_OVERLAY_RGBA.2 as f32 * alpha_sel)
                                as u8;
                        }
                    }
                }
            }
        }

        for character in &rasterizer.characters {
            let in_viewport = character_may_appear_in_viewport(character, box_width, sy, box_height);

            let px = x1 + character.x.round() as i32;
            let py = y1 + (character.y - sy).round() as i32;
            let gx1 = px;
            let gy1 = py;
            let gx2 = px + character.metrics.width as i32;
            let gy2 = py + character.metrics.height as i32;

            let hitboxes_enabled =
                glyph_bool_with_spans(character.char_index, self.hitboxes, &self.hitbox_spans);
            if hitboxes_enabled && in_viewport {
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
            }

            if !in_viewport || py >= y2 {
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

                    let base_rgb = (self.color.0, self.color.1, self.color.2);
                    let (cr, cg, cb) =
                        ui_markup::glyph_rgb_with_spans(character.char_index, base_rgb, &self.color_spans);

                    buffer[idx] = (cr as f32 * alpha + buffer[idx] as f32 * inv_alpha) as u8;
                    buffer[idx + 1] = (cg as f32 * alpha + buffer[idx + 1] as f32 * inv_alpha) as u8;
                    buffer[idx + 2] = (cb as f32 * alpha + buffer[idx + 2] as f32 * inv_alpha) as u8;
                    buffer[idx + 3] = 0xff;
                }
            }

            if hitboxes_enabled && in_viewport {
                let hitbox_color = (255, 0, 0, 255);
                fill_rect_buffer(buffer, frame_width, frame_height, gx1, gy1, gx2, gy1 + 1, hitbox_color);
                fill_rect_buffer(buffer, frame_width, frame_height, gx1, gy2 - 1, gx2, gy2, hitbox_color);
                fill_rect_buffer(buffer, frame_width, frame_height, gx1, gy1, gx1 + 1, gy2, hitbox_color);
                fill_rect_buffer(buffer, frame_width, frame_height, gx2 - 1, gy1, gx2, gy2, hitbox_color);
            }
        }

        if self.show_cursor {
            let (cx, baseline_y) = cursor_xy_in_layout(&rasterizer, self.cursor_position);
            let cursor_top = y1 + (baseline_y - rasterizer.ascent - sy).round() as i32;
            let cursor_bottom = y1 + (baseline_y + rasterizer.descent - sy).round() as i32;
            let cx_i = x1 + cx.round() as i32;

            let y_lo = cursor_top.min(cursor_bottom);
            let y_hi = cursor_top.max(cursor_bottom);
            const CURSOR: (u8, u8, u8, u8) = (0, 255, 0, 255);
            for y in y_lo..y_hi {
                if y >= y1 && y < y2 && cx_i >= x1 && cx_i < x2 {
                    fill_rect_buffer(
                        buffer,
                        frame_width,
                        frame_height,
                        cx_i,
                        y,
                        cx_i + 1,
                        y + 1,
                        CURSOR,
                    );
                }
            }
        }

        if let Some((lx, ly)) = self.trackpad_pointer_px {
            let dot_x_i = lx.round() as i32;
            let dot_y_i = ly.round() as i32;
            let r = TRACKPAD_DOT_RADIUS_PX.ceil() as i32;
            let fw_i = frame_width as i32;
            let fh_i = frame_height as i32;
            let r_sq = TRACKPAD_DOT_RADIUS_PX * TRACKPAD_DOT_RADIUS_PX;
            for dy in -r..=r {
                for dx in -r..=r {
                    if ((dx * dx + dy * dy) as f32) > r_sq {
                        continue;
                    }
                    let x = dot_x_i + dx;
                    let y = dot_y_i + dy;
                    if x >= 0 && x < fw_i && y >= 0 && y < fh_i {
                        let idx = ((y as usize * frame_width + x as usize) * 4) as usize;
                        if idx + 3 < buffer.len() {
                            buffer[idx] = 255;
                            buffer[idx + 1] = 0;
                            buffer[idx + 2] = 0;
                            buffer[idx + 3] = 255;
                        }
                    }
                }
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
