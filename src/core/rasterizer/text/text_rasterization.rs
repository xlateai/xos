use fontdue::{Font, Metrics};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// Snap effective raster sizes (~1/32 px) so glyph cache keys survive f32 jitter
/// (`font_size_px * viewport_scale`, Python floats, caret toggles leaking into scale, …).
///
/// Rasterization and advancement use this same bucket so layouts stay consistent per key.
#[inline]
pub(crate) fn quantize_viewport_raster_px(fs: f32) -> f32 {
    if !fs.is_finite() || fs <= 0.0 {
        return 1.0;
    }
    let fs = fs.min(768.0);
    (fs * 32.0).round() / 32.0
}

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
    /// Grayscale alpha bitmap; shared across identical (char, font_size) via [`GlyphCache`].
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
            max_entries: 56_832,
        }
    }

    pub fn clear(&mut self) {
        self.map.clear();
    }

    fn get_or_insert(&mut self, font: &Font, ch: char, font_size: f32) -> (Metrics, Arc<Vec<u8>>) {
        let fs_q = quantize_viewport_raster_px(font_size.max(1.0)).max(1.0);
        let key = (ch, fs_q.to_bits());
        if let Some((m, arc)) = self.map.get(&key) {
            return (*m, Arc::clone(arc));
        }
        if self.map.len() >= self.max_entries {
            self.map.clear();
        }
        let (metrics, bitmap) = font.rasterize(ch, fs_q);
        let arc = Arc::new(bitmap);
        self.map.insert(key, (metrics, Arc::clone(&arc)));
        (metrics, arc)
    }
}

/// Process-wide raster cache for the **default UI viewport font** (`fonts::default_font()`).
///
/// Keys include [`super::fonts::FontFamily`] so F3 font swaps never alias bitmaps. Callers should hold
/// [`ViewportGlyphCacheSession`] once per layout pass (rich text, `TextRasterizer::tick`) so iOS does not
/// pay a mutex lock per glyph and so the cache is shared across worker/main threads (unlike `thread_local`).
struct GlobalViewportGlyphScratch {
    font_version: u64,
    font_family: u8,
    map: HashMap<(u8, char, u32), (Metrics, Arc<Vec<u8>>)>,
}

const GLOBAL_VIEWPORT_GLYPH_MAX: usize = 262_144;
/// When the table is full, drop a batch of entries instead of clearing (which caused constant JP re-raster).
const VIEWPORT_GLYPH_EVICT_BATCH: usize = 48_576;

impl GlobalViewportGlyphScratch {
    fn new() -> Self {
        Self {
            font_version: super::fonts::default_font_version(),
            font_family: super::fonts::default_font_family() as u8,
            map: HashMap::new(),
        }
    }

    fn sync_identity(&mut self) {
        let v = super::fonts::default_font_version();
        let fam = super::fonts::default_font_family() as u8;
        if self.font_version != v || self.font_family != fam {
            self.font_version = v;
            self.font_family = fam;
            self.map.clear();
        }
    }

    fn evict_if_full(&mut self) {
        if self.map.len() < GLOBAL_VIEWPORT_GLYPH_MAX {
            return;
        }
        let n = VIEWPORT_GLYPH_EVICT_BATCH.min(self.map.len());
        let keys: Vec<(u8, char, u32)> = self.map.keys().take(n).cloned().collect();
        for k in keys {
            self.map.remove(&k);
        }
    }

    fn lookup_or_rasterize(&mut self, font: &Font, ch: char, font_size: f32) -> (Metrics, Arc<Vec<u8>>) {
        let fs = quantize_viewport_raster_px(font_size).max(1.0).min(768.0);
        let key = (self.font_family, ch, fs.to_bits());
        if let Some((m, arc)) = self.map.get(&key) {
            return (*m, Arc::clone(arc));
        }
        self.evict_if_full();
        let (metrics, bitmap) = font.rasterize(ch, fs);
        let arc = Arc::new(bitmap);
        self.map.insert(key, (metrics, Arc::clone(&arc)));
        (metrics, arc)
    }
}

static VIEWPORT_GLYPH_GLOBAL: OnceLock<Mutex<GlobalViewportGlyphScratch>> = OnceLock::new();

/// One lock per rich-text layout or viewport `TextRasterizer::tick` — **not** per glyph (critical on iOS).
pub(crate) struct ViewportGlyphCacheSession {
    scratch: std::sync::MutexGuard<'static, GlobalViewportGlyphScratch>,
}

impl ViewportGlyphCacheSession {
    #[inline]
    pub(crate) fn cached_raster(&mut self, font: &Font, ch: char, font_size: f32) -> (Metrics, Arc<Vec<u8>>) {
        self.scratch.lookup_or_rasterize(font, ch, font_size)
    }
}

#[inline]
pub(crate) fn viewport_glyph_cache_session() -> ViewportGlyphCacheSession {
    let m = VIEWPORT_GLYPH_GLOBAL.get_or_init(|| Mutex::new(GlobalViewportGlyphScratch::new()));
    let mut scratch = m.lock().expect("viewport glyph cache mutex poisoned");
    scratch.sync_identity();
    ViewportGlyphCacheSession { scratch }
}

#[derive(Debug)]
pub struct LineInfo {
    pub baseline_y: f32,
    pub start_index: usize,
    pub end_index: usize,
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
    /// Fast path for Python/UI viewport text tied to [`super::fonts::default_font`] only.
    use_global_viewport_glyph_cache: bool,
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
            use_global_viewport_glyph_cache: false,
        }
    }

    /// Like [`Self::new`], but reuses the process-wide viewport glyph session (see [`viewport_glyph_cache_session`])
    /// so CJK-heavy UIs avoid re-rasterizing the same glyphs every frame. Intended only with
    /// [`super::fonts::default_font`] (viewport / Study).
    pub fn new_viewport_global_glyph_cache(font: Font, font_size: f32) -> Self {
        let mut r = Self::new(font, font_size);
        r.use_global_viewport_glyph_cache = true;
        r
    }

    pub fn set_text(&mut self, text: String) {
        // Normalize Windows-style CRLF to LF so '\r' doesn't render as a visible trailing glyph.
        self.text = text.replace("\r\n", "\n").replace('\r', "\n");
    }

    /// Updates metrics for a new font size (call before [`tick`](Self::tick) to relayout).
    pub fn set_font_size(&mut self, font_size: f32) {
        if (self.font_size - font_size).abs() < 0.02 {
            return;
        }
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

    /// Swap the active font (same pixel size). Clears glyph cache and refreshes line metrics.
    pub fn set_font(&mut self, font: Font) {
        self.font = font;
        self.glyph_cache.clear();
        let metrics = self
            .font
            .horizontal_line_metrics(self.font_size)
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

        let mut viewport_glyphs = if self.use_global_viewport_glyph_cache {
            Some(viewport_glyph_cache_session())
        } else {
            None
        };

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

            let (metrics, bitmap) = if let Some(ref mut sess) = viewport_glyphs {
                sess.cached_raster(font, ch, fs)
            } else {
                self.glyph_cache.get_or_insert(font, ch, fs)
            };
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
    }
}

/// When the global default font changes (F3 menu), apply it to this rasterizer.
/// `last_version` must track [`super::fonts::default_font_version`] for this rasterizer.
pub fn sync_rasterizer_to_default_font(r: &mut TextRasterizer, last_version: &mut u64) -> bool {
    let v = super::fonts::default_font_version();
    if v == *last_version {
        return false;
    }
    *last_version = v;
    r.set_font(super::fonts::default_font());
    true
}


