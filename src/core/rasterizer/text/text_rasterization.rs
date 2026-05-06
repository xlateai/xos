use fontdue::{Font, Metrics};

use super::fonts;
use std::collections::HashMap;
use std::sync::Arc;

/// A single rendered glyph in pixel space.
#[derive(Debug)]
pub struct Character {
    pub ch: char,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub line_index: usize,
    pub char_index: usize,
    pub metrics: Metrics,
    /// Grayscale alpha bitmap; shared across identical `(char, font_size)` via [`GlyphCache`].
    pub bitmap: Arc<Vec<u8>>,
}

/// Caches fontdue raster output keyed by `(char, font_size_bits)` — avoids re-rasterizing every frame.
#[derive(Debug)]
pub struct GlyphCache {
    map: HashMap<(char, u32), (Metrics, Arc<Vec<u8>>)>,
    max_entries: usize,
}

impl GlyphCache {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            max_entries: 16_384,
        }
    }

    pub fn clear(&mut self) {
        self.map.clear();
    }

    fn get_or_insert(&mut self, font: &Font, ch: char, font_size: f32) -> (Metrics, Arc<Vec<u8>>) {
        let key = (ch, font_size.to_bits());
        if let Some((m, arc)) = self.map.get(&key) {
            return (*m, Arc::clone(arc));
        }
        if self.map.len() >= self.max_entries {
            self.map.clear();
        }
        let (metrics, bitmap) = font.rasterize(ch, font_size);
        let arc = Arc::new(bitmap);
        self.map.insert(key, (metrics, Arc::clone(&arc)));
        (metrics, arc)
    }
}

#[derive(Debug)]
pub struct LineInfo {
    pub baseline_y: f32,
    pub start_index: usize,
    pub end_index: usize,
}

/// Loose document-space vertical band around a wrapped line (matches [`TextApp::paint_viewport`] glyph culling).
#[inline]
pub fn line_doc_vertical_band(baseline_y: f32, ascent: f32, descent: f32, line_gap: f32, font_size: f32) -> (f32, f32) {
    let line_pad_y = line_gap * 0.5 + font_size * 0.15;
    let top = baseline_y - ascent - line_pad_y;
    let bottom = baseline_y + descent.abs() + line_pad_y;
    (top, bottom)
}

/// Whether a line’s vertical band can intersect the visible document slice `[scroll_y, scroll_y + visible_doc_height)`.
#[inline]
pub fn line_band_intersects_doc_viewport(
    baseline_y: f32,
    ascent: f32,
    descent: f32,
    line_gap: f32,
    font_size: f32,
    scroll_y: f32,
    visible_doc_height: f32,
) -> bool {
    let (lt, lb) = line_doc_vertical_band(baseline_y, ascent, descent, line_gap, font_size);
    let vis_bottom = scroll_y + visible_doc_height;
    !(lb < scroll_y || lt > vis_bottom)
}

/// Whether a laid-out glyph might appear inside the viewport (doc Y + layout width), before per-pixel clip.
/// `layout_w` is the text wrap width (widget inner width in layout space).
#[inline]
pub fn character_may_appear_in_viewport(
    character: &Character,
    layout_w: f32,
    scroll_y: f32,
    visible_doc_height: f32,
) -> bool {
    let vis_bottom = scroll_y + visible_doc_height;
    let g_top = character.y;
    let g_bottom = character.y + character.height;
    if g_bottom < scroll_y || g_top > vis_bottom {
        return false;
    }
    let slide_pad = character.width;
    let g_left = character.x - slide_pad;
    let g_right = character.x + character.metrics.advance_width + character.metrics.width as f32;
    !(g_right < 0.0 || g_left > layout_w)
}

/// Optional normalized alignment for [`TextRasterizer::tick_aligned`]:
/// `(0,0)=top-left`, `(0.5,0.5)=center`, `(1,1)=bottom-right`.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct TextLayoutAlign {
    /// Horizontal alignment within each wrapped line's free space (`0..=1`).
    pub x: f32,
    /// Vertical alignment within block free space (`0..=1`), supports bottom-origin typing behavior.
    pub y: f32,
}

impl TextLayoutAlign {
    #[inline]
    pub fn normalized(self) -> Self {
        Self {
            x: self.x.clamp(0.0, 1.0),
            y: self.y.clamp(0.0, 1.0),
        }
    }
}

pub struct TextRasterizer {
    pub text: String,
    pub characters: Vec<Character>,
    pub lines: Vec<LineInfo>,
    pub font_size: f32,
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
    pub font: Font,
    glyph_cache: GlyphCache,
    /// When fingerprint matches [`Self::tick`] inputs unchanged, reuse [`Self::characters`]/[`Self::lines`].
    last_layout_quick_fp: Option<u64>,
    /// After [`Self::tick_aligned`], X for a caret at the **start** of each line (empty lines, centered layout).
    pub line_caret_start_x: Vec<f32>,
    /// Multipliers for start-to-start spacing: `(x, y)` where `1.0` is default behavior.
    spacing: (f32, f32),
    /// Half-open char-index spans × relative font scale (`1.0` outside spans). Cleared by [`Self::set_text`].
    pub glyph_scale_spans: Vec<(usize, usize, f32)>,
}

impl TextRasterizer {
    pub fn new(font: Font, font_size: f32) -> Self {
        let metrics = font
            .horizontal_line_metrics(font_size)
            .expect("Font missing horizontal metrics");

        Self {
            text: String::new(),
            characters: vec![],
            lines: vec![],
            font_size,
            ascent: metrics.ascent,
            descent: metrics.descent.abs(),
            line_gap: metrics.line_gap,
            font,
            glyph_cache: GlyphCache::new(),
            last_layout_quick_fp: None,
            line_caret_start_x: Vec::new(),
            spacing: (1.0, 1.0),
            glyph_scale_spans: Vec::new(),
        }
    }

    pub fn set_text(&mut self, text: String) {
        // Normalize Windows-style CRLF to LF so '\r' doesn't render as a visible trailing glyph.
        self.invalidate_layout_cache();
        self.glyph_scale_spans.clear();
        self.text = text.replace("\r\n", "\n").replace('\r', "\n");
    }

    #[inline]
    pub fn invalidate_layout_cache(&mut self) {
        self.last_layout_quick_fp = None;
    }

    #[inline(always)]
    fn mix_fp(mut x: u64, y: u64) -> u64 {
        x ^= y.wrapping_mul(0x9e3779b97f4a7c15);
        x = x.rotate_left(29).wrapping_mul(0xc2b2ae35);
        x
    }

    /// Cheap stable fingerprint when the buffer is empty (no pointer/capacity noise).
    #[inline(always)]
    fn empty_layout_fp(wrap_width: f32, font_size: f32) -> u64 {
        let mut h = Self::mix_fp(0x811c_9dc5_0012_335b, 0);
        h = Self::mix_fp(h, font_size.to_bits() as u64);
        Self::mix_fp(h, wrap_width.to_bits() as u64)
    }

    /// O(min(n, 256)) fingerprints from **content only** — no pointer/capacity (allocations must not invalidate cache).
    /// Uses wrap width only (height does not affect line breaking — excluding it avoids relayout jitter on viewport resize).
    fn quick_layout_stable_fp(&self, wrap_width: f32) -> u64 {
        let bytes = self.text.as_bytes();
        let n = bytes.len();

        let mut h = Self::mix_fp(0xcbf29ce484222325, n as u64);
        h = Self::mix_fp(h, self.font_size.to_bits() as u64);
        h = Self::mix_fp(h, wrap_width.to_bits() as u64);

        const SAMPLES: usize = 256;
        if n <= SAMPLES {
            for &b in bytes.iter() {
                h = Self::mix_fp(h, b as u64);
            }
        } else {
            let denom = SAMPLES.saturating_sub(1).max(1);
            for i in 0..SAMPLES {
                let j = (i * n.saturating_sub(1)) / denom;
                h = Self::mix_fp(h, bytes[j] as u64);
            }
        }
        h
    }

    #[inline(always)]
    fn mix_scale_spans_fp(spans: &[(usize, usize, f32)]) -> u64 {
        let mut h = Self::mix_fp(0xd15ca5e_u64, spans.len() as u64);
        for (s, e, m) in spans {
            h = Self::mix_fp(h, *s as u64);
            h = Self::mix_fp(h, *e as u64);
            h = Self::mix_fp(h, m.to_bits() as u64);
        }
        h
    }

    #[inline]
    fn scale_at_char(spans: &[(usize, usize, f32)], char_index: usize) -> f32 {
        spans
            .iter()
            .rev()
            .find(|(s, e, _)| char_index >= *s && char_index < *e)
            .map(|(_, _, m)| *m)
            .unwrap_or(1.0)
            .clamp(0.125, 16.0)
    }

    /// Updates metrics for a new font size (call before [`tick`](Self::tick) to relayout).
    pub fn set_font_size(&mut self, font_size: f32) {
        if (self.font_size - font_size).abs() < 0.02 {
            return;
        }
        self.last_layout_quick_fp = None;
        self.font_size = font_size;
        self.glyph_cache.clear();
        let metrics = self
            .font
            .horizontal_line_metrics(font_size)
            .expect("Font missing horizontal metrics");
        self.ascent = metrics.ascent;
        self.descent = metrics.descent.abs();
        self.line_gap = metrics.line_gap;
    }

    /// Set spacing multipliers for character/line start distances (`(1,1)` = default).
    pub fn set_spacing(&mut self, spacing_x: f32, spacing_y: f32) {
        let sx = spacing_x.max(0.0);
        let sy = spacing_y.max(0.0);
        if (self.spacing.0 - sx).abs() < 1e-4 && (self.spacing.1 - sy).abs() < 1e-4 {
            return;
        }
        self.spacing = (sx, sy);
        self.last_layout_quick_fp = None;
    }

    #[inline]
    pub fn spacing(&self) -> (f32, f32) {
        self.spacing
    }

    #[inline]
    pub fn advance_with_spacing(&self, advance_width: f32) -> f32 {
        advance_width * self.spacing.0.max(0.0)
    }

    pub fn tick(&mut self, window_width: f32, window_height: f32) {
        self.tick_aligned(window_width, window_height, TextLayoutAlign::default());
    }

    #[inline]
    fn mix_align_into_fp(
        base_fp: u64,
        window_height: f32,
        align: TextLayoutAlign,
        spacing: (f32, f32),
    ) -> u64 {
        let mut h = Self::mix_fp(base_fp, window_height.to_bits() as u64);
        h = Self::mix_fp(h, align.x.to_bits() as u64);
        h = Self::mix_fp(h, align.y.to_bits() as u64);
        h = Self::mix_fp(h, spacing.0.to_bits() as u64);
        h = Self::mix_fp(h, spacing.1.to_bits() as u64);
        h
    }

    /// Word-wrap in `window_width`, optional per-line horizontal centering and vertical centering
    /// when the laid-out block is shorter than `window_height`.
    pub fn tick_aligned(&mut self, window_width: f32, window_height: f32, align: TextLayoutAlign) {
        // Callers often assign `text` directly (e.g. coder); normalize CRLF here too so `\r`
        // never renders as a trailing glyph on Windows-sourced files.
        if self.text.contains('\r') {
            self.text = self.text.replace("\r\n", "\n").replace('\r', "\n");
            self.last_layout_quick_fp = None;
        }

        let align = align.normalized();
        let base_fp = if self.text.is_empty() {
            Self::empty_layout_fp(window_width, self.font_size)
        } else {
            self.quick_layout_stable_fp(window_width)
        };
        let fp = Self::mix_fp(
            Self::mix_align_into_fp(base_fp, window_height, align, self.spacing),
            Self::mix_scale_spans_fp(&self.glyph_scale_spans),
        );
        if self.last_layout_quick_fp == Some(fp) {
            return;
        }

        self.characters.clear();
        self.lines.clear();

        let font = &self.font;
        let fs = self.font_size;
        let sx = self.spacing.0.max(0.0);
        let sy = self.spacing.1.max(0.0);
        let default_line_stride = (self.ascent + self.descent + self.line_gap) * sy;
        let scales = &self.glyph_scale_spans;
        let body_line_gap = self.line_gap;
        let chars: Vec<char> = self.text.chars().collect();

        /// One wrapped / newline-delimited row: glyph index range plus ink extents vs baseline (`y` untouched).
        #[derive(Clone, Copy)]
        struct ProtoLine {
            start: usize,
            end_excl: usize,
            ascent: f32,
            descent: f32,
            max_scale: f32,
        }

        // Unscaled gap from the font at this line's em-size; multiply the full stride by `sy` below so Y spacing
        // matches X (where `advance * sx` scales the entire advance), not just the metric `line_gap` sliver.
        let line_gap_unscaled = |max_scale_on_line: f32| -> f32 {
            let px = (fs * max_scale_on_line).max(0.5);
            font
                .horizontal_line_metrics(px)
                .map(|m| m.line_gap)
                .unwrap_or(body_line_gap)
        };

        let mut proto_lines: Vec<ProtoLine> = Vec::new();
        let mut line_start = 0usize;
        let mut x = 0.0f32;
        let mut line_ascent = 0.0f32;
        let mut line_descent = 0.0f32;
        let mut line_max_scale = 1.0f32;

        if chars.is_empty() {
            proto_lines.push(ProtoLine {
                start: 0,
                end_excl: 0,
                ascent: 0.0,
                descent: 0.0,
                max_scale: 1.0,
            });
        } else {
            let mut i = 0usize;
            while i < chars.len() {
                let ch = chars[i];
                if ch == '\n' {
                    proto_lines.push(ProtoLine {
                        start: line_start,
                        end_excl: i,
                        ascent: line_ascent,
                        descent: line_descent,
                        max_scale: line_max_scale,
                    });
                    line_start = i + 1;
                    line_ascent = 0.0;
                    line_descent = 0.0;
                    line_max_scale = 1.0;
                    x = 0.0;
                    i += 1;
                    continue;
                }

                let scale_i = Self::scale_at_char(scales, i);
                let fs_i = fs * scale_i;
                let (metrics, _) = self.glyph_cache.get_or_insert(font, ch, fs_i);
                let advance_step = metrics.advance_width * sx;

                if x + advance_step > window_width {
                    proto_lines.push(ProtoLine {
                        start: line_start,
                        end_excl: i,
                        ascent: line_ascent,
                        descent: line_descent,
                        max_scale: line_max_scale,
                    });
                    line_start = i;
                    line_ascent = 0.0;
                    line_descent = 0.0;
                    line_max_scale = 1.0;
                    x = 0.0;
                }

                let ink_above = metrics.height as f32 + metrics.ymin as f32;
                let ink_below = (-metrics.ymin as f32).max(0.0);
                line_ascent = line_ascent.max(ink_above);
                line_descent = line_descent.max(ink_below);
                line_max_scale = line_max_scale.max(scale_i);

                x += advance_step;
                i += 1;
            }

            proto_lines.push(ProtoLine {
                start: line_start,
                end_excl: chars.len(),
                ascent: line_ascent,
                descent: line_descent,
                max_scale: line_max_scale,
            });
        }

        let n_proto = proto_lines.len().max(1);
        let mut baselines = vec![0.0f32; n_proto];
        baselines[0] = self.ascent;

        const Z: f32 = 1e-6;
        for k in 0..proto_lines.len().saturating_sub(1) {
            let prev = proto_lines[k];
            let next = proto_lines[k + 1];
            let prev_empty = prev.ascent <= Z && prev.descent <= Z;
            let next_empty = next.ascent <= Z && next.descent <= Z;

            let mut step = if prev_empty {
                default_line_stride
            } else {
                let gap_u = line_gap_unscaled(prev.max_scale);
                (prev.descent + gap_u + next.ascent) * sy
            };

            if next_empty {
                step = step.max(default_line_stride);
            }

            baselines[k + 1] = baselines[k] + step;
        }

        self.lines.reserve(proto_lines.len());
        for (li, proto) in proto_lines.iter().enumerate() {
            let baseline_y = baselines[li];
            let line_index = li;
            self.lines.push(LineInfo {
                baseline_y,
                start_index: proto.start,
                end_index: proto.end_excl,
            });

            let mut line_x = 0.0f32;
            for i in proto.start..proto.end_excl {
                let ch = chars[i];
                let scale_i = Self::scale_at_char(scales, i);
                let fs_i = fs * scale_i;
                let (metrics, bitmap) = self.glyph_cache.get_or_insert(font, ch, fs_i);
                let advance_step = metrics.advance_width * sx;

                let y = baseline_y - metrics.height as f32 - metrics.ymin as f32;
                self.characters.push(Character {
                    ch,
                    x: line_x,
                    y,
                    width: metrics.width as f32,
                    height: metrics.height as f32,
                    line_index,
                    char_index: i,
                    metrics,
                    bitmap,
                });

                line_x += advance_step;
            }
        }

        // --- Horizontal centering (per wrapped line) ---
        // Important: compute centering from the *actual rendered glyph bounds*,
        // not trailing pen advance. With spacing_x > 1.0, using trailing advance
        // introduces an extra right-side phantom gap and visually shifts text left.
        if align.x > 0.0 {
            let n_lines = self.lines.len().max(1);
            let mut line_dx = vec![0.0f32; n_lines];
            for li in 0..self.lines.len() {
                let mut line_left = f32::INFINITY;
                let mut line_right = f32::NEG_INFINITY;
                let mut any = false;
                for c in &self.characters {
                    if c.line_index == li {
                        any = true;
                        line_left = line_left.min(c.x);
                        line_right = line_right.max(c.x + c.width);
                    }
                }
                line_dx[li] = if any {
                    let line_w = (line_right - line_left).max(0.0);
                    ((window_width - line_w).max(0.0)) * align.x - line_left
                } else {
                    window_width * align.x
                };
            }
            for c in &mut self.characters {
                let li = c.line_index;
                if li < line_dx.len() {
                    c.x += line_dx[li];
                }
            }
        }

        // --- Vertical centering when content is shorter than the viewport ---
        if align.y > 0.0 {
            let mut min_y = f32::INFINITY;
            let mut max_y = f32::NEG_INFINITY;
            for c in &self.characters {
                min_y = min_y.min(c.y);
                max_y = max_y.max(c.y + c.height);
            }
            if self.characters.is_empty() {
                if let Some(ln) = self.lines.first() {
                    let top = ln.baseline_y - self.ascent;
                    let bottom = ln.baseline_y + self.descent;
                    min_y = top;
                    max_y = bottom;
                } else {
                    min_y = 0.0;
                    max_y = self.ascent + self.descent;
                }
            }
            let content_h = (max_y - min_y).max(1.0);
            let target_top = (window_height - content_h) * align.y;
            let shift_y = target_top - min_y;
            if shift_y.abs() > 1e-4 {
                for c in &mut self.characters {
                    c.y += shift_y;
                }
                for ln in &mut self.lines {
                    ln.baseline_y += shift_y;
                }
            }
        }

        // Caret X at logical line start (handles empty lines + horizontal centering).
        self.line_caret_start_x.clear();
        for li in 0..self.lines.len() {
            let mut min_x = f32::INFINITY;
            let mut any = false;
            for c in &self.characters {
                if c.line_index == li {
                    any = true;
                    min_x = min_x.min(c.x);
                }
            }
            let cx = if any {
                min_x
            } else if align.x > 0.0 {
                window_width * align.x
            } else {
                0.0
            };
            self.line_caret_start_x.push(cx);
        }

        self.last_layout_quick_fp = Some(fp);
    }

    #[inline]
    pub fn line_leading_caret_x(&self, line_idx: usize) -> f32 {
        self.line_caret_start_x.get(line_idx).copied().unwrap_or(0.0)
    }

    /// When the engine default font family changes (e.g. F3 menu), replace [`Self::font`] and line metrics.
    /// `seen_version` is the caller's watermark from [`fonts::default_font_version`]; advance it when swapping.
    pub fn sync_default_font_family_from_engine(&mut self, seen_version: &mut u64) {
        let ver = fonts::default_font_version();
        if ver == *seen_version {
            return;
        }
        *seen_version = ver;
        self.font = fonts::default_font();
        self.glyph_cache.clear();
        self.last_layout_quick_fp = None;

        let fs = self.font_size;
        let metrics = self
            .font
            .horizontal_line_metrics(fs)
            .expect("Font missing horizontal metrics");
        self.ascent = metrics.ascent;
        self.descent = metrics.descent.abs();
        self.line_gap = metrics.line_gap;
    }
}

