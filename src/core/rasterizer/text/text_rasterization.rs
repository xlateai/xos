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
        }
    }

    pub fn set_text(&mut self, text: String) {
        // Normalize Windows-style CRLF to LF so '\r' doesn't render as a visible trailing glyph.
        self.last_layout_quick_fp = None;
        self.text = text.replace("\r\n", "\n").replace('\r', "\n");
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

    pub fn tick(&mut self, window_width: f32, _window_height: f32) {
        // Callers often assign `text` directly (e.g. coder); normalize CRLF here too so `\r`
        // never renders as a trailing glyph on Windows-sourced files.
        if self.text.contains('\r') {
            self.text = self.text.replace("\r\n", "\n").replace('\r', "\n");
            self.last_layout_quick_fp = None;
        }

        let fp = if self.text.is_empty() {
            Self::empty_layout_fp(window_width, self.font_size)
        } else {
            self.quick_layout_stable_fp(window_width)
        };
        if self.last_layout_quick_fp == Some(fp) {
            return;
        }

        self.characters.clear();
        self.lines.clear();

        let mut x = 0.0;
        let mut baseline_y = self.ascent;
        let mut line_start = 0;
        let mut line_index = 0;

        let mut last_index = 0;
        let font = &self.font;
        let fs = self.font_size;

        for (i, ch) in self.text.chars().enumerate() {
            if ch == '\n' {
                self.lines.push(LineInfo {
                    baseline_y,
                    start_index: line_start,
                    end_index: i,
                });

                x = 0.0;
                baseline_y += self.ascent + self.descent + self.line_gap;
                line_start = i + 1;
                line_index += 1;
                continue;
            }

            let (metrics, bitmap) = self.glyph_cache.get_or_insert(font, ch, fs);
            let advance = metrics.advance_width;

            if x + advance > window_width {
                self.lines.push(LineInfo {
                    baseline_y,
                    start_index: line_start,
                    end_index: i,
                });

                x = 0.0;
                baseline_y += self.ascent + self.descent + self.line_gap;
                line_start = i;
                line_index += 1;
            }

            let y = baseline_y - metrics.height as f32 - metrics.ymin as f32;

            self.characters.push(Character {
                ch,
                x,
                y,
                width: metrics.width as f32,
                height: metrics.height as f32,
                line_index,
                char_index: i,
                metrics,
                bitmap,
            });

            x += advance;
            last_index = i;
        }

        self.lines.push(LineInfo {
            baseline_y,
            start_index: line_start,
            end_index: last_index + 1,
        });

        self.last_layout_quick_fp = Some(fp);
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

