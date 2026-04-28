//! Read-only text in a screen rectangle: same scroll / wheel / drag physics and glyph draw path as
//! [`super::text::TextApp`](crate::apps::text::TextApp) — no keyboard, cursor, selection, or debug overlays.

use crate::engine::EngineState;
use crate::rasterizer::text::text_rasterization::TextRasterizer;
use fontdue::Font;
use std::time::Instant;

// Keep in sync with `text.rs` for identical feel
const SCROLL_REF_DT: f32 = 1.0 / 60.0;
const SCROLL_ELASTIC_STRENGTH: f32 = 0.04;
const SCROLL_OVERSCROLL_LIMIT: f32 = 0.25;
const SCROLL_SMOOTH_RATE: f32 = 18.0;
const DRAG_MOMENTUM_DECAY: f32 = 0.94;
const DRAG_MOMENTUM_STOP: f32 = 38.0;
const MOUSE_WHEEL_LINE_SCALE: f32 = 80.0;
const WHEEL_CHARGE_PER_NOTCH: f32 = 0.085;
const WHEEL_ACCEL_IDLE_DECAY: f32 = 0.86;
const WHEEL_ACCEL_SMOOTH_RATE: f32 = 14.0;
const WHEEL_STEP_SMOOTH_BLEND: f32 = 0.42;
const H_PAD: f32 = 6.0;

const TEXT_R: u8 = 230;
const TEXT_G: u8 = 236;
const TEXT_B: u8 = 240;

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
    wheel_accel_smooth: f32,
    /// When the user is scrolled to the bottom, new content can auto-snap there.
    pub stick_to_tail: bool,
    pending_snap_to_tail: bool,
    color_spans: Vec<ColorSpan>,
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
            wheel_accel_smooth: 0.0,
            stick_to_tail: true,
            pending_snap_to_tail: false,
            color_spans: Vec::new(),
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
    pub fn on_scroll(&mut self, dy: f32) {
        self.stick_to_tail = false;
        self.drag_scroll_momentum = 0.0;
        let scaled = if dy.abs() <= 3.0 {
            dy * MOUSE_WHEEL_LINE_SCALE
        } else {
            dy
        };
        self.wheel_accel_target = (self.wheel_accel_target + WHEEL_CHARGE_PER_NOTCH).min(1.0);
        let step = self.wheel_accel_target - self.wheel_accel_smooth;
        self.wheel_accel_smooth += step * WHEEL_STEP_SMOOTH_BLEND;
        self.wheel_accel_smooth = self.wheel_accel_smooth.clamp(0.0, 1.0);
        let mult = 1.0 + 2.0 * self.wheel_accel_smooth;
        self.scroll_target -= scaled * mult;
    }

    pub fn on_mouse_down(&mut self, y: f32) {
        self.dragging = true;
        self.last_mouse_y = y;
        self.last_drag_sample_time = None;
    }

    pub fn on_mouse_move_drag(&mut self, y: f32) {
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
        self.wheel_accel_smooth = 0.0;
        self.scroll_target -= dy;
        self.scroll_y = self.scroll_target;
        self.last_mouse_y = y;
        self.stick_to_tail = false;
    }

    pub fn on_mouse_up(&mut self) {
        self.dragging = false;
        self.last_drag_sample_time = None;
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging
    }

    /// `rect` is `(x0, y0, x1, y1)` in frame pixels. Clips drawing to that rectangle.
    pub fn tick(&mut self, state: &mut EngineState, rect: (f32, f32, f32, f32)) {
        let (rx0, ry0, rx1, ry1) = rect;
        let content_w = (rx1 - rx0 - H_PAD * 2.0).max(1.0);
        let visible_height = (ry1 - ry0).max(1.0);
        let content_top = ry0;
        let ox = rx0 + H_PAD;

        let dt = state.delta_time_seconds.clamp(1e-4, 0.1);

        self.wheel_accel_target *= WHEEL_ACCEL_IDLE_DECAY.powf(dt / SCROLL_REF_DT);
        self.wheel_accel_target = self.wheel_accel_target.clamp(0.0, 1.0);
        let wa_diff = self.wheel_accel_target - self.wheel_accel_smooth;
        if wa_diff.abs() > 1e-5 {
            let a = 1.0 - (-WHEEL_ACCEL_SMOOTH_RATE * dt).exp();
            self.wheel_accel_smooth += wa_diff * a;
        } else {
            self.wheel_accel_smooth = self.wheel_accel_target;
        }
        self.wheel_accel_smooth = self.wheel_accel_smooth.clamp(0.0, 1.0);

        if !self.dragging && self.drag_scroll_momentum.abs() > DRAG_MOMENTUM_STOP {
            self.scroll_target += self.drag_scroll_momentum * dt;
            self.drag_scroll_momentum *= DRAG_MOMENTUM_DECAY.powf(dt / SCROLL_REF_DT);
        } else if !self.dragging {
            self.drag_scroll_momentum = 0.0;
        }

        // Bounds from previous frame layout (same as `TextApp::tick` — one frame of lag on height).
        let lh0 = self.text_rasterizer.ascent
            + self.text_rasterizer.descent.abs()
            + self.text_rasterizer.line_gap;
        let text_content_height_prev = if !self.text_rasterizer.lines.is_empty() {
            let first_y = self
                .text_rasterizer
                .lines
                .first()
                .map(|l| l.baseline_y)
                .unwrap_or(0.0);
            let last_y = self
                .text_rasterizer
                .lines
                .last()
                .map(|l| l.baseline_y)
                .unwrap_or(0.0);
            (last_y - first_y).abs() + lh0 * 2.0
        } else {
            lh0
        };
        let natural_min = 0.0;
        let natural_max = (text_content_height_prev - visible_height).max(0.0);
        let overscroll_distance = visible_height * SCROLL_OVERSCROLL_LIMIT;
        let limit_min = natural_min - overscroll_distance;
        let limit_max = natural_max + overscroll_distance;

        if !self.dragging {
            if self.scroll_target < natural_min {
                let overshoot = natural_min - self.scroll_target;
                self.scroll_target += overshoot * SCROLL_ELASTIC_STRENGTH;
            } else if self.scroll_target > natural_max {
                let overshoot = self.scroll_target - natural_max;
                self.scroll_target -= overshoot * SCROLL_ELASTIC_STRENGTH;
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

        self.text_rasterizer.tick(content_w, visible_height);

        // New layout: snap to bottom for growing transcript, then re-clamp to new extents.
        let lh1 = self.text_rasterizer.ascent
            + self.text_rasterizer.descent.abs()
            + self.text_rasterizer.line_gap;
        let text_content_height = if !self.text_rasterizer.lines.is_empty() {
            let first_y = self
                .text_rasterizer
                .lines
                .first()
                .map(|l| l.baseline_y)
                .unwrap_or(0.0);
            let last_y = self
                .text_rasterizer
                .lines
                .last()
                .map(|l| l.baseline_y)
                .unwrap_or(0.0);
            (last_y - first_y).abs() + lh1 * 2.0
        } else {
            lh1
        };
        let natural_max = (text_content_height - visible_height).max(0.0);
        let overscroll_distance = visible_height * SCROLL_OVERSCROLL_LIMIT;
        let limit_min = natural_min - overscroll_distance;
        let limit_max = natural_max + overscroll_distance;

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

        if (natural_max - self.scroll_target).abs() < lh1 * 0.75 {
            self.stick_to_tail = true;
        }

        let shape = state.frame.shape();
        let width = shape[1] as f32;
        let height = shape[0] as f32;
        let w_i = width as i32;
        let h_i = height as i32;
        let buffer = state.frame_buffer_mut();
        let vis_top = self.scroll_y;
        let vis_bottom = self.scroll_y + visible_height;

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
    }
}
