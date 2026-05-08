//! Read-only text in a screen rectangle: same scroll / wheel / drag physics and glyph draw path as
//! [`super::text::TextApp`](crate::apps::text::TextApp) — no keyboard, cursor, selection, or debug overlays.

use crate::engine::{EngineState, ScrollWheelUnit};
use crate::ui::onscreen_keyboard::KeyType;
use crate::rasterizer::text::text_rasterization::TextRasterizer;
use crate::time::Instant;
use fontdue::Font;
use std::time::Duration;

// Keep in sync with `text.rs` for identical feel
const SCROLL_REF_DT: f32 = 1.0 / 60.0;
const SCROLL_ELASTIC_RATE: f32 = 28.0;
const SCROLL_OVERSCROLL_LIMIT: f32 = 0.25;
const SCROLL_SMOOTH_RATE: f32 = 40.0;
const DRAG_MOMENTUM_DECAY: f32 = 0.94;
const DRAG_MOMENTUM_STOP: f32 = 38.0;
const MOUSE_WHEEL_LINE_SCALE: f32 = 240.0;
const TRACKPAD_SCROLL_PIXEL_SCALE: f32 = 1.0;
const WHEEL_CHARGE_PER_NOTCH: f32 = 0.085;
const WHEEL_ACCEL_IDLE_DECAY: f32 = 0.86;
const WHEEL_STREAK_HOLD: Duration = Duration::from_millis(72);
const H_PAD: f32 = 6.0;

const TEXT_R: u8 = 230;
const TEXT_G: u8 = 236;
const TEXT_B: u8 = 240;
const CURSOR_COLOR: (u8, u8, u8) = (0, 255, 0);
const SELECTION_COLOR: (u8, u8, u8, u8) = (50, 120, 200, 128);
const DOUBLE_TAP_TIME_MS: u64 = 300;
const DOUBLE_TAP_DISTANCE: f32 = 50.0;

#[derive(Clone, Debug)]
struct ColorSpan {
    start_char: usize,
    end_char: usize,
    color: (u8, u8, u8),
}

/// Read-only view: TextApp-equivalent scroll + draw in a sub-rect.
pub struct TranscriptTextView {
    text_rasterizer: TextRasterizer,
    pub scroll_y: f32,
    pub scroll_target: f32,
    dragging: bool,
    last_mouse_y: f32,
    drag_scroll_momentum: f32,
    last_drag_sample_time: Option<Instant>,
    wheel_accel_target: f32,
    wheel_last_activity: Option<Instant>,
    /// When the user is scrolled to the bottom, new content can auto-snap there.
    pub stick_to_tail: bool,
    pending_snap_to_tail: bool,
    color_spans: Vec<ColorSpan>,
    cursor_position: usize,
    selection_start: Option<usize>,
    selection_end: Option<usize>,
    trackpad_active: bool,
    trackpad_selecting: bool,
    trackpad_moved: bool,
    trackpad_laser_x: Option<f32>,
    trackpad_laser_y: Option<f32>,
    trackpad_last_mouse_x: Option<f32>,
    trackpad_last_mouse_y: Option<f32>,
    selecting: bool,
    last_pointer_x: f32,
    last_rect: (f32, f32, f32, f32),
    trackpad_last_tap_time: Option<Instant>,
    trackpad_last_tap_x: f32,
    trackpad_last_tap_y: f32,
    smooth_cursor_x: f32,
    selection_anim_phase: f32,
}

impl TranscriptTextView {
    pub fn new(font: Font, font_size: f32) -> Self {
        Self {
            text_rasterizer: TextRasterizer::new(font, font_size),
            scroll_y: 0.0,
            scroll_target: 0.0,
            dragging: false,
            last_mouse_y: 0.0,
            drag_scroll_momentum: 0.0,
            last_drag_sample_time: None,
            wheel_accel_target: 0.0,
            wheel_last_activity: None,
            stick_to_tail: true,
            pending_snap_to_tail: false,
            color_spans: Vec::new(),
            cursor_position: 0,
            selection_start: None,
            selection_end: None,
            trackpad_active: false,
            trackpad_selecting: false,
            trackpad_moved: false,
            trackpad_laser_x: None,
            trackpad_laser_y: None,
            trackpad_last_mouse_x: None,
            trackpad_last_mouse_y: None,
            selecting: false,
            last_pointer_x: 0.0,
            last_rect: (0.0, 0.0, 0.0, 0.0),
            trackpad_last_tap_time: None,
            trackpad_last_tap_x: 0.0,
            trackpad_last_tap_y: 0.0,
            smooth_cursor_x: 0.0,
            selection_anim_phase: 0.0,
        }
    }

    pub fn set_font_size(&mut self, font_size: f32) {
        self.text_rasterizer.set_font_size(font_size);
    }

    pub fn set_font(&mut self, font: Font) {
        let font_size = self.text_rasterizer.font_size;
        let text = self.text_rasterizer.text.clone();
        self.text_rasterizer = TextRasterizer::new(font, font_size);
        self.text_rasterizer.set_text(text);
    }

    /// Replace document text. If [`Self::stick_to_tail`] is true and the string changed, we snap to
    /// the bottom after layout (new transcript lines).
    pub fn set_text(&mut self, text: String) {
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        let cur = self.text_rasterizer.text.replace("\r\n", "\n").replace('\r', "\n");
        if normalized != cur && self.stick_to_tail {
            self.pending_snap_to_tail = true;
        }
        self.text_rasterizer.set_text(text);
        let max_idx = self.text_rasterizer.text.chars().count();
        self.cursor_position = self.cursor_position.min(max_idx);
        self.selection_start = self.selection_start.map(|v| v.min(max_idx));
        self.selection_end = self.selection_end.map(|v| v.min(max_idx));
    }

    /// Set text plus optional per-character spans (half-open ranges in char indices).
    pub fn set_text_with_color_spans(
        &mut self,
        text: String,
        spans: Vec<(usize, usize, (u8, u8, u8))>,
    ) {
        self.set_text(text);
        self.color_spans = spans
            .into_iter()
            .filter(|(s, e, _)| s < e)
            .map(|(start_char, end_char, color)| ColorSpan {
                start_char,
                end_char,
                color,
            })
            .collect();
    }

    /// Same wheel semantics as [`super::text::TextApp::on_scroll`](crate::apps::text::TextApp::on_scroll).
    pub fn on_scroll(&mut self, dy: f32, unit: ScrollWheelUnit) {
        self.stick_to_tail = false;
        self.drag_scroll_momentum = 0.0;
        self.wheel_last_activity = Some(Instant::now());
        let scaled = match unit {
            ScrollWheelUnit::Line => dy * MOUSE_WHEEL_LINE_SCALE,
            ScrollWheelUnit::Pixel => dy * TRACKPAD_SCROLL_PIXEL_SCALE,
        };
        self.wheel_accel_target = (self.wheel_accel_target + WHEEL_CHARGE_PER_NOTCH).min(1.0);
        let mult = 1.0 + 2.0 * self.wheel_accel_target;
        self.scroll_target -= scaled * mult;
    }

    pub fn on_mouse_down(&mut self, x: f32, y: f32) {
        self.dragging = true;
        self.last_mouse_y = y;
        self.last_pointer_x = x;
        self.selecting = false;
        self.last_drag_sample_time = None;
    }

    pub fn on_mouse_move_drag(&mut self, x: f32, y: f32, _keyboard_shown: bool) {
        if !self.dragging && !self.selecting {
            return;
        }
        if self.dragging {
            let dx = x - self.last_pointer_x;
            let dy = y - self.last_mouse_y;
            let abs_dx = dx.abs();
            let abs_dy = dy.abs();
            if abs_dx > 5.0 || abs_dy > 5.0 {
                #[cfg(not(target_os = "ios"))]
                {
                    // Vertical drag → scroll (like a touchpad/page); horizontal → text selection.
                    // Ignore `keyboard_shown` so OSK open does not eat vertical scrolling.
                    let should_select = abs_dx > abs_dy;
                    if should_select {
                        self.dragging = false;
                        self.selecting = true;
                        let start_idx =
                            self.find_nearest_char_index(self.last_pointer_x, self.last_mouse_y, self.last_rect);
                        self.selection_start = Some(start_idx);
                        self.selection_end = Some(start_idx);
                        self.cursor_position = start_idx;
                    }
                }
            }
        }
        if self.selecting {
            let idx = self.find_nearest_char_index(x, y, self.last_rect);
            self.selection_end = Some(idx);
            self.cursor_position = idx;
            self.last_pointer_x = x;
            self.last_mouse_y = y;
            return;
        }
        if !self.dragging {
            return;
        }
        let now = Instant::now();
        let dy = y - self.last_mouse_y;
        if let Some(t0) = self.last_drag_sample_time {
            let sample_dt = now.duration_since(t0).as_secs_f32().max(1e-4);
            let instant_v = (-dy) / sample_dt;
            self.drag_scroll_momentum = self.drag_scroll_momentum * 0.42 + instant_v * 0.58;
        } else {
            self.drag_scroll_momentum = 0.0;
        }
        self.last_drag_sample_time = Some(now);
        self.wheel_accel_target = 0.0;
        self.wheel_last_activity = None;
        self.scroll_target -= dy;
        self.scroll_y = self.scroll_target;
        self.last_pointer_x = x;
        self.last_mouse_y = y;
        self.stick_to_tail = false;
    }

    pub fn on_mouse_up(&mut self) {
        self.dragging = false;
        self.selecting = false;
        self.last_drag_sample_time = None;
    }

    pub fn on_action_key(&mut self, action: KeyType) -> Option<String> {
        match action {
            KeyType::Mouse => {
                self.trackpad_active = false;
                self.trackpad_selecting = false;
                None
            }
            KeyType::Copy => self.copy_selection_text(),
            KeyType::SelectAll => {
                let n = self.text_rasterizer.text.chars().count();
                if self.selection_start.is_some() && self.selection_end.is_some() {
                    self.selection_start = None;
                    self.selection_end = None;
                } else {
                    self.selection_start = Some(0);
                    self.selection_end = Some(n);
                    self.cursor_position = n;
                }
                None
            }
            _ => None,
        }
    }

    pub fn on_trackpad_pointer_down(&mut self, mx: f32, my: f32, rect: (f32, f32, f32, f32)) {
        self.trackpad_active = true;
        self.trackpad_last_mouse_x = Some(mx);
        self.trackpad_last_mouse_y = Some(my);
        self.trackpad_moved = false;
        if self.trackpad_laser_x.is_none() || self.trackpad_laser_y.is_none() {
            self.initialize_laser_at_cursor(rect);
        }
        let now = Instant::now();
        let is_double_tap = self.trackpad_last_tap_time.is_some_and(|last_time| {
            let time_since_last = now.duration_since(last_time);
            let distance = ((mx - self.trackpad_last_tap_x).powi(2)
                + (my - self.trackpad_last_tap_y).powi(2))
            .sqrt();
            time_since_last < Duration::from_millis(DOUBLE_TAP_TIME_MS)
                && distance < DOUBLE_TAP_DISTANCE
        });
        if is_double_tap {
            self.trackpad_selecting = true;
            self.selection_start = Some(self.cursor_position);
            self.selection_end = Some(self.cursor_position);
            self.trackpad_last_tap_time = None;
        } else {
            self.trackpad_last_tap_time = Some(now);
            self.trackpad_last_tap_x = mx;
            self.trackpad_last_tap_y = my;
        }
    }

    pub fn on_trackpad_pointer_move(&mut self, mx: f32, my: f32, rect: (f32, f32, f32, f32), is_left_clicking: bool) {
        if !self.trackpad_active || !is_left_clicking {
            return;
        }
        let (Some(last_x), Some(last_y), Some(lx), Some(ly)) = (
            self.trackpad_last_mouse_x,
            self.trackpad_last_mouse_y,
            self.trackpad_laser_x,
            self.trackpad_laser_y,
        ) else {
            return;
        };
        let dx = mx - last_x;
        let dy = my - last_y;
        if dx.abs() > 2.0 || dy.abs() > 2.0 {
            self.trackpad_moved = true;
        }
        let nx = (lx + dx * 2.0).clamp(rect.0, rect.2);
        let ny = (ly + dy * 2.0).clamp(rect.1, rect.3);
        self.trackpad_laser_x = Some(nx);
        self.trackpad_laser_y = Some(ny);
        self.trackpad_last_mouse_x = Some(mx);
        self.trackpad_last_mouse_y = Some(my);

        let char_idx = self.find_nearest_char_index(nx, ny, rect);
        self.cursor_position = char_idx;
        if self.trackpad_selecting {
            self.selection_end = Some(char_idx);
        }
    }

    pub fn on_trackpad_pointer_up(&mut self) {
        let was_trackpad_session = self.trackpad_active;
        // Only apply trackpad tap-to-clear when this up ends a laser session (see transcribe mouse_up).
        if was_trackpad_session && !self.trackpad_moved && !self.trackpad_selecting {
            self.selection_start = None;
            self.selection_end = None;
        }
        self.trackpad_active = false;
        self.trackpad_selecting = false;
        self.trackpad_moved = false;
        self.trackpad_last_mouse_x = None;
        self.trackpad_last_mouse_y = None;
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging
    }

    /// Autoscroll when the trackpad laser rests near the top/bottom clip of the transcript.
    fn apply_laser_edge_autoscroll(&mut self, ly: Option<f32>, rect: (f32, f32, f32, f32)) {
        let Some(ly) = ly else { return };
        let (_, ry0, _, ry1) = rect;
        let visible_height = (ry1 - ry0).max(1.0);
        // Older 1%-of-height thresholds were unusably thin; ~8%+ with sane minimum matches finger/laser UX.
        let edge_threshold = (visible_height * 0.085).clamp(24.0, visible_height * 0.42);
        let dist_from_top = ly - ry0;
        let dist_from_bottom = ry1 - ly;
        let base_scroll_speed = 26.0_f32;
        if dist_from_top >= 0.0 && dist_from_top <= edge_threshold {
            let progress = 1.0 - (dist_from_top / edge_threshold.max(1.0));
            let scroll_speed = base_scroll_speed * progress;
            self.scroll_target = (self.scroll_target - scroll_speed).max(0.0);
            self.scroll_y = self.scroll_target;
            self.drag_scroll_momentum = 0.0;
            self.wheel_accel_target = 0.0;
            self.wheel_last_activity = None;
            self.stick_to_tail = false;
        } else if dist_from_bottom >= 0.0 && dist_from_bottom <= edge_threshold {
            let progress = 1.0 - (dist_from_bottom / edge_threshold.max(1.0));
            let scroll_speed = base_scroll_speed * progress;
            self.scroll_target += scroll_speed;
            self.scroll_y = self.scroll_target;
            self.drag_scroll_momentum = 0.0;
            self.wheel_accel_target = 0.0;
            self.wheel_last_activity = None;
            self.stick_to_tail = false;
        }
    }

    /// `rect` is `(x0, y0, x1, y1)` in frame pixels. Clips drawing to that rectangle.
    pub fn tick(&mut self, state: &mut EngineState, rect: (f32, f32, f32, f32)) {
        self.last_rect = rect;
        let (rx0, ry0, rx1, ry1) = rect;
        let content_w = (rx1 - rx0 - H_PAD * 2.0).max(1.0);
        let visible_height = (ry1 - ry0).max(1.0);
        let content_top = ry0;
        let ox = rx0 + H_PAD;

        let dt = state.delta_time_seconds.clamp(1e-4, 0.1);
        self.selection_anim_phase += dt;

        let wheel_idle_for_decay = match self.wheel_last_activity {
            None => true,
            Some(t) => t.elapsed() >= WHEEL_STREAK_HOLD,
        };
        if wheel_idle_for_decay {
            self.wheel_accel_target *= WHEEL_ACCEL_IDLE_DECAY.powf(dt / SCROLL_REF_DT);
            self.wheel_accel_target = self.wheel_accel_target.clamp(0.0, 1.0);
        }

        if !self.dragging && self.drag_scroll_momentum.abs() > DRAG_MOMENTUM_STOP {
            self.scroll_target += self.drag_scroll_momentum * dt;
            self.drag_scroll_momentum *= DRAG_MOMENTUM_DECAY.powf(dt / SCROLL_REF_DT);
        } else if !self.dragging {
            self.drag_scroll_momentum = 0.0;
        }

        // Reflow before scroll limits (same as [`TextApp::tick`]) so wrap width → correct `natural_max`.
        self.text_rasterizer.tick(content_w, visible_height);

        let lh = self.text_rasterizer.ascent
            + self.text_rasterizer.descent.abs()
            + self.text_rasterizer.line_gap;
        let d = self.text_rasterizer.descent.abs();
        let last_line_bottom = self
            .text_rasterizer
            .lines
            .last()
            .map(|l| l.baseline_y + d)
            .unwrap_or(lh);
        let doc_bottom = if self.text_rasterizer.characters.is_empty() {
            last_line_bottom.max(lh).max(1.0)
        } else {
            let mut glyph_bottom = f32::NEG_INFINITY;
            for c in &self.text_rasterizer.characters {
                glyph_bottom = glyph_bottom.max(c.y + c.height);
            }
            glyph_bottom.max(last_line_bottom).max(1.0)
        };
        let natural_min = 0.0;
        let natural_max = (doc_bottom - visible_height).max(0.0);
        let overscroll_distance = visible_height * SCROLL_OVERSCROLL_LIMIT;
        let limit_min = natural_min - overscroll_distance;
        let limit_max = natural_max + overscroll_distance;

        if !self.dragging {
            if self.scroll_target < natural_min {
                let overshoot = natural_min - self.scroll_target;
                let b = 1.0 - (-SCROLL_ELASTIC_RATE * dt).exp();
                self.scroll_target += overshoot * b;
            } else if self.scroll_target > natural_max {
                let overshoot = self.scroll_target - natural_max;
                let b = 1.0 - (-SCROLL_ELASTIC_RATE * dt).exp();
                self.scroll_target -= overshoot * b;
            }
            self.scroll_target = self.scroll_target.max(limit_min).min(limit_max);

            let diff = self.scroll_target - self.scroll_y;
            if diff.abs() > 0.02 {
                let alpha = 1.0 - (-SCROLL_SMOOTH_RATE * dt).exp();
                self.scroll_y += diff * alpha;
            } else {
                self.scroll_y = self.scroll_target;
            }
        } else {
            self.scroll_target = self.scroll_target.max(limit_min).min(limit_max);
            self.scroll_y = self.scroll_target;
        }

        // Growing transcript / tail snap after layout-aligned bounds above.
        if self.pending_snap_to_tail {
            self.scroll_target = natural_max;
            self.scroll_y = natural_max;
            self.pending_snap_to_tail = false;
            self.stick_to_tail = true;
        } else {
            self.scroll_target = self.scroll_target.max(limit_min).min(limit_max);
            if !self.dragging {
                self.scroll_y = self.scroll_y.max(limit_min).min(limit_max);
            }
        }

        if (natural_max - self.scroll_target).abs() < lh * 0.75 {
            self.stick_to_tail = true;
        }

        let shape = state.frame.shape();
        let width = shape[1] as f32;
        let height = shape[0] as f32;
        let w_i = width as i32;
        let h_i = height as i32;
        let show_trackpad_laser =
            state.keyboard.onscreen.is_trackpad_mode() && state.keyboard.onscreen.is_shown();
        let buffer = state.frame_buffer_mut();
        let vis_top = self.scroll_y;
        let vis_bottom = self.scroll_y + visible_height;

        if show_trackpad_laser && self.trackpad_active {
            self.apply_laser_edge_autoscroll(self.trackpad_laser_y, rect);
            if let (Some(lx), Some(ly)) = (self.trackpad_laser_x, self.trackpad_laser_y) {
                let idx = self.find_nearest_char_index(lx, ly, rect);
                self.cursor_position = idx;
                if self.trackpad_selecting {
                    self.selection_end = Some(idx);
                }
            }
        }

        if let (Some(sel_start), Some(sel_end)) = (self.selection_start, self.selection_end) {
            let (start_idx, end_idx) = if sel_start <= sel_end { (sel_start, sel_end) } else { (sel_end, sel_start) };
            let pulse_alpha =
                SELECTION_COLOR.3 as f32 / 255.0 * ((self.selection_anim_phase * 2.4).sin() * 0.12 + 0.88);
            let mut per_line: std::collections::HashMap<usize, (f32, f32, f32)> = std::collections::HashMap::new();
            for c in &self.text_rasterizer.characters {
                if c.char_index >= start_idx && c.char_index < end_idx {
                    let left = c.x;
                    let right = c.x + c.metrics.advance_width;
                    let baseline_y = self.text_rasterizer.lines.get(c.line_index).map(|l| l.baseline_y).unwrap_or(0.0);
                    per_line
                        .entry(c.line_index)
                        .and_modify(|(min_x, max_x, _)| {
                            *min_x = min_x.min(left);
                            *max_x = max_x.max(right);
                        })
                        .or_insert((left, right, baseline_y));
                }
            }
            for (_, (min_x, max_x, baseline_y)) in per_line {
                let sel_left = (ox + min_x).round() as i32;
                let sel_right = (ox + max_x).round() as i32;
                let sel_top = ((baseline_y - self.text_rasterizer.ascent - self.scroll_y) + content_top) as i32;
                let sel_bottom = ((baseline_y + self.text_rasterizer.descent - self.scroll_y) + content_top) as i32;
                for y in sel_top.max(0)..sel_bottom.min(h_i) {
                    for x in sel_left.max(0)..sel_right.min(w_i) {
                        let sxf = x as f32;
                        let syf = y as f32;
                        if sxf < rx0 || sxf > rx1 || syf < ry0 || syf > ry1 {
                            continue;
                        }
                        let idx = ((y as u32 * shape[1] as u32 + x as u32) * 4) as usize;
                        if idx + 3 < buffer.len() {
                            let a = pulse_alpha;
                            let inv = 1.0 - a;
                            buffer[idx] = (buffer[idx] as f32 * inv + SELECTION_COLOR.0 as f32 * a) as u8;
                            buffer[idx + 1] = (buffer[idx + 1] as f32 * inv + SELECTION_COLOR.1 as f32 * a) as u8;
                            buffer[idx + 2] = (buffer[idx + 2] as f32 * inv + SELECTION_COLOR.2 as f32 * a) as u8;
                        }
                    }
                }
            }
        }

        for character in &self.text_rasterizer.characters {
            let g_top = character.y;
            let g_bottom = character.y + character.height;
            if g_bottom < vis_top || g_top > vis_bottom {
                continue;
            }
            let slide_max = character.width;
            let g_left = character.x - slide_max;
            let g_right = character.x + character.metrics.advance_width + character.metrics.width as f32;
            if g_right < 0.0 || g_left > content_w {
                continue;
            }

            let px_base = (ox + character.x) as i32;
            let py_base = ((character.y - self.scroll_y) + content_top) as i32;

            for y in 0..character.metrics.height {
                for x in 0..character.metrics.width {
                    let val = character.bitmap[y * character.metrics.width + x];
                    if val == 0 {
                        continue;
                    }
                    let sx = px_base + x as i32;
                    let sy = py_base + y as i32;
                    let syf = sy as f32;
                    let sxf = sx as f32;
                    if sxf < rx0 || sxf > rx1 || syf < ry0 || syf > ry1 {
                        continue;
                    }
                    if sx < 0 || sx >= w_i || sy < 0 || sy >= h_i {
                        continue;
                    }
                    let idx = ((sy as u32 * shape[1] as u32 + sx as u32) * 4) as usize;
                    if idx + 3 < buffer.len() {
                        let alpha = val as f32 / 255.0;
                        let inv = 1.0 - alpha;
                        let (r, g, b) = self
                            .color_spans
                            .iter()
                            .find(|s| {
                                character.char_index >= s.start_char
                                    && character.char_index < s.end_char
                            })
                            .map(|s| s.color)
                            .unwrap_or((TEXT_R, TEXT_G, TEXT_B));
                        buffer[idx] = (r as f32 * alpha + buffer[idx] as f32 * inv) as u8;
                        buffer[idx + 1] = (g as f32 * alpha + buffer[idx + 1] as f32 * inv) as u8;
                        buffer[idx + 2] = (b as f32 * alpha + buffer[idx + 2] as f32 * inv) as u8;
                        buffer[idx + 3] = 0xff;
                    }
                }
            }
        }

        let (mut target_cursor_x, baseline_y) = self.get_cursor_screen_position();
        target_cursor_x = target_cursor_x.clamp(0.0, content_w);
        self.smooth_cursor_x += (target_cursor_x - self.smooth_cursor_x) * 0.2_f32;

        let has_expand_selection =
            matches!((self.selection_start, self.selection_end), (Some(a), Some(b)) if a != b);
        if !has_expand_selection {
            let cx_f = ox + self.smooth_cursor_x;
            if cx_f >= rx0 && cx_f <= rx1 {
                let cursor_top = ((baseline_y - self.text_rasterizer.ascent - self.scroll_y) + content_top).round() as i32;
                let cursor_bottom = ((baseline_y + self.text_rasterizer.descent - self.scroll_y) + content_top).round()
                    as i32;
                let cx = cx_f.round() as i32;
                let y0 = cursor_top.max(ry0.floor() as i32).max(0);
                let y1 = cursor_bottom.min(ry1.ceil() as i32).min(h_i);
                if y0 < y1 && cx >= 0 && cx < w_i {
                    for y in y0..y1 {
                        let sy = y as f32;
                        if sy < ry0 || sy > ry1 {
                            continue;
                        }
                        let idx = ((y as u32 * shape[1] as u32 + cx as u32) * 4) as usize;
                        if idx + 3 < buffer.len() {
                            buffer[idx] = CURSOR_COLOR.0;
                            buffer[idx + 1] = CURSOR_COLOR.1;
                            buffer[idx + 2] = CURSOR_COLOR.2;
                            buffer[idx + 3] = 255;
                        }
                    }
                }
            }
        }

        if show_trackpad_laser {
            if self.trackpad_laser_x.is_none() || self.trackpad_laser_y.is_none() {
                self.initialize_laser_at_cursor(rect);
            }
            if let (Some(lx), Some(ly)) = (self.trackpad_laser_x, self.trackpad_laser_y) {
                let radius = 6_i32;
                let cx = lx.round() as i32;
                let cy = ly.round() as i32;
                for dy in -radius..=radius {
                    for dx in -radius..=radius {
                        if ((dx * dx + dy * dy) as f32).sqrt() > radius as f32 {
                            continue;
                        }
                        let x = cx + dx;
                        let y = cy + dy;
                        if x < 0 || y < 0 || x >= w_i || y >= h_i {
                            continue;
                        }
                        let idx = ((y as u32 * shape[1] as u32 + x as u32) * 4) as usize;
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
    }

    fn initialize_laser_at_cursor(&mut self, rect: (f32, f32, f32, f32)) {
        let (x, baseline_y) = self.get_cursor_screen_position();
        let y = baseline_y - self.scroll_y + rect.1;
        self.trackpad_laser_x = Some((rect.0 + H_PAD + x).clamp(rect.0, rect.2));
        self.trackpad_laser_y = Some(y.clamp(rect.1, rect.3));
    }

    fn copy_selection_text(&self) -> Option<String> {
        let (Some(a), Some(b)) = (self.selection_start, self.selection_end) else {
            return None;
        };
        let (s, e) = if a <= b { (a, b) } else { (b, a) };
        if s == e {
            return None;
        }
        let chars: Vec<char> = self.text_rasterizer.text.chars().collect();
        Some(chars[s.min(chars.len())..e.min(chars.len())].iter().collect())
    }

    fn find_nearest_char_index(&self, screen_x: f32, screen_y: f32, rect: (f32, f32, f32, f32)) -> usize {
        let text_x = screen_x - (rect.0 + H_PAD);
        let text_y = screen_y - rect.1 + self.scroll_y;
        let mut nearest_idx = self.text_rasterizer.text.chars().count();
        let mut min_dist_sq = f32::MAX;
        for c in &self.text_rasterizer.characters {
            let cx = c.x + c.width * 0.5;
            let cy = c.y + c.height * 0.5;
            let dx = text_x - cx;
            let dy = text_y - cy;
            let d = dx * dx + dy * dy;
            if d < min_dist_sq {
                min_dist_sq = d;
                nearest_idx = if text_x > cx { c.char_index + 1 } else { c.char_index };
            }
        }
        nearest_idx.min(self.text_rasterizer.text.chars().count())
    }

    fn get_cursor_screen_position(&self) -> (f32, f32) {
        let cursor = self.cursor_position;
        let line_info_with_idx = self
            .text_rasterizer
            .lines
            .iter()
            .enumerate()
            .find(|(_, line)| line.start_index <= cursor && cursor <= line.end_index);
        if let Some((line_idx, line)) = line_info_with_idx {
            let chars_in_line: Vec<_> = self
                .text_rasterizer
                .characters
                .iter()
                .filter(|c| c.line_index == line_idx)
                .collect();
            if chars_in_line.is_empty() || cursor == line.start_index {
                return (0.0, line.baseline_y);
            }
            if let Some(char_at_cursor) = self.text_rasterizer.characters.iter().find(|c| c.char_index == cursor) {
                return (char_at_cursor.x, line.baseline_y);
            }
            if let Some(last) = chars_in_line.last() {
                return (last.x + last.metrics.advance_width, line.baseline_y);
            }
        }
        (0.0, self.text_rasterizer.ascent)
    }
}
