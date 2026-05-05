//! [`TextApp`] — scrollable/editor shell used by standalone panels, [`xos.ui.Text`], Coder buffers, meshes, ….
//! The interactive **`text`** binary target (`xos app text`) runs Python from [`super::launcher`] + `text.py`, not [`Application`] for [`TextApp`] directly.
//!
//! To experiment with legacy Rust-as-app behavior locally, instantiate [`TextApp`] in a scratch `main` / test harness.
use crate::engine::{Application, EngineState, ScrollWheelUnit};
use crate::rasterizer::{fill, fill_rect_buffer};
use crate::rasterizer::text::fonts;
use crate::rasterizer::text::text_rasterization::{
    line_band_intersects_doc_viewport, TextLayoutAlign, TextRasterizer,
};
use crate::ui::text as ui_text_edit;
use crate::ui::onscreen_keyboard::KeyType;
use crate::clipboard;
use crate::engine::keyboard::shortcuts::ShortcutAction;
use std::time::{Instant, Duration};

const BACKGROUND_COLOR: (u8, u8, u8) = (0, 0, 0);
const TEXT_COLOR: (u8, u8, u8) = (255, 255, 255);
const CURSOR_COLOR: (u8, u8, u8) = (0, 255, 0);
const BOUND_COLOR: (u8, u8, u8) = (255, 0, 0);
const BASELINE_COLOR: (u8, u8, u8) = (100, 100, 100);
const SELECTION_COLOR: (u8, u8, u8, u8) = (50, 120, 200, 128); // Semi-transparent blue

const SHOW_BOUNDING_RECTANGLES: bool = true;
const DRAW_BASELINES: bool = true;
const DOUBLE_TAP_TIME_MS: u64 = 300; // 300ms window for double tap
const DOUBLE_TAP_DISTANCE: f32 = 50.0; // Maximum distance between taps in pixels

// Scroll: `scroll_target` is authoritative; wheel updates it immediately on input (event-driven).
// `scroll_y` eases toward target each tick (`1 - exp(-rate*dt)`). Wheel stacks acceleration toward 3×
// (target charged per event, smooth toward target per tick), idle decay — no target coast. Clears `drag_scroll_momentum`.
// Click-drag: `drag_scroll_momentum` coasts after release; wheel streak clears when drag-scroll starts.
const SCROLL_REF_DT: f32 = 1.0 / 60.0;
/// Edge spring for [`TextApp::scroll_target`]: pull fraction `1 - exp(-rate * dt)` of overshoot toward bounds.
const SCROLL_ELASTIC_RATE: f32 = 28.0;
const SCROLL_OVERSCROLL_LIMIT: f32 = 0.25; // How far off-screen you can scroll (0.25 = 25% of visible height on each side)
/// Higher = snappier catch-up to [`TextApp::scroll_target`] (wall-clock rate 1/s). Uses `1 - exp(-rate * dt)`.
const SCROLL_SMOOTH_RATE: f32 = 40.0;
/// Treat scroll animation as settled for double-tap if |target − y| is below this (px).
const SCROLL_SETTLE_FOR_TAP: f32 = 12.0;
/// After finger release, coast `scroll_target` with this decay per [`SCROLL_REF_DT`].
const DRAG_MOMENTUM_DECAY: f32 = 0.94;
/// Stop drag coast when |velocity| falls below this (px/s).
const DRAG_MOMENTUM_STOP: f32 = 38.0;
/// Double-tap keyboard only if drag coast speed is below this (px/s).
const DRAG_MOMENTUM_SETTLE_FOR_TAP: f32 = 130.0;

/// Mouse wheel [`MouseScrollDelta::LineDelta`] is typically ±1 per notch; scale to ~pixels (~+45% vs legacy).
/// Standalone text only: 30% smaller than the prior default while the F3 slider stays at 50% = 1.0× engine coeff.
const TEXT_STANDALONE_SIZE_FACTOR: f32 = 0.7;

/// [`ScrollWheelUnit::Line`]: discrete wheel steps scaled to approximate pixels.
const MOUSE_WHEEL_LINE_SCALE: f32 = 240.0;
/// [`ScrollWheelUnit::Pixel`]: multiply OS logical pixel deltas (normally 1.0 — tune if needed).
const TRACKPAD_SCROLL_PIXEL_SCALE: f32 = 1.0;
/// Per wheel event: adds to [`TextApp::wheel_accel_target`] (0..=1). Sustained scrolling stacks toward 3×.
const WHEEL_CHARGE_PER_NOTCH: f32 = 0.085;
/// Idle decay of wheel streak per [`SCROLL_REF_DT`] (applied only after a quiet period — see [`TextApp::wheel_last_activity`]).
const WHEEL_ACCEL_IDLE_DECAY: f32 = 0.86;
/// Ignore wheel notch streak decay briefly after the last [`TextApp::on_scroll`] so FPS does not bleed the stacker.
const WHEEL_STREAK_HOLD: Duration = Duration::from_millis(72);

// Arrow key characters (using Unicode arrow symbols)
const ARROW_LEFT: char = '\u{2190}';  // ←
const ARROW_RIGHT: char = '\u{2192}'; // →
const ARROW_UP: char = '\u{2191}';    // ↑
const ARROW_DOWN: char = '\u{2193}';  // ↓

use std::collections::HashMap;

#[derive(Clone, Copy)]
pub(crate) struct TextViewportMetrics {
    frame_w: f32,
    frame_h: f32,
    pub(crate) layout_w: f32,
    pub(crate) visible_h: f32,
    pub(crate) draw_x: f32,
    pub(crate) draw_y: f32,
    pub(crate) embed: bool,
}

/// Immutable clip/layout rectangle for [`TextApp::paint_viewport`] (mirrors standalone tick paint math).
#[derive(Clone, Copy)]
pub(crate) struct ViewportPaintCtx {
    pub(crate) fw: usize,
    pub(crate) fh: usize,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) w_i: i32,
    pub(crate) h_i: i32,
    pub(crate) clip_left: i32,
    pub(crate) clip_top: i32,
    pub(crate) clip_right: i32,
    pub(crate) clip_bottom: i32,
    pub(crate) draw_off_x: f32,
    pub(crate) draw_off_y: f32,
    pub(crate) layout_w: f32,
    pub(crate) visible_height: f32,
    pub(crate) content_top: f32,
    pub(crate) is_trackpad_mode: bool,
    pub(crate) is_keyboard_shown: bool,
    /// Caret overlay (standalone uses [`TextApp::show_cursor`]; Python may override per `_text_render`).
    pub(crate) paint_cursor: bool,
}

pub struct TextApp {
    pub text_rasterizer: TextRasterizer,
    /// Smoothed scroll offset used for drawing (eases toward [`Self::scroll_target`]).
    pub scroll_y: f32,
    /// Desired scroll offset (wheel / drag / auto-scroll). Reset both when snapping from outside (e.g. coder).
    pub scroll_target: f32,
    pub smooth_cursor_x: f32,
    pub fade_map: HashMap<(char, u32, u32), f32>,
    last_tap_time: Option<Instant>,
    last_tap_x: f32,
    last_tap_y: f32,
    last_tap_scrolled: bool, // Track if user scrolled between taps
    pub cursor_position: usize, // Character index where cursor should be
    dragging: bool,
    last_mouse_y: f32,
    touch_started_on_keyboard: bool,
    // Cursor positioning on release
    pending_cursor_tap_x: Option<f32>,
    pending_cursor_tap_y: Option<f32>,
    /// [`TextApp::scroll_target`] at mouse down — detect scroll between tap and release.
    initial_scroll_target: f32,
    /// Finger-derived scroll speed (px/s) while drag-scrolling; after release, coasts `scroll_target`.
    drag_scroll_momentum: f32,
    last_drag_sample_time: Option<Instant>,
    /// 0..=1 — wheel speed stack toward 3×; decays when idle (no coast on `scroll_target`).
    wheel_accel_target: f32,
    /// Last wheel event (for idle-only streak decay).
    wheel_last_activity: Option<Instant>,
    // Trackpad mode tracking
    trackpad_active: bool,
    trackpad_last_tap_time: Option<Instant>,
    trackpad_selecting: bool,
    trackpad_moved: bool, // Track if mouse moved during tap (to distinguish tap from drag)
    // Temp trackpad activation tracking (for Shift/SymbolToggle drag)
    temp_trackpad_initial_x: Option<f32>,
    temp_trackpad_initial_y: Option<f32>,
    // Trackpad laser pointer
    trackpad_laser_x: Option<f32>, // Screen coordinates
    trackpad_laser_y: Option<f32>, // Screen coordinates
    trackpad_last_mouse_x: Option<f32>,
    trackpad_last_mouse_y: Option<f32>,
    // Clipboard
    clipboard_content: String,
    // Undo/redo history
    undo_stack: Vec<(String, usize)>, // (text, cursor_position)
    redo_stack: Vec<(String, usize)>,
    // Text selection state
    selection_start: Option<usize>, // Character index where selection starts
    selection_end: Option<usize>,   // Character index where selection ends
    selecting: bool,                // True when actively selecting text (dragging)
    // Configuration flags
    pub show_cursor: bool,
    pub show_debug_visuals: bool,
    pub read_only: bool,
    /// Last wrap width / font size used for glyph fade keys — when these change, reflow invalidates (x,y) keys.
    last_fade_wrap_width: f32,
    last_fade_font_size: f32,
    /// When true, clear the frame to transparent instead of [`BACKGROUND_COLOR`] (for overlay windows).
    pub transparent_background: bool,
    /// Color for debug glyph bounding rectangles ([`SHOW_BOUNDING_RECTANGLES`]).
    pub bound_color: (u8, u8, u8),
    /// Last font size applied from global UI scale (standalone text only); avoids redundant `set_font_size`.
    last_engine_scaled_font: f32,
    /// When true (e.g. [`CoderApp`] editors), skip global scale here — parent sets [`TextApp::set_font_size`].
    pub uses_parent_ui_scale: bool,
    /// Pixels to subtract from the keyboard top line so text ends above parent chrome (e.g. coder task bar).
    pub bottom_chrome_height_px: f32,
    /// Pixels below the safe region top reserved for parent chrome (e.g. coder editor tab bar).
    pub top_chrome_height_px: f32,
    /// When set (`Some((x_px, y_px, width, height))`), layouts and paints into a framebuffer sub-rectangle (Python `xos.ui.Text`).
    pub python_viewport: Option<(i32, i32, u32, u32)>,
    /// Normalized `[0,1]²` rect (`x1`,`y1`,`x2`,`y2`) for Python embed; [`Self::python_viewport`] is rebuilt each [`tick`] via [`UiText`] rounding.
    pub python_viewport_norm: Option<(f32, f32, f32, f32)>,
    pub py_scrollable: bool,
    pub py_selectable: bool,
    pub py_allow_shortcuts: bool,
    pub py_allow_copypaste: bool,
    /// When true, [`TextApp::tick`] updates layout/scroll/OSK ingestion but skips painting — Python uses [`xos.ui._text_render`].
    pub embed_skip_frame_present: bool,
    /// Python `Text` widgets use engine default fonts only (`font=None`); follow F3 default family changes.
    pub(crate) follow_engine_default_font: bool,
    /// Watermark paired with [`TextRasterizer::sync_default_font_family_from_engine`] for [`Self::follow_engine_default_font`].
    pub(crate) engine_font_family_version_seen: u64,
    /// Python embedded full-screen text: skip slide-in + per-glyph [`HashMap`] fade (big win for large fonts / long docs).
    pub(crate) embed_fast_glyph_paint: bool,
    /// Per-frame reuse for [`Self::paint_viewport`] line-vs-viewport overlap (avoids `Vec::collect` each paint).
    paint_line_visible_scratch: Vec<bool>,
    /// Python embedded: synced from [`xos.ui.Text.is_focused`] each [`tick`]; gates character keys, shortcuts, and pointer hit-testing.
    pub py_input_focused: bool,
    /// Python `xos.ui.Text` normalized alignment (`0,0` top-left; `1,1` bottom-right).
    pub py_alignment: (f32, f32),
}


impl TextApp {
    pub fn new() -> Self {
        let font = fonts::default_font();

        // Increase font size by 10% on iOS (~50% smaller than legacy 48px default for perf at long docs)
        let base_font_size = 24.0;
        let font_size = if cfg!(target_os = "ios") {
            base_font_size * 1.1 // 10% larger on iOS
        } else {
            base_font_size
        };

        let mut text_rasterizer = TextRasterizer::new(font, font_size);

        // Set default text on iOS
        let initial_cursor_pos = if cfg!(target_os = "ios") {
            let default_text = "double tap screen to open keyboard".to_string();
            let cursor_pos = default_text.chars().count();
            text_rasterizer.set_text(default_text);
            cursor_pos
        } else {
            0
        };

        Self {
            text_rasterizer,
            scroll_y: 0.0, // Always start at 0 (top of safe region)
            scroll_target: 0.0,
            smooth_cursor_x: 0.0,
            fade_map: HashMap::new(),
            last_tap_time: None,
            last_tap_x: 0.0,
            last_tap_y: 0.0,
            last_tap_scrolled: false,
            cursor_position: initial_cursor_pos,
            dragging: false,
            last_mouse_y: 0.0,
            touch_started_on_keyboard: false,
            pending_cursor_tap_x: None,
            pending_cursor_tap_y: None,
            initial_scroll_target: 0.0,
            drag_scroll_momentum: 0.0,
            last_drag_sample_time: None,
            wheel_accel_target: 0.0,
            wheel_last_activity: None,
            trackpad_active: false,
            trackpad_last_tap_time: None,
            trackpad_selecting: false,
            trackpad_moved: false,
            temp_trackpad_initial_x: None,
            temp_trackpad_initial_y: None,
            trackpad_laser_x: None,
            trackpad_laser_y: None,
            trackpad_last_mouse_x: None,
            trackpad_last_mouse_y: None,
            clipboard_content: String::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            selection_start: None,
            selection_end: None,
            selecting: false,
            show_cursor: true,
            show_debug_visuals: true,
            read_only: false,
            last_fade_wrap_width: -1.0,
            last_fade_font_size: -1.0,
            transparent_background: false,
            bound_color: BOUND_COLOR,
            last_engine_scaled_font: -1.0,
            uses_parent_ui_scale: false,
            bottom_chrome_height_px: 0.0,
            top_chrome_height_px: 0.0,
            python_viewport: None,
            python_viewport_norm: None,
            py_scrollable: true,
            py_selectable: true,
            py_allow_shortcuts: true,
            py_allow_copypaste: true,
            embed_skip_frame_present: false,
            follow_engine_default_font: false,
            engine_font_family_version_seen: fonts::default_font_version(),
            embed_fast_glyph_paint: false,
            paint_line_visible_scratch: Vec::new(),
            py_input_focused: false,
            py_alignment: (0.0, 0.0),
        }
    }

    pub(crate) fn clear_trackpad_state_for_python_embed_handoff(&mut self) {
        self.trackpad_laser_x = None;
        self.trackpad_laser_y = None;
        self.trackpad_last_mouse_x = None;
        self.trackpad_last_mouse_y = None;
        self.trackpad_active = false;
        self.trackpad_selecting = false;
        self.trackpad_moved = false;
    }

    /// Whether (`mx`,`my`) lies inside the rounded Python embed viewport (normalized rect → px).
    pub(crate) fn python_viewport_contains_screen_point(&self, mx: f32, my: f32) -> bool {
        let Some((vx, vy, vw, vh)) = self.python_viewport else {
            return true;
        };
        let x0 = vx as f32;
        let y0 = vy as f32;
        let x1 = x0 + vw as f32;
        let y1 = y0 + vh as f32;
        mx >= x0 && mx < x1 && my >= y0 && my < y1
    }

    /// Rounded pixel viewport from normalized rect — must match [`crate::ui::text::UiText::render`].
    pub(crate) fn rounded_norm_rect_to_px(
        nx1: f32,
        ny1: f32,
        nx2: f32,
        ny2: f32,
        frame_w: f32,
        frame_h: f32,
    ) -> (i32, i32, u32, u32) {
        let fw = frame_w.max(1.0);
        let fh = frame_h.max(1.0);
        let xa = (nx1.clamp(0.0, 1.0) * fw).round() as i32;
        let ya = (ny1.clamp(0.0, 1.0) * fh).round() as i32;
        let xb = (nx2.clamp(0.0, 1.0) * fw).round() as i32;
        let yb = (ny2.clamp(0.0, 1.0) * fh).round() as i32;
        let vw = (xb.saturating_sub(xa)).max(1) as u32;
        let vh = (yb.saturating_sub(ya)).max(1) as u32;
        (xa, ya, vw, vh)
    }

    fn sync_python_viewport_from_norm(&mut self, frame_w: f32, frame_h: f32) {
        let Some((nx1, ny1, nx2, ny2)) = self.python_viewport_norm else {
            return;
        };
        self.python_viewport = Some(Self::rounded_norm_rect_to_px(
            nx1, ny1, nx2, ny2, frame_w, frame_h,
        ));
    }

    /// Lowest document Y that must stay visible below the last line (`baseline + descent`).
    /// Scroll limits use **absolute** doc Y (viewport covers `[scroll_y, scroll_y + visible_h]`), not `(max_y−min_y)`.
    #[inline]
    fn document_bottom_y_px(&self) -> f32 {
        let a = self.text_rasterizer.ascent;
        let d = self.text_rasterizer.descent.abs();
        let line_height = a + d + self.text_rasterizer.line_gap;

        let last_line_bottom = self
            .text_rasterizer
            .lines
            .last()
            .map(|l| l.baseline_y + d)
            .unwrap_or(line_height);

        if self.text_rasterizer.characters.is_empty() {
            return last_line_bottom.max(line_height).max(1.0);
        }

        let mut glyph_bottom = f32::NEG_INFINITY;
        for c in &self.text_rasterizer.characters {
            glyph_bottom = glyph_bottom.max(c.y + c.height);
        }
        // Bitmap bbox can sit slightly above the line box; still allow scrolling to full descender depth.
        glyph_bottom.max(last_line_bottom).max(1.0)
    }

    /// Max scroll offset: last pixel row of content can meet the bottom of the viewport.
    fn max_scroll_y_for_viewport(&self, state: &EngineState) -> f32 {
        let visible_height = self.viewport_metrics(state).visible_h;
        let doc_bottom = self.document_bottom_y_px();
        (doc_bottom - visible_height).max(0.0)
    }

    pub(crate) fn viewport_metrics(&self, state: &EngineState) -> TextViewportMetrics {
        let shape = state.frame.shape();
        let frame_w = shape[1] as f32;
        let frame_h = shape[0] as f32;
        let safe = &state.frame.safe_region_boundaries;
        let (_, keyboard_top_y, _, _) = state.keyboard.onscreen.top_edge_coordinates();
        let content_top = self.layout_content_top(safe.y1, frame_h);
        let content_bottom = self.effective_content_bottom_px(frame_h, keyboard_top_y);
        if let Some((vx_i, vy_i, vw_u, vh_u)) = self.python_viewport {
            let vx = vx_i as f32;
            let vy = vy_i as f32;
            let vw = vw_u.max(1) as f32;
            // Match [`UiText::render`] clip rect height (no keyboard clip — render draws the same box).
            let vh = vh_u.max(1) as f32;
            TextViewportMetrics {
                frame_w,
                frame_h,
                layout_w: vw,
                visible_h: vh,
                draw_x: vx,
                draw_y: vy,
                embed: true,
            }
        } else {
            TextViewportMetrics {
                frame_w,
                frame_h,
                layout_w: frame_w,
                visible_h: (content_bottom - content_top).max(1.0),
                draw_x: 0.0,
                draw_y: content_top,
                embed: false,
            }
        }
    }

    pub(crate) fn build_viewport_paint_ctx(
        &self,
        vp_m: TextViewportMetrics,
        width: f32,
        height: f32,
        content_top: f32,
        is_trackpad_mode: bool,
        is_keyboard_shown: bool,
        paint_cursor: bool,
    ) -> ViewportPaintCtx {
        let layout_w = vp_m.layout_w;
        let visible_height = vp_m.visible_h;
        let draw_off_x = vp_m.draw_x;
        let draw_off_y = vp_m.draw_y;
        ViewportPaintCtx {
            fw: width as usize,
            fh: height as usize,
            width,
            height,
            w_i: width as i32,
            h_i: height as i32,
            clip_left: draw_off_x as i32,
            clip_top: draw_off_y as i32,
            clip_right: (draw_off_x + layout_w) as i32,
            clip_bottom: (draw_off_y + visible_height) as i32,
            draw_off_x,
            draw_off_y,
            layout_w,
            visible_height,
            content_top,
            is_trackpad_mode,
            is_keyboard_shown,
            paint_cursor,
        }
    }

    pub(crate) fn paint_viewport(&mut self, ctx: &ViewportPaintCtx, buffer: &mut [u8], glyph_rgba: (u8, u8, u8, u8)) {
        let fw = ctx.fw;
        let fh = ctx.fh;
        let width = ctx.width;
        let height = ctx.height;
        let w_i = ctx.w_i;
        let h_i = ctx.h_i;
        let clip_left = ctx.clip_left;
        let clip_top = ctx.clip_top;
        let clip_right = ctx.clip_right;
        let clip_bottom = ctx.clip_bottom;
        let draw_off_x = ctx.draw_off_x;
        let draw_off_y = ctx.draw_off_y;
        let layout_w = ctx.layout_w;
        let visible_height = ctx.visible_height;
        let _content_top = ctx.content_top;
        let is_trackpad_mode = ctx.is_trackpad_mode;
        let is_keyboard_shown = ctx.is_keyboard_shown;
        let paint_cursor = ctx.paint_cursor;

        let (gr, gg, gb, ga) = (
            glyph_rgba.0 as u32,
            glyph_rgba.1 as u32,
            glyph_rgba.2 as u32,
            glyph_rgba.3 as u32,
        );

        let vis_top_doc = self.scroll_y;
        let vis_bottom_doc = self.scroll_y + visible_height;
        let a_doc = self.text_rasterizer.ascent;
        let d_doc = self.text_rasterizer.descent.abs();
        // Loose vertical band around each line baseline—skip all glyphs on lines that cannot intersect viewport.
        let n_lines = self.text_rasterizer.lines.len();
        self.paint_line_visible_scratch.resize(n_lines, false);
        for (i, ln) in self.text_rasterizer.lines.iter().enumerate() {
            self.paint_line_visible_scratch[i] = line_band_intersects_doc_viewport(
                ln.baseline_y,
                a_doc,
                d_doc,
                self.text_rasterizer.line_gap,
                self.text_rasterizer.font_size,
                vis_top_doc,
                visible_height,
            );
        }

        let line_quick_visible = |line_idx: usize| -> bool {
            match self.paint_line_visible_scratch.get(line_idx) {
                Some(false) => false,
                Some(true) | None => true,
            }
        };

        // Draw baselines (offset by layout origin)
        if DRAW_BASELINES && self.show_debug_visuals {
            for (line_idx, line) in self.text_rasterizer.lines.iter().enumerate() {
                if !line_quick_visible(line_idx) {
                    continue;
                }
                let y = ((line.baseline_y - self.scroll_y) + draw_off_y) as i32;
                if y >= 0 && y < height as i32 && y >= clip_top && y < clip_bottom {
                    fill_rect_buffer(
                        buffer,
                        fw,
                        fh,
                        clip_left,
                        y,
                        clip_right,
                        y + 1,
                        (BASELINE_COLOR.0, BASELINE_COLOR.1, BASELINE_COLOR.2, 0xff),
                    );
                }
            }
        }

        // Draw selection highlighting
        if let (Some(sel_start), Some(sel_end)) = (self.selection_start, self.selection_end) {
            let (start_idx, end_idx) = if sel_start <= sel_end {
                (sel_start, sel_end)
            } else {
                (sel_end, sel_start)
            };

            use std::collections::HashMap;
            let mut line_selections: HashMap<usize, (f32, f32, f32)> = HashMap::new();

            // Include every selected glyph on a visible line (do not use per-glyph screen culling here:
            // skipping “off-screen” advance boxes would shrink or empty the line’s min/max span).
            for character in &self.text_rasterizer.characters {
                if !line_quick_visible(character.line_index) {
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
                            *baseline_y = self
                                .text_rasterizer
                                .lines
                                .get(character.line_index)
                                .map(|line| line.baseline_y)
                                .unwrap_or(*baseline_y);
                        })
                        .or_insert_with(|| {
                            let baseline_y = self
                                .text_rasterizer
                                .lines
                                .get(character.line_index)
                                .map(|line| line.baseline_y)
                                .unwrap_or(0.0);
                            (char_left, char_right, baseline_y)
                        });
                }
            }

            for (_line_idx, (min_x, max_x, baseline_y)) in line_selections.iter() {
                // Layout-space spans must be offset by the embed viewport origin (same as glyph `px`).
                let sel_left = (draw_off_x + *min_x).round() as i32;
                let sel_right = (draw_off_x + *max_x).round() as i32;
                let sel_top =
                    ((baseline_y - self.text_rasterizer.ascent - self.scroll_y) + draw_off_y) as i32;
                let sel_bottom =
                    ((baseline_y + self.text_rasterizer.descent - self.scroll_y) + draw_off_y) as i32;

                let y_lo = sel_top.min(sel_bottom).max(clip_top).max(0).min(h_i.min(clip_bottom));
                let y_hi = sel_top.max(sel_bottom).max(clip_top).max(0).min(h_i.min(clip_bottom));
                let x_lo = sel_left.min(sel_right).max(clip_left).max(0).min(w_i.min(clip_right));
                let x_hi = sel_left.max(sel_right).max(clip_left).max(0).min(w_i.min(clip_right));

                for y in y_lo..y_hi {
                    for x in x_lo..x_hi {
                        let idx = ((y as u32 * width as u32 + x as u32) * 4) as usize;
                        let alpha = SELECTION_COLOR.3 as f32 / 255.0;
                        let inv_alpha = 1.0 - alpha;
                        buffer[idx + 0] = (buffer[idx + 0] as f32 * inv_alpha + SELECTION_COLOR.0 as f32 * alpha) as u8;
                        buffer[idx + 1] = (buffer[idx + 1] as f32 * inv_alpha + SELECTION_COLOR.1 as f32 * alpha) as u8;
                        buffer[idx + 2] = (buffer[idx + 2] as f32 * inv_alpha + SELECTION_COLOR.2 as f32 * alpha) as u8;
                    }
                }
            }
        }

        let fast_paint = self.embed_fast_glyph_paint;
        let ga_f = ga as f32 / 255.0;

        for character in &self.text_rasterizer.characters {
            if !line_quick_visible(character.line_index) {
                continue;
            }
            let g_top = character.y;
            let g_bottom = character.y + character.height;
            if g_bottom < vis_top_doc || g_top > vis_bottom_doc {
                continue;
            }
            let slide_max = character.width;
            let g_left = character.x - slide_max;
            let g_right = character.x + character.metrics.advance_width + character.metrics.width as f32;
            if g_right < 0.0 || g_left > layout_w {
                continue;
            }

            let (fade_alpha_byte, slide_offset, alpha_outline) = if fast_paint {
                (255u32, 0_i32, 255u8)
            } else {
                let fade_key = (character.ch, character.x.to_bits(), character.y.to_bits());
                let fade = self.fade_map.entry(fade_key).or_insert(0.0);
                *fade = (*fade + 0.16).min(1.0);
                let fade_alpha_byte = ((*fade * 255.0).round() as u32).min(255);
                let slide_offset = (character.width as f32 * 1.0 * (1.0 - *fade)) as i32;
                let alpha_outline = ((*fade * 255.0).round() as u8).min(255);
                (fade_alpha_byte, slide_offset, alpha_outline)
            };

            let px = draw_off_x as i32 + (character.x as i32) + slide_offset;
            let py = draw_off_y as i32 + ((character.y - self.scroll_y) as i32);
            let pw = character.width as u32;
            let ph = character.height as u32;

            let fade_f = fade_alpha_byte as f32 / 255.0;

            for y in 0..character.metrics.height {
                for x in 0..character.metrics.width {
                    let val = character.bitmap[y * character.metrics.width + x];
                    if val == 0 {
                        continue;
                    }

                    let sx = px + x as i32;
                    let sy = py + y as i32;

                    let in_viewport = sx >= clip_left && sx < clip_right && sy >= clip_top && sy < clip_bottom;
                    if !in_viewport || sx < 0 || sx >= width as i32 || sy < 0 || sy >= height as i32 {
                        continue;
                    }

                    let glyph_frac = val as f32 / 255.0;
                    // Same blend model as [`crate::ui::text::UiText::render`] so selection shows through AA edges.
                    let composite = (glyph_frac * fade_f * ga_f).clamp(0.0, 1.0);
                    let inv = 1.0 - composite;

                    let idx = ((sy as u32 * width as u32 + sx as u32) * 4) as usize;
                    buffer[idx + 0] = (gr as f32 * composite + buffer[idx + 0] as f32 * inv).clamp(0.0, 255.0) as u8;
                    buffer[idx + 1] = (gg as f32 * composite + buffer[idx + 1] as f32 * inv).clamp(0.0, 255.0) as u8;
                    buffer[idx + 2] = (gb as f32 * composite + buffer[idx + 2] as f32 * inv).clamp(0.0, 255.0) as u8;
                    buffer[idx + 3] = 0xff;
                }
            }

            if SHOW_BOUNDING_RECTANGLES && self.show_debug_visuals {
                Self::draw_rect(
                    buffer,
                    width as u32,
                    height as u32,
                    px,
                    py,
                    pw,
                    ph,
                    alpha_outline,
                    self.bound_color,
                );
            }
        }

        if paint_cursor {
            let (target_x, baseline_y) = self.get_cursor_screen_position();

            self.smooth_cursor_x += (target_x - self.smooth_cursor_x) * 0.2;

            let cursor_top =
                ((baseline_y - self.text_rasterizer.ascent - self.scroll_y) + draw_off_y).round() as i32;
            let cursor_bottom =
                ((baseline_y + self.text_rasterizer.descent - self.scroll_y) + draw_off_y).round() as i32;
            let cx = (draw_off_x + self.smooth_cursor_x).round() as i32;

            for y in cursor_top..cursor_bottom {
                if y >= clip_top && y < clip_bottom && cx >= clip_left && cx < clip_right
                    && y >= 0 && y < height as i32 && cx >= 0 && cx < width as i32
                {
                    let idx = ((y as u32 * width as u32 + cx as u32) * 4) as usize;
                    buffer[idx + 0] = CURSOR_COLOR.0;
                    buffer[idx + 1] = CURSOR_COLOR.1;
                    buffer[idx + 2] = CURSOR_COLOR.2;
                    buffer[idx + 3] = 0xff;
                }
            }
        }

        if is_trackpad_mode && is_keyboard_shown {
            // One shared laser across Python embed widgets: draw only from the focused editor.
            if !(self.python_viewport.is_some() && !self.py_input_focused) {
            if let (Some(laser_x), Some(laser_y)) = (self.trackpad_laser_x, self.trackpad_laser_y) {
                let dot_radius = 6.0;
                let dot_x_i = laser_x.round() as i32;
                let dot_y_i = laser_y.round() as i32;

                for dy in -(dot_radius as i32)..=(dot_radius as i32) {
                    for dx in -(dot_radius as i32)..=(dot_radius as i32) {
                        let distance = ((dx * dx + dy * dy) as f32).sqrt();
                        if distance <= dot_radius {
                            let x = dot_x_i + dx;
                            let y = dot_y_i + dy;
                            if x >= 0 && x < width as i32 && y >= 0 && y < height as i32 {
                                let idx = ((y as u32 * width as u32 + x as u32) * 4) as usize;
                                buffer[idx + 0] = 255;
                                buffer[idx + 1] = 0;
                                buffer[idx + 2] = 0;
                                buffer[idx + 3] = 255;
                            }
                        }
                    }
                }
            }
            }
        }
    }

    /// Blit this widget into `buffer` (`stride_w` × `stride_h` RGBA8) using the same viewport math as [`Self::tick`].
    pub(crate) fn paint_into_buffer_for_engine_frame(
        &mut self,
        engine: &EngineState,
        buffer: &mut [u8],
        stride_w: usize,
        stride_h: usize,
        glyph_rgba: (u8, u8, u8, u8),
        paint_cursor: bool,
    ) -> Result<(), &'static str> {
        let shape = engine.frame.shape();
        let width = shape[1] as f32;
        let height = shape[0] as f32;
        if stride_w != width as usize || stride_h != height as usize {
            return Err("stride does not match engine frame dimensions");
        }
        let needed = stride_w.saturating_mul(stride_h).saturating_mul(4);
        if buffer.len() < needed {
            return Err("frame buffer slice too short");
        }

        let safe = &engine.frame.safe_region_boundaries;
        let content_top = self.layout_content_top(safe.y1, height);
        let is_trackpad_mode = engine.keyboard.onscreen.is_trackpad_mode();
        let is_keyboard_shown = engine.keyboard.onscreen.is_shown();

        let vp_m = self.viewport_metrics(engine);
        if is_trackpad_mode && is_keyboard_shown {
            // One shared laser across Python embed widgets; ensure coords exist before painting.
            if !(self.python_viewport.is_some() && !self.py_input_focused) {
                self.ensure_trackpad_laser_initialized(engine);
            }
        }
        let ctx = self.build_viewport_paint_ctx(
            vp_m,
            width,
            height,
            content_top,
            is_trackpad_mode,
            is_keyboard_shown,
            paint_cursor,
        );
        self.paint_viewport(&ctx, buffer, glyph_rgba);
        Ok(())
    }

    /// Layout-space XY for glyph hit-testing: x in `[0, layout_w]`, y in document coords (includes [`Self::scroll_y`]).
    fn to_text_xy(&self, state: &EngineState, screen_x: f32, screen_y: f32) -> (f32, f32) {
        let vp = self.viewport_metrics(state);
        (
            screen_x - vp.draw_x,
            screen_y - vp.draw_y + self.scroll_y,
        )
    }

    #[inline]
    pub(crate) fn layout_content_top(&self, safe_y1: f32, height: f32) -> f32 {
        safe_y1 * height + self.top_chrome_height_px
    }

    #[inline]
    fn effective_content_bottom_px(&self, height: f32, keyboard_top_y: f32) -> f32 {
        keyboard_top_y * height - self.bottom_chrome_height_px
    }

    /// Same as [`TextApp::new`] but tuned for the small transparent overlay (`xos app overlay`).
    pub fn new_for_overlay() -> Self {
        let mut t = Self::new();
        t.transparent_background = true;
        t.bound_color = (57, 255, 20);
        t.set_font_size(14.0);
        t.last_engine_scaled_font = 14.0;
        t
    }

    /// Resize editor text; layout is updated on the next [`tick`](TextApp::tick).
    pub fn set_font_size(&mut self, font_size: f32) {
        self.text_rasterizer.set_font_size(font_size);
    }

    /// Clears stacked wheel speed (1×…3×). Call when resetting scroll from outside (e.g. coder file load).
    pub fn clear_wheel_scroll_accel(&mut self) {
        self.wheel_accel_target = 0.0;
        self.wheel_last_activity = None;
    }

    fn draw_rect(
        buffer: &mut [u8],
        width: u32,
        height: u32,
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        alpha: u8,
        rgb: (u8, u8, u8),
    ) {
        if x < 0 || y < 0 || w == 0 || h == 0 {
            return;
        }
        let x = x as u32;
        let y = y as u32;
    
        let mut draw_pixel = |x, y| {
            if x < width && y < height {
                let idx = ((y * width + x) * 4) as usize;
                buffer[idx + 0] = rgb.0;
                buffer[idx + 1] = rgb.1;
                buffer[idx + 2] = rgb.2;
                buffer[idx + 3] = alpha;
            }
        };
    
        for dx in 0..w {
            draw_pixel(x + dx, y);
            draw_pixel(x + dx, y + h.saturating_sub(1));
        }
        for dy in 0..h {
            draw_pixel(x, y + dy);
            draw_pixel(x + w.saturating_sub(1), y + dy);
        }
    }
}

impl Application for TextApp {
    fn setup(&mut self, state: &mut EngineState) -> Result<(), String> {
        // On iOS, initialize scroll to 0 (text starts at safe region top)
        if cfg!(target_os = "ios") && !self.text_rasterizer.text.is_empty() {
            let shape = state.frame.shape();
            let height = shape[0] as f32;
            let safe_region = &state.frame.safe_region_boundaries;
            let content_top = self.layout_content_top(safe_region.y1, height);
            let content_height = height - content_top;
            
            // Tick the engine once to calculate line positions
            self.text_rasterizer.tick(shape[1] as f32, content_height);
            
            // Start with scroll at 0 (text at top of safe region)
            self.scroll_y = 0.0;
            self.scroll_target = 0.0;
        }
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        // OSK queues are global: only the focused Python embed (or any standalone app) may drain them.
        // Otherwise the first widget ticked (e.g. text1 in a Group) consumes all `pop_pending_char` input.
        let ingest_onscreen_keys = self.python_viewport.is_none() || self.py_input_focused;
        if ingest_onscreen_keys {
            while let Some(ch) = state.keyboard.onscreen.pop_pending_char() {
                self.on_key_char(state, ch);
            }

            if let Some(action) = state.keyboard.onscreen.get_last_action_key() {
                self.handle_action_key(action, state);
            }

            let now = Instant::now();
            if let Some(action) = state.keyboard.onscreen.check_action_key_hold_repeat(now) {
                self.handle_action_key(action, state);
            }
        }

        if self.follow_engine_default_font {
            self.text_rasterizer
                .sync_default_font_family_from_engine(&mut self.engine_font_family_version_seen);
        }
        
        // Extract all needed values in a block to release borrows
        let (width, height, content_top, _content_bottom, is_trackpad_mode, is_keyboard_shown) = {
            let shape = state.frame.shape();
            let width = shape[1] as f32;
            let height = shape[0] as f32;
            
            let safe_region = &state.frame.safe_region_boundaries;
            
            // Get keyboard top edge (whether visible or not)
            let (_, keyboard_top_y, _, _) = state.keyboard.onscreen.top_edge_coordinates();
            
            let content_top = self.layout_content_top(safe_region.y1, height);
            let content_bottom = self.effective_content_bottom_px(height, keyboard_top_y); // Above keyboard / parent chrome
            
            let is_trackpad_mode = state.keyboard.onscreen.is_trackpad_mode();
            let is_keyboard_shown = state.keyboard.onscreen.is_shown();
            
            (width, height, content_top, content_bottom, is_trackpad_mode, is_keyboard_shown)
        };

        self.sync_python_viewport_from_norm(width, height);

        let vp_m = self.viewport_metrics(state);
        let layout_w = vp_m.layout_w;
        let visible_height = vp_m.visible_h;
        let python_embed_active = vp_m.embed;

        // Standalone text app: F3 multiplier is `ui_scale_percent / 100` (25–500% → 0.25–5.0), same as coder.
        if !self.uses_parent_ui_scale {
            let short_edge = width.min(height);
            let base_u = (short_edge / 920.0).clamp(0.28, 1.0);
            let coeff = state.f3_ui_scale_multiplier();
            let ios = if cfg!(target_os = "ios") { 1.1 } else { 1.0 };
            let (base_px, text_cal) = if self.transparent_background {
                (14.0_f32, 1.0)
            } else {
                (24.0_f32 * ios, 50.0 / 20.0)
            };
            let standalone_mul = if self.transparent_background {
                1.0
            } else {
                TEXT_STANDALONE_SIZE_FACTOR
            };
            let target_font = base_px * base_u * coeff * text_cal * standalone_mul;
            if (target_font - self.last_engine_scaled_font).abs() > 0.01 {
                self.set_font_size(target_font);
                self.last_engine_scaled_font = target_font;
            }
        }
        
        let dt = state.delta_time_seconds.clamp(1e-4, 0.1);

        // Reflow/Wrap FIRST: scroll limits MUST use lines from *this* frame's wrap width (`layout_w`).
        let align = if self.python_viewport.is_some() {
            TextLayoutAlign {
                x: self.py_alignment.0,
                y: self.py_alignment.1,
            }
        } else {
            TextLayoutAlign::default()
        };
        self.text_rasterizer
            .tick_aligned(layout_w, visible_height, align);

        // Mouse wheel streak decay: only while the user is not actively emitting wheel events (FPS-stable).
        let wheel_idle_for_decay = match self.wheel_last_activity {
            None => true,
            Some(t) => t.elapsed() >= WHEEL_STREAK_HOLD,
        };
        if wheel_idle_for_decay {
            self.wheel_accel_target *= WHEEL_ACCEL_IDLE_DECAY.powf(dt / SCROLL_REF_DT);
            self.wheel_accel_target = self.wheel_accel_target.clamp(0.0, 1.0);
        }

        // Drag-release coast: move target by momentum, then decay (dt-corrected).
        if !self.dragging && self.drag_scroll_momentum.abs() > DRAG_MOMENTUM_STOP {
            self.scroll_target += self.drag_scroll_momentum * dt;
            self.drag_scroll_momentum *= DRAG_MOMENTUM_DECAY.powf(dt / SCROLL_REF_DT);
        } else if !self.dragging {
            self.drag_scroll_momentum = 0.0;
        }

        // Scroll bounds: viewport top is `scroll_y` in doc space; bottom is `scroll_y + visible_height`.
        let doc_bottom = self.document_bottom_y_px();

        // 3. Calculate natural content bounds
        let natural_min = 0.0;
        let natural_max = (doc_bottom - visible_height).max(0.0);
        
        // 4. Calculate overscroll limits (based on screen percentage)
        let overscroll_distance = visible_height * SCROLL_OVERSCROLL_LIMIT;
        let limit_min = natural_min - overscroll_distance;
        let limit_max = natural_max + overscroll_distance;
        
        // 5. Apply limits and physics (authoritative `scroll_target`, smoothed `scroll_y`)
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

        if !self.embed_fast_glyph_paint {
            // Reflow/resize changes every glyph (x,y), which would reset fade keys — seed full opacity so resize stays solid.
            let wrap_width = layout_w;
            let font_size = self.text_rasterizer.font_size;
            let layout_changed_for_fade = (wrap_width - self.last_fade_wrap_width).abs() > 0.01
                || (font_size - self.last_fade_font_size).abs() > 0.01;
            if layout_changed_for_fade {
                self.fade_map.clear();
                for character in &self.text_rasterizer.characters {
                    let fade_key = (character.ch, character.x.to_bits(), character.y.to_bits());
                    self.fade_map.insert(fade_key, 1.0);
                }
                self.last_fade_wrap_width = wrap_width;
                self.last_fade_font_size = font_size;
            }
        }

        if self.embed_skip_frame_present {
            return;
        }

        if !python_embed_active {
            if self.transparent_background {
                fill(&mut state.frame, (0, 0, 0, 0));
            } else {
                fill(
                    &mut state.frame,
                    (BACKGROUND_COLOR.0, BACKGROUND_COLOR.1, BACKGROUND_COLOR.2, 0xff),
                );
            }
        }

        let buffer = state.frame_buffer_mut();
        let vp_ctx = self.build_viewport_paint_ctx(
            vp_m,
            width,
            height,
            content_top,
            is_trackpad_mode,
            is_keyboard_shown,
            self.show_cursor,
        );
        self.paint_viewport(
            &vp_ctx,
            buffer,
            (TEXT_COLOR.0, TEXT_COLOR.1, TEXT_COLOR.2, 255),
        );
    }
    

    fn on_scroll(&mut self, _state: &mut EngineState, _dx: f32, dy: f32, unit: ScrollWheelUnit) {
        if self.python_viewport.is_some() && !self.py_scrollable {
            return;
        }
        self.drag_scroll_momentum = 0.0;
        self.wheel_last_activity = Some(Instant::now());
        let scaled = match unit {
            ScrollWheelUnit::Line => dy * MOUSE_WHEEL_LINE_SCALE,
            ScrollWheelUnit::Pixel => dy * TRACKPAD_SCROLL_PIXEL_SCALE,
        };
        self.wheel_accel_target = (self.wheel_accel_target + WHEEL_CHARGE_PER_NOTCH).min(1.0);
        // Multiplier tracks charged streak immediately (no tick-lagged duplicate state).
        let mult = 1.0 + 2.0 * self.wheel_accel_target;
        self.scroll_target -= scaled * mult;
        self.last_tap_scrolled = true;
    }

    fn on_key_char(&mut self, state: &mut EngineState, ch: char) {
        // Don't process keys if read-only
        if self.read_only {
            return;
        }
        
        let content_height = self.viewport_metrics(state).visible_h;
        
        match ch {
            ARROW_LEFT => {
                self.move_cursor_left();
                self.ensure_cursor_visible(content_height);
            }
            ARROW_RIGHT => {
                self.move_cursor_right();
                self.ensure_cursor_visible(content_height);
            }
            ARROW_UP => {
                self.move_cursor_up();
                self.ensure_cursor_visible(content_height);
            }
            ARROW_DOWN => {
                self.move_cursor_down();
                self.ensure_cursor_visible(content_height);
            }
            '\t' => {
                self.save_undo_state();
                // Delete selection if present
                self.delete_selection();
                // Insert tab at cursor position
                let text_chars: Vec<char> = self.text_rasterizer.text.chars().collect();
                let mut new_text = String::new();
                for (i, &c) in text_chars.iter().enumerate() {
                    if i == self.cursor_position {
                        new_text.push_str("    ");
                    }
                    new_text.push(c);
                }
                if self.cursor_position >= text_chars.len() {
                    new_text.push_str("    ");
                }
                self.text_rasterizer.text = new_text;
                self.cursor_position += 4;
                self.ensure_cursor_visible(content_height);
            }
            '\r' | '\n' => {
                self.save_undo_state();
                // Delete selection if present
                self.delete_selection();
                // Insert newline at cursor position
                let text_chars: Vec<char> = self.text_rasterizer.text.chars().collect();
                let mut new_text = String::new();
                for (i, &c) in text_chars.iter().enumerate() {
                    if i == self.cursor_position {
                        new_text.push('\n');
                    }
                    new_text.push(c);
                }
                if self.cursor_position >= text_chars.len() {
                    new_text.push('\n');
                }
                self.text_rasterizer.text = new_text;
                self.cursor_position += 1;
                self.ensure_cursor_visible(content_height);
            }
            '\u{8}' => {
                // Backspace - delete selection if present, otherwise delete character before cursor
                self.save_undo_state();
                if !self.delete_selection() {
                    // No selection, delete character before cursor
                    if self.cursor_position > 0 {
                        let text_chars: Vec<char> = self.text_rasterizer.text.chars().collect();
                        let mut new_text = String::new();
                        for (i, &c) in text_chars.iter().enumerate() {
                            if i != self.cursor_position - 1 {
                                new_text.push(c);
                            }
                        }
                        self.text_rasterizer.text = new_text;
                        self.cursor_position -= 1;
                    }
                }
                self.ensure_cursor_visible(content_height);
            }
            _ => {
                if !ch.is_control() {
                    self.save_undo_state();
                    // Delete selection if present
                    self.delete_selection();
                    // Insert character at cursor position
                    let text_chars: Vec<char> = self.text_rasterizer.text.chars().collect();
                    let mut new_text = String::new();
                    for (i, &c) in text_chars.iter().enumerate() {
                        if i == self.cursor_position {
                            new_text.push(ch);
                        }
                        new_text.push(c);
                    }
                    if self.cursor_position >= text_chars.len() {
                        new_text.push(ch);
                    }
                    self.text_rasterizer.text = new_text;
                    self.cursor_position += 1;
                    self.ensure_cursor_visible(content_height);
                }
            }
        }
    }

    fn on_mouse_move(&mut self, state: &mut EngineState) {
        let shape = state.frame.shape();
        let width = shape[1] as f32;
        let height = shape[0] as f32;
        let allow_sel = !(self.python_viewport.is_some() && !self.py_selectable);

        // Check if temp trackpad mode should be activated (Shift/SymbolToggle drag)
        if let (Some(initial_x), Some(initial_y)) = (self.temp_trackpad_initial_x, self.temp_trackpad_initial_y) {
            if state.keyboard.onscreen.check_temp_trackpad_activation(initial_x, initial_y, state.mouse.x, state.mouse.y) {
                // Temp trackpad mode was activated — bootstrap laser using cursor / anchors.
                self.ensure_trackpad_laser_initialized(state);
                
                // Clear initial position tracking
                self.temp_trackpad_initial_x = None;
                self.temp_trackpad_initial_y = None;
                
                // Mark as active and set last mouse position
                self.trackpad_active = true;
                self.trackpad_last_mouse_x = Some(state.mouse.x);
                self.trackpad_last_mouse_y = Some(state.mouse.y);
            }
        }
        
        // Check if we're in trackpad mode AND actively using it
        if state.keyboard.onscreen.is_trackpad_mode() {
            // Bootstrap laser if unset (last plain click, strip-proportional fallback, or caret).
            if self.trackpad_laser_x.is_none() || self.trackpad_laser_y.is_none() {
                self.ensure_trackpad_laser_initialized(state);
            }
            
            // If mouse is in trackpad area and active (dragging), move the laser
            if self.trackpad_active && state.mouse.is_left_clicking {
                if let (Some(laser_x), Some(laser_y), Some(last_mouse_x), Some(last_mouse_y)) = 
                    (self.trackpad_laser_x, self.trackpad_laser_y, self.trackpad_last_mouse_x, self.trackpad_last_mouse_y) {
                    
                    let mouse_dx = state.mouse.x - last_mouse_x;
                    let mouse_dy = state.mouse.y - last_mouse_y;
                    
                    // Track if mouse moved (for tap vs drag detection)
                    if mouse_dx.abs() > 2.0 || mouse_dy.abs() > 2.0 {
                        self.trackpad_moved = true;
                    }
                    
                    // Move laser 2x with mouse movement (double speed)
                    let new_laser_x = (laser_x + mouse_dx * 2.0).max(0.0).min(width);

                    let embed_py = self.python_viewport.is_some();
                    let safe_region = &state.frame.safe_region_boundaries;
                    let content_top = self.layout_content_top(safe_region.y1, height);
                    let (_, keyboard_top_y, _, _) = state.keyboard.onscreen.top_edge_coordinates();
                    let content_bottom = self.effective_content_bottom_px(height, keyboard_top_y);

                    // Python multi-editor: laser may use the full frame from the top (notch/status)
                    // down to just above the OSK; caret updates only while the laser is inside a text rect.
                    let laser_y_global_min = if embed_py { 0.0 } else { safe_region.y1 * height };
                    let keyboard_top_px = keyboard_top_y * height;
                    let y_global_max = if keyboard_top_px > laser_y_global_min + 2.0 {
                        keyboard_top_px - 2.0
                    } else {
                        keyboard_top_px
                    };

                    let unconstrained_y = laser_y + mouse_dy * 2.0;
                    let new_laser_y = if embed_py {
                        unconstrained_y.max(laser_y_global_min).min(y_global_max)
                    } else if self.trackpad_active && state.mouse.is_left_clicking {
                        let vp = self.viewport_metrics(state);
                        let visible_h = vp.visible_h;
                        let overscroll = visible_h * SCROLL_OVERSCROLL_LIMIT;
                        let scroll_min = -overscroll;
                        let scroll_upper = self.max_scroll_y_for_viewport(state) + overscroll;

                        let mut y = unconstrained_y;
                        if unconstrained_y < content_top {
                            let spill = content_top - unconstrained_y;
                            self.scroll_target = (self.scroll_target - spill).max(scroll_min);
                            self.scroll_y = self.scroll_target;
                            y = content_top;
                        } else if unconstrained_y > content_bottom {
                            let spill = unconstrained_y - content_bottom;
                            self.scroll_target = (self.scroll_target + spill).min(scroll_upper);
                            self.scroll_y = self.scroll_target;
                            y = content_bottom;
                        }
                        y
                    } else {
                        unconstrained_y.max(content_top).min(content_bottom)
                    };

                    self.trackpad_laser_x = Some(new_laser_x);
                    self.trackpad_laser_y = Some(new_laser_y);

                    // Only move caret/selection while the laser is over this pane (still free elsewhere for future UI taps).
                    if !embed_py || self.python_viewport_contains_screen_point(new_laser_x, new_laser_y) {
                        let (text_x, text_y) = self.to_text_xy(state, new_laser_x, new_laser_y);
                        let char_index = self.find_nearest_char_index(text_x, text_y);
                        if self.trackpad_selecting {
                            self.selection_end = Some(char_index);
                        }
                        self.cursor_position = char_index;
                        let vh_sel = self.viewport_metrics(state).visible_h;
                        self.ensure_cursor_visible(vh_sel);
                    }
                }
                
                // Update last mouse position
                self.trackpad_last_mouse_x = Some(state.mouse.x);
                self.trackpad_last_mouse_y = Some(state.mouse.y);
                
                // Don't allow normal scrolling when actively using trackpad
                return;
            }
        } else {
            // Not in trackpad mode - clear laser
            self.trackpad_laser_x = None;
            self.trackpad_laser_y = None;
            self.trackpad_last_mouse_x = None;
            self.trackpad_last_mouse_y = None;
        }
        
        // Don't allow scrolling if touch started on keyboard
        if self.touch_started_on_keyboard {
            return;
        }
        
        // Check if mouse moved significantly from tap position (start dragging or selecting)
        if !self.dragging && !self.selecting && state.mouse.is_left_clicking {
            let dx = (state.mouse.x - self.last_tap_x).abs();
            let dy = (state.mouse.y - self.last_tap_y).abs();
            // Start dragging/selecting if moved more than 5 pixels
            if dx > 5.0 || dy > 5.0 {
                // On iOS: never allow text selection from touch, only from trackpad
                // On macOS: allow text selection from mouse drag
                #[cfg(target_os = "ios")]
                {
                    // iOS: Always scroll on touch, selection only via trackpad (handled separately)
                    self.dragging = true;
                    self.last_mouse_y = state.mouse.y;
                }
                
                #[cfg(not(target_os = "ios"))]
                {
                    // macOS/Desktop: Allow text selection from click-drag
                    // When keyboard is shown (mobile mode): vertical is scroll, horizontal is selection
                    // When keyboard is hidden (desktop mode): horizontal is selection, vertical is scroll
                    if state.keyboard.onscreen.is_shown() {
                        // Mobile mode (keyboard visible): vertical drag scrolls, horizontal drag selects
                        if dy > dx {
                            // Vertical movement dominates - scroll
                            self.dragging = true;
                            self.last_mouse_y = state.mouse.y;
                        } else if allow_sel {
                            // Horizontal movement dominates - select
                            self.selecting = true;
                            // Get character index at initial tap position for selection start
                            let (text_x, text_y) = self.to_text_xy(state, self.last_tap_x, self.last_tap_y);
                            let start_char_idx = self.find_nearest_char_index(text_x, text_y);
                            
                            self.selection_start = Some(start_char_idx);
                            self.selection_end = Some(start_char_idx);
                            self.cursor_position = start_char_idx;
                        }
                    } else {
                        // Desktop mode (keyboard hidden): horizontal drag selects, vertical drag scrolls
                        if dx > dy && allow_sel {
                            self.selecting = true;
                            let (text_x, text_y) = self.to_text_xy(state, self.last_tap_x, self.last_tap_y);
                            let start_char_idx = self.find_nearest_char_index(text_x, text_y);
                            
                            self.selection_start = Some(start_char_idx);
                            self.selection_end = Some(start_char_idx);
                            self.cursor_position = start_char_idx;
                        } else {
                            self.dragging = true;
                            self.last_mouse_y = state.mouse.y;
                        }
                    }
                }
            }
        }
        
        // Handle text selection while dragging
        if allow_sel && self.selecting && state.mouse.is_left_clicking {
            // Convert mouse coordinates to text coordinates
            let (text_x, text_y) = self.to_text_xy(state, state.mouse.x, state.mouse.y);
            
            // Find nearest character to mouse position
            let char_index = self.find_nearest_char_index(text_x, text_y);
            self.selection_end = Some(char_index);
            self.cursor_position = char_index;
            let vh_drag = self.viewport_metrics(state).visible_h;
            self.ensure_cursor_visible(vh_drag);
        }
        
        // Scroll drag: 1:1 tracking + EMA of finger speed for release momentum.
        if self.dragging {
            if self.last_drag_sample_time.is_none() {
                self.wheel_accel_target = 0.0;
                self.wheel_last_activity = None;
            }
            let now = Instant::now();
            let dy = state.mouse.y - self.last_mouse_y;
            if let Some(t0) = self.last_drag_sample_time {
                let sample_dt = now.duration_since(t0).as_secs_f32().max(1e-4);
                let instant_v = (-dy) / sample_dt;
                self.drag_scroll_momentum = self.drag_scroll_momentum * 0.42 + instant_v * 0.58;
            } else {
                self.drag_scroll_momentum = 0.0;
            }
            self.last_drag_sample_time = Some(now);
            self.scroll_target -= dy;
            self.scroll_y = self.scroll_target;
            self.last_mouse_y = state.mouse.y;
        }
    }

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        let shape = state.frame.shape();
        let height = shape[0] as f32;
        let width = shape[1] as f32;

        // Embedded Python `xos.ui.Text`: [`PyApp`] already called [`OnScreenKeyboard::on_mouse_down`] and
        // queued any character/action. Any press in the **visible OSK strip** (keys + gaps) must not run
        // text hit-testing: `check_key_type_at_position` misses gaps / nearest-target mismatches and would
        // set `pending_cursor_tap_*`, snapping the caret on mouse-up — e.g. after OSK Enter, the next key
        // release appears to jump to the prior line.
        if self.python_viewport.is_some() && state.keyboard.onscreen.is_shown() {
            let (_, keyboard_top_y, _, _) = state.keyboard.onscreen.top_edge_coordinates();
            let keyboard_region_top = keyboard_top_y * height;

            if state.mouse.y >= keyboard_region_top {
                // Trackpad strip uses the standalone trackpad branches below (`is_trackpad_mode`).
                if !state.keyboard.onscreen.is_trackpad_mode() {
                    self.touch_started_on_keyboard = true;
                    self.pending_cursor_tap_x = None;
                    self.pending_cursor_tap_y = None;

                    let held_key = state.keyboard.onscreen.get_held_key_type();
                    if let Some(key_type) = held_key {
                        match key_type {
                            KeyType::Shift | KeyType::SymbolToggle => {
                                self.temp_trackpad_initial_x = Some(state.mouse.x);
                                self.temp_trackpad_initial_y = Some(state.mouse.y);
                            }
                            _ => {}
                        }
                    }
                    return;
                }
            } else {
                self.touch_started_on_keyboard = false;
                state.embed_last_plain_click_screen = Some((state.mouse.x, state.mouse.y));
            }
        }

        // Check if keyboard handled the event (Python host forwards pointer first when embedded — skip dup)
        let keyboard_claimed =
            self.python_viewport.is_none()
                && state.keyboard.onscreen.on_mouse_down(
                    state.mouse.x,
                    state.mouse.y,
                    width,
                    height,
                );
        if keyboard_claimed {
            // Mark that touch started on keyboard to prevent scrolling
            self.touch_started_on_keyboard = true;
            
            // Check if user pressed Shift or SymbolToggle (for temp trackpad activation on drag)
            // This covers: Shift (standard mode), #+= (symbols1 mode), and 123 (symbols2 mode)
            let held_key = state.keyboard.onscreen.get_held_key_type();
            if let Some(key_type) = held_key {
                match key_type {
                    KeyType::Shift | KeyType::SymbolToggle => {
                        // Record initial position for temp trackpad activation check
                        self.temp_trackpad_initial_x = Some(state.mouse.x);
                        self.temp_trackpad_initial_y = Some(state.mouse.y);
                    }
                    _ => {}
                }
            }
            return;
        }
        
        // Check if we're in trackpad mode and clicking in the trackpad area
        if state.keyboard.onscreen.is_trackpad_mode() {
            let (_, keyboard_top_y, _, _) = state.keyboard.onscreen.top_edge_coordinates();
            let keyboard_region_top = keyboard_top_y * height;
            
            // Check if clicking in the trackpad area (keyboard region)
            if state.mouse.y >= keyboard_region_top {
                self.trackpad_active = true;
                
                if self.trackpad_laser_x.is_none() || self.trackpad_laser_y.is_none() {
                    self.ensure_trackpad_laser_initialized(state);
                }
                
                self.trackpad_last_mouse_x = Some(state.mouse.x);
                self.trackpad_last_mouse_y = Some(state.mouse.y);
                
                // Check for double-tap to start selection
                let now = Instant::now();
                let is_double_tap = if let Some(last_time) = self.trackpad_last_tap_time {
                    let time_since_last = now.duration_since(last_time);
                    time_since_last < Duration::from_millis(DOUBLE_TAP_TIME_MS)
                } else {
                    false
                };
                
                if is_double_tap {
                    // Start selection mode
                    self.trackpad_selecting = true;
                    self.selection_start = Some(self.cursor_position);
                    self.selection_end = Some(self.cursor_position);
                    self.trackpad_last_tap_time = None; // Reset to prevent triple-tap
                } else {
                    // Record tap time (selection will be cleared on release if no drag)
                    self.trackpad_last_tap_time = Some(now);
                }
                
                // Reset moved flag
                self.trackpad_moved = false;
                
                return;
            }
        }
        
        // Touch started outside keyboard or keyboard is hidden
        self.touch_started_on_keyboard = false;
        
        // Get keyboard region for double-tap detection
        let (_, keyboard_top_y, _, _) = state.keyboard.onscreen.top_edge_coordinates();
        let keyboard_region_top = keyboard_top_y * height;
        
        // Check for double tap in content area OR in keyboard region (even if hidden)
        // Only trigger if user didn't scroll between taps AND velocity is low
        let now = Instant::now();
        let is_double_tap = if let Some(last_time) = self.last_tap_time {
            let time_since_last = now.duration_since(last_time);
            let distance = ((state.mouse.x - self.last_tap_x).powi(2) + (state.mouse.y - self.last_tap_y).powi(2)).sqrt();
            
            time_since_last < Duration::from_millis(DOUBLE_TAP_TIME_MS) 
                && distance < DOUBLE_TAP_DISTANCE 
                && !self.last_tap_scrolled // Don't trigger if user scrolled
                && (self.scroll_target - self.scroll_y).abs() < SCROLL_SETTLE_FOR_TAP
                && self.drag_scroll_momentum.abs() < DRAG_MOMENTUM_SETTLE_FOR_TAP // Not coasting from drag
        } else {
            false
        };
        
        if is_double_tap {
            // While keyboard is visible, taps on the keyboard bar are mostly key hits — typing can
            // produce two taps within DOUBLE_TAP_TIME_MS; don't treat that as "toggle keyboard".
            let quick_typing_on_keyboard = state.keyboard.onscreen.is_shown()
                && (state.mouse.y >= keyboard_region_top || self.last_tap_y >= keyboard_region_top);
            if quick_typing_on_keyboard {
                // Refresh tap baseline so we don't accumulate spurious gesture state
                self.last_tap_time = Some(now);
                self.last_tap_x = state.mouse.x;
                self.last_tap_y = state.mouse.y;
                self.last_tap_scrolled = false;
                return;
            }

            // Toggle keyboard (typically from double-tap in the text area)
            state.keyboard.onscreen.toggle_minimize();
            // Reset tap tracking to prevent triple-tap from immediately closing
            self.last_tap_time = None;
            self.last_tap_scrolled = false;
            // Clear pending cursor position
            self.pending_cursor_tap_x = None;
            self.pending_cursor_tap_y = None;
            return; // Don't process cursor positioning for double-tap
        }
        
        // If single tap in keyboard region while keyboard is hidden, don't move cursor
        if state.mouse.y >= keyboard_region_top && !state.keyboard.onscreen.is_shown() {
            // Just update tap tracking for potential double-tap
            self.last_tap_time = Some(now);
            self.last_tap_x = state.mouse.x;
            self.last_tap_y = state.mouse.y;
            self.last_tap_scrolled = false; // Reset scroll flag for new tap
            return;
        }
        
        // Normal single tap in content area — skip cursor placement when not selectable (Python widgets)
        if !(self.python_viewport.is_some() && !self.py_selectable) {
            // Single tap - record position but don't move cursor yet
            // We'll move cursor on mouse up if user didn't scroll
            self.pending_cursor_tap_x = Some(state.mouse.x);
            self.pending_cursor_tap_y = Some(state.mouse.y);
            self.initial_scroll_target = self.scroll_target;

            // Standalone taps clear prior selection before drag; embedded multi-editor taps keep selections
            // per pane until an explicit caret placement on that pane (persist across unfocus elsewhere).
            if self.python_viewport.is_none() {
                self.selection_start = None;
                self.selection_end = None;
            }
        }
        
        // Update tap tracking
        self.last_tap_time = Some(now);
        self.last_tap_x = state.mouse.x;
        self.last_tap_y = state.mouse.y;
        self.last_tap_scrolled = false; // Reset scroll flag for new tap
        
        // Don't start dragging immediately - wait for mouse movement
        // Dragging will be started in on_mouse_move if mouse moves significantly
    }

    fn on_mouse_up(&mut self, state: &mut EngineState) {
        // Release keyboard key holds (skipped when embedded: host already routed pointer)
        if self.python_viewport.is_none() {
            state.keyboard.onscreen.on_mouse_up();
        }
        
        // Check if we should move cursor (only if user didn't scroll and didn't drag/select)
        if let (Some(tap_x), Some(tap_y)) = (self.pending_cursor_tap_x, self.pending_cursor_tap_y) {
            // Check if user scrolled (scroll_y changed significantly)
            let scroll_delta = (self.scroll_target - self.initial_scroll_target).abs();
            let scroll_threshold = 1.0; // pixels
            
            // Check if user dragged (moved mouse significantly)
            let drag_distance = ((state.mouse.x - tap_x).powi(2) + (state.mouse.y - tap_y).powi(2)).sqrt();
            let drag_threshold = 10.0; // pixels
            
            // Only move cursor if user didn't scroll and didn't drag/select
            if scroll_delta < scroll_threshold && !self.selecting && (!self.dragging || drag_distance < drag_threshold) {
                let (text_x, text_y) = self.to_text_xy(state, tap_x, tap_y);
                let char_index = self.find_nearest_char_index(text_x, text_y);
                self.cursor_position = char_index;

                // Standalone clears selection on caret taps; embedded clears only when the tap lands in this pane.
                if self.python_viewport.is_none() {
                    self.selection_start = None;
                    self.selection_end = None;
                } else if self.python_viewport_contains_screen_point(tap_x, tap_y) {
                    self.selection_start = None;
                    self.selection_end = None;
                }
            }
            
            // Clear pending cursor position
            self.pending_cursor_tap_x = None;
            self.pending_cursor_tap_y = None;
        }
        
        // Stop dragging and selecting
        self.dragging = false;
        self.last_drag_sample_time = None;
        self.selecting = false;
        // Reset touch tracking
        self.touch_started_on_keyboard = false;
        if self.trackpad_active && !self.trackpad_moved && !self.trackpad_selecting {
            let clear_sel = self.python_viewport.is_none()
                || (self.python_viewport.is_some() && self.py_input_focused);
            if clear_sel {
                self.selection_start = None;
                self.selection_end = None;
            }
        }

        // Tap on OSK trackpad strip (no drag / not word-selection): synthesize pointer at laser so Python
        // `Text` widgets can swap focus without moving into the upper frame.
        if self.python_viewport.is_some()
            && self.py_input_focused
            && state.keyboard.onscreen.is_trackpad_mode()
            && self.trackpad_active
            && !self.trackpad_moved
            && !self.trackpad_selecting
        {
            if let (Some(lx), Some(ly)) = (self.trackpad_laser_x, self.trackpad_laser_y) {
                state.embed_synthetic_click_screen = Some((lx, ly));
                state.embed_last_plain_click_screen = Some((lx, ly));
            }
        }

        // Clear trackpad tracking (but keep laser visible)
        self.trackpad_active = false;
        self.trackpad_selecting = false;
        self.trackpad_moved = false;
        self.trackpad_last_mouse_x = None;
        self.trackpad_last_mouse_y = None;
        
        // Clear temp trackpad initial position tracking
        self.temp_trackpad_initial_x = None;
        self.temp_trackpad_initial_y = None;
    }
    
    fn on_key_shortcut(&mut self, state: &mut EngineState, shortcut: ShortcutAction) {
        self.apply_keyboard_shortcut(shortcut, state);
    }
}

impl TextApp {
    pub(crate) fn apply_keyboard_shortcut(&mut self, shortcut: ShortcutAction, state: &mut EngineState) {
        if self.python_viewport.is_some() && !self.py_allow_shortcuts {
            return;
        }
        let key_type = match shortcut {
            ShortcutAction::Copy => KeyType::Copy,
            ShortcutAction::Cut => KeyType::Cut,
            ShortcutAction::Paste => KeyType::Paste,
            ShortcutAction::SelectAll => KeyType::SelectAll,
            ShortcutAction::Undo => KeyType::Undo,
            ShortcutAction::Redo => KeyType::Redo,
        };
        self.handle_action_key(key_type, state);
    }

    /// Selection + trackpad laser for [`xos.ui._text_render`] (fields stay crate-private otherwise).
    pub fn ui_peek_overlay(&self) -> (Option<usize>, Option<usize>, Option<(f32, f32)>) {
        let laser = if self.python_viewport.is_some() && !self.py_input_focused {
            None
        } else {
            self.trackpad_laser_x.zip(self.trackpad_laser_y)
        };
        (self.selection_start, self.selection_end, laser)
    }

    /// Cursor + selection state used by collaborative wrappers (e.g. text mesh).
    pub fn shared_selection_state(&self) -> (usize, Option<usize>, Option<usize>) {
        (self.cursor_position, self.selection_start, self.selection_end)
    }

    /// Applies externally synchronized cursor/selection with bounds clamping.
    pub fn apply_shared_selection_state(
        &mut self,
        cursor_position: usize,
        selection_start: Option<usize>,
        selection_end: Option<usize>,
    ) {
        let text_len = self.text_rasterizer.text.chars().count();
        self.cursor_position = cursor_position.min(text_len);
        self.selection_start = selection_start.map(|v| v.min(text_len));
        self.selection_end = selection_end.map(|v| v.min(text_len));
    }

    /// Cursor / layout → full-frame pixel position (Python embed [`TextViewportMetrics::draw_x`] / `draw_y`).
    fn initialize_laser_at_cursor(&mut self, state: &EngineState) {
        let vp = self.viewport_metrics(state);
        let (cursor_x, cursor_baseline_y) = self.get_cursor_screen_position();
        let screen_x = vp.draw_x + cursor_x;
        let screen_y = cursor_baseline_y - self.scroll_y + vp.draw_y;
        self.trackpad_laser_x = Some(screen_x);
        self.trackpad_laser_y = Some(screen_y);
    }

    /// Ensure the red trackpad dot exists: embed prefers last content click, then strip-normalized Y, then caret.
    fn ensure_trackpad_laser_initialized(&mut self, state: &EngineState) {
        if self.trackpad_laser_x.is_some() && self.trackpad_laser_y.is_some() {
            return;
        }
        let shape = state.frame.shape();
        let height = shape[0] as f32;
        let width = shape[1] as f32;

        if self.python_viewport.is_some() {
            let laser_y_global_min = 0.0_f32;
            let (_, keyboard_top_y_n, _, _) = state.keyboard.onscreen.top_edge_coordinates();
            let keyboard_top_px = keyboard_top_y_n * height;
            let y_global_max_raw = if keyboard_top_px > laser_y_global_min + 2.0 {
                keyboard_top_px - 2.0
            } else {
                keyboard_top_px
            };
            let y_global_max = y_global_max_raw.max(laser_y_global_min);

            if let Some((px, py)) = state.embed_last_plain_click_screen {
                self.trackpad_laser_x = Some(px.clamp(0.0, width));
                self.trackpad_laser_y = Some(py.clamp(laser_y_global_min, y_global_max));
                return;
            }
            if state.keyboard.onscreen.is_shown() {
                let my = state.mouse.y;
                let strip_top = keyboard_top_px;
                if my >= strip_top {
                    let strip_bottom = height;
                    let denom = (strip_bottom - strip_top).max(1.0);
                    let t_strip = ((my - strip_top) / denom).clamp(0.0, 1.0);
                    let ly = laser_y_global_min + t_strip * (y_global_max - laser_y_global_min);
                    let lx = state.mouse.x.clamp(0.0, width);
                    self.trackpad_laser_x = Some(lx);
                    self.trackpad_laser_y = Some(ly);
                    return;
                }
            }
            self.initialize_laser_at_cursor(state);
            return;
        }

        self.initialize_laser_at_cursor(state);
    }
    
    /// Auto-scroll to keep cursor visible on screen
    fn ensure_cursor_visible(&mut self, content_height: f32) {
        let (_, cursor_baseline_y) = self.get_cursor_screen_position();
        let line_height = self.text_rasterizer.ascent + self.text_rasterizer.descent.abs() + self.text_rasterizer.line_gap;

        // Match scroll limits in [`tick`]: requesting past `natural_max` fights elastic clamps → jitter typing at EOF.
        let doc_bottom = self.document_bottom_y_px();
        let natural_max = (doc_bottom - content_height).max(0.0);

        // Calculate cursor top and bottom in text coordinates
        let cursor_top = cursor_baseline_y - self.text_rasterizer.ascent;
        let cursor_bottom = cursor_baseline_y + self.text_rasterizer.descent;

        // Calculate visible range in text coordinates
        let visible_top = self.scroll_y;
        let visible_bottom = self.scroll_y + content_height;

        // Add padding to make scrolling feel more natural
        // Use full line height for top padding, and 1.5x line height for bottom padding to ensure full character visibility
        let top_padding = line_height * 0.5;
        let bottom_padding = line_height * 1.5;

        // If cursor is above visible area, scroll up
        if cursor_top < visible_top + top_padding {
            self.drag_scroll_momentum = 0.0;
            self.wheel_accel_target = 0.0;
            self.wheel_last_activity = None;
            self.scroll_target = (cursor_top - top_padding).max(0.0).min(natural_max);
            self.scroll_y = self.scroll_target;
        }
        // If cursor is below visible area, scroll down
        else if cursor_bottom > visible_bottom - bottom_padding {
            self.drag_scroll_momentum = 0.0;
            self.wheel_accel_target = 0.0;
            self.wheel_last_activity = None;
            self.scroll_target = (cursor_bottom + bottom_padding - content_height)
                .max(0.0)
                .min(natural_max);
            self.scroll_y = self.scroll_target;
        }
    }
    
    fn move_cursor_left(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
        }
    }

    fn move_cursor_right(&mut self) {
        let text_len = self.text_rasterizer.text.chars().count();
        if self.cursor_position < text_len {
            self.cursor_position += 1;
        }
    }

    fn move_cursor_up(&mut self) {
        // Find current line
        let line_idx_opt = self.text_rasterizer.lines.iter()
            .enumerate()
            .find(|(_, line)| {
                line.start_index <= self.cursor_position && self.cursor_position <= line.end_index
            })
            .map(|(idx, _)| idx);
        
        if let Some(line_idx) = line_idx_opt {
            if line_idx > 0 {
                // Move to previous line
                let prev_line = &self.text_rasterizer.lines[line_idx - 1];
                
                // Find current x position in current line
                let current_x = if let Some(char_at_cursor) = self.text_rasterizer.characters.iter()
                    .find(|c| c.char_index == self.cursor_position) {
                    char_at_cursor.x
                } else if let Some(last_in_line) = self.text_rasterizer.characters.iter()
                    .filter(|c| c.line_index == line_idx)
                    .last() {
                    last_in_line.x + last_in_line.metrics.advance_width
                } else {
                    0.0
                };
                
                // Find character in previous line closest to current_x
                let mut best_char_index = prev_line.end_index;
                let mut min_distance = f32::MAX;
                
                for character in self.text_rasterizer.characters.iter()
                    .filter(|c| c.line_index == line_idx - 1) {
                    let distance = (character.x - current_x).abs();
                    if distance < min_distance {
                        min_distance = distance;
                        best_char_index = character.char_index;
                    }
                    // Also check position after this character
                    let after_distance = (character.x + character.metrics.advance_width - current_x).abs();
                    if after_distance < min_distance {
                        min_distance = after_distance;
                        best_char_index = character.char_index + 1;
                    }
                }
                
                self.cursor_position = best_char_index.min(prev_line.end_index);
            } else {
                // Already at first line, move to start
                self.cursor_position = 0;
            }
        }
    }

    fn move_cursor_down(&mut self) {
        // Find current line
        let line_idx_opt = self.text_rasterizer.lines.iter()
            .enumerate()
            .find(|(_, line)| {
                line.start_index <= self.cursor_position && self.cursor_position <= line.end_index
            })
            .map(|(idx, _)| idx);
        
        if let Some(line_idx) = line_idx_opt {
            if line_idx < self.text_rasterizer.lines.len() - 1 {
                // Move to next line
                let next_line = &self.text_rasterizer.lines[line_idx + 1];
                
                // Find current x position in current line
                let current_x = if let Some(char_at_cursor) = self.text_rasterizer.characters.iter()
                    .find(|c| c.char_index == self.cursor_position) {
                    char_at_cursor.x
                } else if let Some(last_in_line) = self.text_rasterizer.characters.iter()
                    .filter(|c| c.line_index == line_idx)
                    .last() {
                    last_in_line.x + last_in_line.metrics.advance_width
                } else {
                    0.0
                };
                
                // Find character in next line closest to current_x
                let mut best_char_index = next_line.end_index;
                let mut min_distance = f32::MAX;
                
                for character in self.text_rasterizer.characters.iter()
                    .filter(|c| c.line_index == line_idx + 1) {
                    let distance = (character.x - current_x).abs();
                    if distance < min_distance {
                        min_distance = distance;
                        best_char_index = character.char_index;
                    }
                    // Also check position after this character
                    let after_distance = (character.x + character.metrics.advance_width - current_x).abs();
                    if after_distance < min_distance {
                        min_distance = after_distance;
                        best_char_index = character.char_index + 1;
                    }
                }
                
                self.cursor_position = best_char_index.min(next_line.end_index);
            } else {
                // Already at last line, move to end
                self.cursor_position = self.text_rasterizer.text.chars().count();
            }
        }
    }
    
    fn find_nearest_char_index(&self, text_x: f32, text_y: f32) -> usize {
        // First, find which line the tap is on
        let mut tapped_line_idx: Option<usize> = None;
        for (line_idx, line) in self.text_rasterizer.lines.iter().enumerate() {
            let line_y = line.baseline_y;
            
            // Check if tap is within this line's vertical bounds
            if text_y >= line_y - self.text_rasterizer.ascent && text_y <= line_y + self.text_rasterizer.descent {
                tapped_line_idx = Some(line_idx);
                break;
            }
        }
        
        // If we found a line, check if we should place cursor at end of line
        if let Some(line_idx) = tapped_line_idx {
            let line = &self.text_rasterizer.lines[line_idx];
            
            // Find characters on this line
            let chars_on_line: Vec<_> = self.text_rasterizer.characters.iter()
                .filter(|c| c.line_index == line_idx)
                .collect();
            
            if chars_on_line.is_empty() {
                // Empty line - place cursor at start of line
                return line.start_index;
            }
            
            // Find the rightmost character on this line
            if let Some(last_char) = chars_on_line.last() {
                let line_end_x = last_char.x + last_char.metrics.advance_width;
                
                // If tap is to the right of the last character, place cursor at end of line
                if text_x >= line_end_x {
                    return line.end_index;
                }
            }
        }
        
        // Otherwise, find nearest character
        let mut nearest_char_index = self.text_rasterizer.text.chars().count();
        let mut min_distance_sq = f32::MAX;
        
        for character in &self.text_rasterizer.characters {
            let char_center_x = character.x + character.width / 2.0;
            let char_center_y = character.y + character.height / 2.0;
            
            let dx = text_x - char_center_x;
            let dy = text_y - char_center_y;
            let distance_sq = dx * dx + dy * dy;
            
            // Check if tap is before this character horizontally
            if text_x < character.x && character.line_index == 0 {
                // Tap is before this character, cursor should be at this character's index
                if distance_sq < min_distance_sq {
                    min_distance_sq = distance_sq;
                    nearest_char_index = character.char_index;
                }
            } else if distance_sq < min_distance_sq {
                min_distance_sq = distance_sq;
                // If tap is to the right of character center, cursor goes after it
                if text_x > char_center_x {
                    nearest_char_index = character.char_index + 1;
                } else {
                    nearest_char_index = character.char_index;
                }
            }
        }
        
        nearest_char_index.min(self.text_rasterizer.text.chars().count())
    }
    
    fn save_undo_state(&mut self) {
        ui_text_edit::save_undo_state(
            &mut self.undo_stack,
            &mut self.redo_stack,
            &self.text_rasterizer.text,
            self.cursor_position,
        );
    }
    
    /// Get the cursor position in text coordinates (x, y)
    fn get_cursor_screen_position(&self) -> (f32, f32) {
        // Find cursor position based on cursor_position index
        // First, find which line the cursor is on
        let line_info_with_idx = self.text_rasterizer.lines.iter()
            .enumerate()
            .find(|(_, line)| {
                line.start_index <= self.cursor_position && self.cursor_position <= line.end_index
            });
        
        if let Some((line_idx, line)) = line_info_with_idx {
            // Found the line - check if there are characters in this line
            let chars_in_line: Vec<_> = self.text_rasterizer.characters.iter()
                .filter(|c| c.line_index == line_idx)
                .collect();
            
            if chars_in_line.is_empty() {
                // Empty line — X from layout (e.g. horizontal centering).
                (
                    self.text_rasterizer.line_leading_caret_x(line_idx),
                    line.baseline_y,
                )
            } else {
                // Line has characters - find the appropriate x position
                // Check if cursor is at the start of the line
                if self.cursor_position == line.start_index {
                    (
                        self.text_rasterizer.line_leading_caret_x(line_idx),
                        line.baseline_y,
                    )
                } else {
                    // Find character at or before cursor position
                    let mut found_char = None;
                    let mut char_after = None;
                    
                    for character in self.text_rasterizer.characters.iter() {
                        if character.char_index == self.cursor_position {
                            found_char = Some(character);
                            break;
                        } else if character.char_index > self.cursor_position && character.line_index == line_idx {
                            char_after = Some(character);
                            break;
                        }
                    }
                    
                    if let Some(char_at_cursor) = found_char {
                        // Cursor is before this character
                        (char_at_cursor.x, line.baseline_y)
                    } else if let Some(char_after_cursor) = char_after {
                        // Cursor is before this character (on same line)
                        (char_after_cursor.x, line.baseline_y)
                    } else {
                        // Cursor is at end of line - find last character's end position
                        if let Some(last_in_line) = chars_in_line.last() {
                            (last_in_line.x + last_in_line.metrics.advance_width, line.baseline_y)
                        } else {
                            (
                                self.text_rasterizer.line_leading_caret_x(line_idx),
                                line.baseline_y,
                            )
                        }
                    }
                }
            }
        } else if self.cursor_position == 0 {
            // Cursor at very start (before any lines)
            if let Some(first_line) = self.text_rasterizer.lines.first() {
                (self.text_rasterizer.line_leading_caret_x(0), first_line.baseline_y)
            } else {
                (0.0, self.text_rasterizer.ascent)
            }
        } else if self.cursor_position >= self.text_rasterizer.text.chars().count() {
            // Cursor at end of text
            if let Some(last_line) = self.text_rasterizer.lines.last() {
                // Find the line index
                let last_line_idx = self.text_rasterizer.lines.len() - 1;
                // Check if last line has characters
                let chars_in_last_line: Vec<_> = self.text_rasterizer.characters.iter()
                    .filter(|c| c.line_index == last_line_idx)
                    .collect();
                
                if chars_in_last_line.is_empty() {
                    (
                        self.text_rasterizer.line_leading_caret_x(last_line_idx),
                        last_line.baseline_y,
                    )
                } else if let Some(last_char) = chars_in_last_line.last() {
                    (last_char.x + last_char.metrics.advance_width, last_line.baseline_y)
                } else {
                    (
                        self.text_rasterizer.line_leading_caret_x(last_line_idx),
                        last_line.baseline_y,
                    )
                }
            } else if let Some(last) = self.text_rasterizer.characters.last() {
                (last.x + last.metrics.advance_width, self.text_rasterizer.lines.last().map_or(self.text_rasterizer.ascent, |line| line.baseline_y))
            } else {
                (0.0, self.text_rasterizer.ascent)
            }
        } else {
            (0.0, self.text_rasterizer.ascent)
        }
    }
    
    /// Delete the current selection and return true if a selection was deleted
    fn delete_selection(&mut self) -> bool {
        ui_text_edit::delete_selection(
            &mut self.text_rasterizer.text,
            &mut self.cursor_position,
            &mut self.selection_start,
            &mut self.selection_end,
        )
    }
    
    fn handle_action_key(&mut self, action: KeyType, state: &mut EngineState) {
        let content_height = self.viewport_metrics(state).visible_h;

        match action {
            KeyType::Mouse => {
                // Mouse/trackpad toggle is handled by the keyboard itself
                // Just clear any active state here
                self.trackpad_active = false;
                self.trackpad_selecting = false;
            }
            KeyType::Copy => {
                if self.python_viewport.is_some() && !self.py_allow_copypaste {
                    return;
                }
                if let Some(selected_text) = ui_text_edit::copy_selection(
                    &self.text_rasterizer.text,
                    self.selection_start,
                    self.selection_end,
                ) {
                    self.clipboard_content = selected_text.clone();
                    let _ = clipboard::set_contents(&selected_text);
                }
            }
            KeyType::Cut => {
                if self.python_viewport.is_some() && !self.py_allow_copypaste {
                    return;
                }
                if let Some(selected_text) = ui_text_edit::copy_selection(
                    &self.text_rasterizer.text,
                    self.selection_start,
                    self.selection_end,
                ) {
                    self.save_undo_state();
                    self.clipboard_content = selected_text.clone();
                    let _ = clipboard::set_contents(&selected_text);
                    let _ = ui_text_edit::delete_selection(
                        &mut self.text_rasterizer.text,
                        &mut self.cursor_position,
                        &mut self.selection_start,
                        &mut self.selection_end,
                    );
                    self.ensure_cursor_visible(content_height);
                }
            }
            KeyType::Paste => {
                if self.python_viewport.is_some() && !self.py_allow_copypaste {
                    return;
                }
                self.save_undo_state();
                if ui_text_edit::paste_at_cursor(
                    &mut self.text_rasterizer.text,
                    &mut self.cursor_position,
                    &mut self.selection_start,
                    &mut self.selection_end,
                    &self.clipboard_content,
                )
                .is_some()
                {
                    self.ensure_cursor_visible(content_height);
                }
            }
            KeyType::SelectAll => {
                if self.python_viewport.is_some() && !self.py_allow_shortcuts {
                    return;
                }
                ui_text_edit::select_all_toggle(
                    &self.text_rasterizer.text,
                    &mut self.cursor_position,
                    &mut self.selection_start,
                    &mut self.selection_end,
                );
            }
            KeyType::Undo => {
                if self.python_viewport.is_some() && !self.py_allow_shortcuts {
                    return;
                }
                if let Some((text, cursor)) = self.undo_stack.pop() {
                    // Save current state to redo stack
                    self.redo_stack.push((self.text_rasterizer.text.clone(), self.cursor_position));
                    // Restore previous state
                    self.text_rasterizer.text = text;
                    self.cursor_position = cursor;
                    // Clear selection
                    self.selection_start = None;
                    self.selection_end = None;
                    self.ensure_cursor_visible(content_height);
                }
            }
            KeyType::Redo => {
                if self.python_viewport.is_some() && !self.py_allow_shortcuts {
                    return;
                }
                if let Some((text, cursor)) = self.redo_stack.pop() {
                    // Save current state to undo stack
                    self.undo_stack.push((self.text_rasterizer.text.clone(), self.cursor_position));
                    // Restore redone state
                    self.text_rasterizer.text = text;
                    self.cursor_position = cursor;
                    // Clear selection
                    self.selection_start = None;
                    self.selection_end = None;
                    self.ensure_cursor_visible(content_height);
                }
            }
            _ => {}
        }
    }
}
